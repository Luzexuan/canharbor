// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit startup selection, validated before any transport is opened.
use canharbor_core::{Backend, CloseSummary};
use canharbor_gsusb::usb::Candidate;
use serde::Deserialize;
#[cfg(feature = "slcan")]
use std::path::PathBuf;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    future::Future,
    io::Read,
    path::Path,
    sync::Arc,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    adapters: Vec<Adapter>,
}

#[cfg(feature = "slcan")]
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Lawicel,
    CanableFd,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum Adapter {
    GsUsb {
        name: String,
        vid_pid: String,
        index: Option<usize>,
        serial: Option<String>,
        interface: u8,
        alternate: u8,
    },
    #[cfg(feature = "slcan")]
    Slcan {
        name: String,
        path: PathBuf,
        serial_baud: u32,
        profile: Profile,
        #[serde(default)]
        polling: bool,
    },
}

impl Adapter {
    fn name(&self) -> &str {
        match self {
            Self::GsUsb { name, .. } => name,
            #[cfg(feature = "slcan")]
            Self::Slcan { name, .. } => name,
        }
    }
}

pub fn load(path: &Path) -> Result<Vec<Adapter>> {
    let file = canharbor_platform::files::open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(crate::status::input("hardware manifest exceeds 64 KiB"));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| crate::status::input(format!("invalid hardware manifest: {error}")))?;
    if manifest.version != 1 || manifest.adapters.is_empty() {
        return Err(crate::status::input(
            "hardware manifest requires version 1 and 1..32 adapters",
        ));
    }
    validate(&manifest.adapters)?;
    Ok(manifest.adapters)
}

pub fn validate(adapters: &[Adapter]) -> Result<()> {
    if adapters.len() > 32 {
        return Err(crate::status::input(
            "at most 32 hardware adapters are allowed",
        ));
    }
    let mut names = BTreeSet::new();
    let mut selectors = BTreeSet::new();
    for adapter in adapters {
        let name = adapter.name();
        if name.is_empty()
            || name.len() > 31
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !names.insert(name)
        {
            return Err(crate::status::input(
                "adapter names/channel namespaces must be unique 1..31 ASCII letters, digits, '-' or '_'",
            ));
        }
        let selector = match adapter {
            Adapter::GsUsb {
                vid_pid,
                index,
                serial,
                ..
            } => {
                let id = crate::parse_vid_pid(vid_pid)
                    .map_err(|error| crate::status::input(error.to_string()))?;
                if index.is_some() == serial.is_some()
                    || serial.as_ref().is_some_and(|s| {
                        s.is_empty() || s.len() > 256 || s.chars().any(char::is_control)
                    })
                {
                    return Err(crate::status::input(
                        "gs_usb requires exactly one index or nonempty serial (at most 256 bytes)",
                    ));
                }
                format!(
                    "usb:{:04x}:{:04x}:{index:?}:{serial:?}",
                    id.vendor, id.product
                )
            }
            #[cfg(feature = "slcan")]
            Adapter::Slcan {
                path,
                serial_baud,
                profile,
                polling,
                ..
            } => {
                if ![9600, 19200, 38400, 57600, 115200, 230400].contains(serial_baud)
                    || *polling && matches!(profile, Profile::CanableFd)
                {
                    return Err(crate::status::input(
                        "invalid SLCAN baud/profile/polling settings",
                    ));
                }
                serial_selector(path).map_err(crate::status::input)?
            }
        };
        if !selectors.insert(selector) {
            return Err(crate::status::input("duplicate hardware selector"));
        }
    }
    Ok(())
}

#[cfg(all(feature = "slcan", windows))]
fn serial_selector(path: &Path) -> std::result::Result<String, String> {
    let name = path.to_str().ok_or("expected an explicit COM port")?;
    let name = name.strip_prefix(r"\\.\").unwrap_or(name);
    let number = name
        .get(3..)
        .filter(|_| name.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("COM")))
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| *n > 0)
        .ok_or("expected an explicit COM port, for example COM3 or \\\\.\\COM10")?;
    Ok(format!("serial:COM{number}"))
}

#[cfg(all(feature = "slcan", unix))]
fn serial_selector(path: &Path) -> std::result::Result<String, String> {
    if path.as_os_str().is_empty() {
        return Err("SLCAN path must not be empty".into());
    }
    Ok(format!("serial:{}", path.display()))
}

struct Prepared {
    adapter: Adapter,
    candidate: Option<Candidate>,
}

// Discovery reads USB descriptors only. Resolve all aliases from one snapshot
// per VID/PID before an open can change device state or enumeration order.
fn prepare(adapters: Vec<Adapter>) -> std::result::Result<Vec<Prepared>, (i32, String)> {
    let mut discovered = BTreeMap::new();
    let mut selected = BTreeSet::new();
    let mut selections = Vec::new();
    #[cfg(all(feature = "slcan", unix))]
    let mut serial_paths = BTreeSet::new();
    for adapter in &adapters {
        match adapter {
            Adapter::GsUsb {
                vid_pid,
                index,
                serial,
                ..
            } => {
                let id = crate::parse_vid_pid(vid_pid).map_err(|e| (2, e.to_string()))?;
                let key = (id.vendor, id.product);
                if let std::collections::btree_map::Entry::Vacant(entry) = discovered.entry(key) {
                    entry.insert(
                        canharbor_gsusb::usb::discover(&[id])
                            .map_err(|e| (4, e.to_string()))?
                            .into_iter()
                            .map(Some)
                            .collect::<Vec<_>>(),
                    );
                }
                let candidates = &discovered[&key];
                let index = select_usb(
                    candidates
                        .iter()
                        .map(|c| c.as_ref().and_then(Candidate::serial)),
                    *index,
                    serial.as_deref(),
                    &mut selected,
                    key,
                )?;
                selections.push(Some((key, index)));
            }
            #[cfg(feature = "slcan")]
            Adapter::Slcan { path, .. } => {
                #[cfg(unix)]
                if !serial_paths.insert(path.canonicalize().map_err(|e| (6, e.to_string()))?) {
                    return Err((2, "duplicate serial device path/alias".into()));
                }
                #[cfg(windows)]
                let _ = path;
                selections.push(None);
            }
        }
    }
    Ok(adapters
        .into_iter()
        .zip(selections)
        .map(|(adapter, selection)| Prepared {
            adapter,
            candidate: selection
                .and_then(|(key, index)| discovered.get_mut(&key).unwrap()[index].take()),
        })
        .collect())
}

fn select_usb<'a>(
    serials: impl Iterator<Item = Option<&'a str>>,
    index: Option<usize>,
    serial: Option<&str>,
    selected: &mut BTreeSet<((u16, u16), usize)>,
    key: (u16, u16),
) -> std::result::Result<usize, (i32, String)> {
    let serials = serials.collect::<Vec<_>>();
    let index = if let Some(serial) = serial {
        canharbor_gsusb::usb::unique_serial(serials, serial).map_err(|e| (4, e.into()))?
    } else {
        index
            .filter(|i| *i < serials.len())
            .ok_or((4, "USB candidate index is unavailable".into()))?
    };
    if !selected.insert((key, index)) {
        return Err((2, "duplicate USB device selected by index/serial".into()));
    }
    Ok(index)
}

pub async fn open(adapters: Vec<Adapter>) -> Result<Vec<Arc<dyn Backend>>> {
    validate(&adapters)?;
    let prepared = tokio::task::spawn_blocking(move || prepare(adapters))
        .await?
        .map_err(|(code, message)| {
            Box::new(crate::status::Failure::Reported { code, message }) as Box<dyn Error>
        })?;
    open_all(prepared, |prepared| async move {
        let epoch = fresh_epoch(prepared.adapter.name());
        match prepared.adapter {
            Adapter::GsUsb {
                interface,
                alternate,
                ..
            } => {
                let candidate = prepared.candidate.expect("prepared USB candidate");
                let transport =
                    tokio::task::spawn_blocking(move || candidate.open(interface, alternate))
                        .await??;
                Ok(Arc::new(
                    canharbor_gsusb::backend::GsUsbBackend::open(
                        transport,
                        u16::from(interface),
                        epoch,
                    )
                    .await?,
                ) as Arc<dyn Backend>)
            }
            #[cfg(feature = "slcan")]
            Adapter::Slcan {
                path,
                serial_baud,
                profile,
                polling,
                ..
            } => {
                let transport = tokio::task::spawn_blocking(move || {
                    canharbor_slcan::serial::Serial::open(&path, serial_baud)
                })
                .await??;
                let profile = match profile {
                    Profile::Lawicel => canharbor_slcan::Profile::Lawicel,
                    Profile::CanableFd => canharbor_slcan::Profile::CanableFd,
                };
                Ok(Arc::new(
                    canharbor_slcan::backend::SlcanBackend::open_with_profile(
                        transport, epoch, polling, profile,
                    )
                    .await?,
                ) as Arc<dyn Backend>)
            }
        }
    })
    .await
}

fn fresh_epoch(name: &str) -> String {
    format!("{name}-{}", uuid::Uuid::new_v4().simple())
}

async fn open_all<T, F, Fut>(items: Vec<T>, mut opener: F) -> Result<Vec<Arc<dyn Backend>>>
where
    F: FnMut(T) -> Fut,
    Fut: Future<Output = Result<Arc<dyn Backend>>>,
{
    let mut opened = Vec::new();
    let mut channels = BTreeSet::new();
    for item in items {
        let backend = match opener(item).await {
            Ok(backend) => backend,
            Err(error) => return Err(cleanup_error(&opened, error).await),
        };
        opened.push(backend.clone());
        for channel in backend.channels() {
            if !channels.insert(channel.id) {
                return Err(cleanup_error(
                    &opened,
                    crate::status::input("duplicate hardware channel"),
                )
                .await);
            }
        }
    }
    Ok(opened)
}

pub async fn cleanup_error(backends: &[Arc<dyn Backend>], error: Box<dyn Error>) -> Box<dyn Error> {
    let mut summary = CloseSummary::default();
    for backend in backends.iter().rev() {
        match backend.shutdown_report().await {
            Ok(report) => {
                summary.unknown_transmissions = summary
                    .unknown_transmissions
                    .saturating_add(report.unknown_transmissions);
                summary.failures.extend(report.failures);
            }
            Err(failure) => summary.failures.push(failure.to_string()),
        }
    }
    with_cleanup(error, summary)
}

pub fn with_cleanup(error: Box<dyn Error>, summary: CloseSummary) -> Box<dyn Error> {
    if summary.failures.is_empty() && summary.unknown_transmissions == 0 {
        return error;
    }
    Box::new(crate::status::Failure::Reported {
        code: if summary.unknown_transmissions > 0 {
            7
        } else {
            6
        },
        message: format!(
            "{error}; startup cleanup unconfirmed transmissions: {}; failures: {:?}",
            summary.unknown_transmissions, summary.failures
        ),
    })
}

pub async fn start_service(
    backends: Vec<Arc<dyn Backend>>,
    dir: &Path,
    port: u16,
    web: bool,
) -> Result<canharbor_server::local::RunningServer> {
    let registry = match canharbor_core::registry::Registry::new(backends.clone()) {
        Ok(registry) => registry,
        Err(error) => return Err(cleanup_error(&backends, Box::new(error)).await),
    };
    let runtime = canharbor_core::runtime::Runtime::new(Arc::new(registry));
    match canharbor_server::local::RunningServer::start(runtime.clone(), dir, port, web).await {
        Ok(server) => Ok(server),
        Err(error) => Err(with_cleanup(Box::new(error), runtime.shutdown().await)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canharbor_core::{BackendEvent, BackendFuture, Config, Frame, control::Permit};
    use std::sync::Mutex;
    use tokio::sync::broadcast;

    // macOS TMPDIR can exceed the Unix socket path limit.
    fn test_temp_dir() -> PathBuf {
        if cfg!(windows) {
            std::env::temp_dir()
        } else {
            PathBuf::from("/tmp")
        }
    }

    fn usb(name: &str, index: usize) -> Adapter {
        Adapter::GsUsb {
            name: name.into(),
            vid_pid: "1d50:606f".into(),
            index: Some(index),
            serial: None,
            interface: 0,
            alternate: 0,
        }
    }

    #[test]
    fn names_selectors_and_settings_are_validated_without_discovery() {
        validate(&[usb("one", 0), usb("two", 1)]).unwrap();
        for adapters in [
            vec![usb("same", 0), usb("same", 1)],
            vec![usb("one", 0), usb("two", 0)],
            vec![usb("bad:name", 0)],
            (0..33).map(|n| usb(&format!("n{n}"), n)).collect(),
        ] {
            assert!(validate(&adapters).is_err());
        }
        for value in [
            serde_json::json!({"backend":"gs_usb","name":"x","vid_pid":"zz:606f","index":0,"interface":0,"alternate":0}),
            serde_json::json!({"backend":"gs_usb","name":"x","vid_pid":"1d50:606f","interface":0,"alternate":0}),
            serde_json::json!({"backend":"gs_usb","name":"x","vid_pid":"1d50:606f","index":0,"serial":"x","interface":0,"alternate":0}),
            serde_json::json!({"backend":"gs_usb","name":"x","vid_pid":"1d50:606f","serial":"","interface":0,"alternate":0}),
        ] {
            let adapter: Adapter = serde_json::from_value(value).unwrap();
            assert!(validate(&[adapter]).is_err());
        }
    }

    #[test]
    fn usb_serial_and_index_aliases_are_rejected_before_open() {
        let serials = [Some("A"), None, Some("B")];
        let mut selected = BTreeSet::new();
        assert_eq!(
            select_usb(serials.into_iter(), None, Some("B"), &mut selected, (1, 2)).unwrap(),
            2
        );
        assert_eq!(
            select_usb(serials.into_iter(), Some(2), None, &mut selected, (1, 2))
                .unwrap_err()
                .0,
            2
        );
        assert!(
            select_usb(
                [Some("A"), Some("A")].into_iter(),
                None,
                Some("A"),
                &mut selected,
                (1, 2)
            )
            .is_err()
        );
        assert!(select_usb(serials.into_iter(), Some(3), None, &mut selected, (1, 2)).is_err());
        assert!(
            select_usb(
                serials.into_iter(),
                None,
                Some("ABSENT"),
                &mut selected,
                (1, 2)
            )
            .is_err()
        );
    }

    #[cfg(all(feature = "slcan", windows))]
    #[test]
    fn com_aliases_and_bad_settings_are_rejected() {
        let make = |name: &str, path: &str| Adapter::Slcan {
            name: name.into(),
            path: path.into(),
            serial_baud: 115200,
            profile: Profile::Lawicel,
            polling: false,
        };
        assert!(validate(&[make("a", "COM01"), make("b", r"\\.\com1")]).is_err());
        assert!(validate(&[make("a", r"\\server\COM1")]).is_err());
        let mut invalid = make("a", "COM3");
        if let Adapter::Slcan { serial_baud, .. } = &mut invalid {
            *serial_baud = 12345;
        }
        assert!(validate(&[invalid]).is_err());
        let invalid = Adapter::Slcan {
            name: "a".into(),
            path: "COM3".into(),
            serial_baud: 115200,
            profile: Profile::CanableFd,
            polling: true,
        };
        assert!(validate(&[invalid]).is_err());
    }

    #[cfg(all(feature = "slcan", unix))]
    #[test]
    fn unix_serial_aliases_are_resolved_before_open() {
        let root = std::env::temp_dir().join(format!("ch-serial-alias-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let device = root.join("device");
        let alias = root.join("alias");
        std::fs::write(&device, b"selector fixture").unwrap();
        std::os::unix::fs::symlink(&device, &alias).unwrap();
        let make = |name: &str, path| Adapter::Slcan {
            name: name.into(),
            path,
            serial_baud: 115200,
            profile: Profile::Lawicel,
            polling: false,
        };
        let adapters = vec![make("one", device), make("two", alias)];
        validate(&adapters).unwrap();
        let error = prepare(adapters).err().expect("aliases must be rejected");
        assert_eq!(error.0, 2);
        assert!(error.1.contains("duplicate serial device path/alias"));
        std::fs::remove_dir_all(root).unwrap();
    }

    struct FakeBackend {
        name: String,
        closed: Arc<Mutex<Vec<String>>>,
        unknown: u64,
        fail: bool,
    }

    impl Backend for FakeBackend {
        fn channels(&self) -> Vec<canharbor_core::Channel> {
            vec![canharbor_core::Channel {
                id: self.name.clone(),
                label: self.name.clone(),
            }]
        }
        fn subscribe(
            &self,
            _: &str,
        ) -> std::result::Result<broadcast::Receiver<Arc<BackendEvent>>, canharbor_core::Error>
        {
            panic!("startup must not subscribe")
        }
        fn check_channel(&self, _: &str) -> std::result::Result<u64, canharbor_core::Error> {
            Ok(0)
        }
        fn transmit(&self, _: &str, _: Frame) -> std::result::Result<(), canharbor_core::Error> {
            panic!("startup must not transmit")
        }
        fn configure<'a>(&'a self, _: &'a str, _: Config) -> BackendFuture<'a, ()> {
            panic!("startup must not configure a controller")
        }
        fn start<'a>(&'a self, _: &'a str, _: Permit) -> BackendFuture<'a, ()> {
            panic!("startup must not start a controller")
        }
        fn stop<'a>(&'a self, _: &'a str) -> BackendFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
        fn close<'a>(&'a self, _: &'a str) -> BackendFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
        fn shutdown_report(&self) -> BackendFuture<'_, CloseSummary> {
            Box::pin(async {
                tokio::task::yield_now().await;
                self.closed.lock().unwrap().push(self.name.clone());
                if self.fail {
                    return Err(canharbor_core::Error::Fault(
                        canharbor_core::BackendFault::StopFailed,
                    ));
                }
                Ok(CloseSummary {
                    unknown_transmissions: self.unknown,
                    failures: vec![],
                })
            })
        }
    }

    fn fake(
        name: &str,
        closed: &Arc<Mutex<Vec<String>>>,
        unknown: u64,
        fail: bool,
    ) -> Arc<dyn Backend> {
        Arc::new(FakeBackend {
            name: name.into(),
            closed: closed.clone(),
            unknown,
            fail,
        })
    }

    #[tokio::test]
    async fn third_open_failure_joins_cleanup_in_reverse_order() {
        let closed = Arc::new(Mutex::new(vec![]));
        let mut attempted = vec![];
        let result = open_all(vec!["one", "two", "three"], |name| {
            attempted.push(name);
            let backend = fake(name, &closed, 0, false);
            async move {
                if name == "three" {
                    Err(crate::status::input("third open failed"))
                } else {
                    Ok(backend)
                }
            }
        })
        .await;
        let error = result.err().expect("third open must fail");
        assert_eq!(crate::status::exit_code(error.as_ref()), 2);
        assert!(error.to_string().contains("third open failed"));
        assert_eq!(attempted, ["one", "two", "three"]);
        assert_eq!(*closed.lock().unwrap(), ["two", "one"]);
    }

    #[tokio::test]
    async fn duplicate_backend_channels_close_the_entire_opened_set() {
        let closed = Arc::new(Mutex::new(vec![]));
        let result = open_all(vec![1, 2], |_| {
            let backend = fake("duplicate", &closed, 0, false);
            async { Ok(backend) }
        })
        .await;
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("duplicate hardware channel")
        );
        assert_eq!(*closed.lock().unwrap(), ["duplicate", "duplicate"]);
    }

    #[tokio::test]
    async fn cleanup_attempts_all_backends_and_reports_uncertainty() {
        let closed = Arc::new(Mutex::new(vec![]));
        let backends = [
            fake("unknown", &closed, 3, false),
            fake("failed", &closed, 0, true),
        ];
        let error = cleanup_error(&backends, crate::status::input("original open failure")).await;
        assert_eq!(crate::status::exit_code(error.as_ref()), 7);
        assert!(error.to_string().contains("original open failure"));
        assert!(error.to_string().contains("unconfirmed transmissions: 3"));
        assert!(error.to_string().contains("StopFailed"));
        assert_eq!(*closed.lock().unwrap(), ["failed", "unknown"]);
    }

    #[tokio::test]
    async fn multiple_backends_reach_service_and_close_without_controller_io() {
        let closed = Arc::new(Mutex::new(vec![]));
        let adapters = vec![usb("one", 0), usb("two", 1)];
        validate(&adapters).unwrap();
        let backends = open_all(adapters, |adapter| {
            let backend = fake(adapter.name(), &closed, 0, false);
            async { Ok(backend) }
        })
        .await
        .unwrap();
        let root = test_temp_dir().join(format!("ch-hardware-{}", uuid::Uuid::new_v4()));
        let service = start_service(backends, &root, 0, false).await.unwrap();
        assert_eq!(
            service
                .state
                .runtime
                .channels()
                .iter()
                .map(|c| c.id.clone())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["one".into(), "two".into()])
        );
        assert!(service.shutdown().await.failures.is_empty());
        assert_eq!(closed.lock().unwrap().len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn service_start_failure_and_registry_rejection_join_cleanup() {
        let closed = Arc::new(Mutex::new(vec![]));
        let path = std::env::temp_dir().join(format!("ch-invalid-root-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"regular file, not a runtime directory").unwrap();
        assert!(
            start_service(vec![fake("one", &closed, 0, false)], &path, 0, false)
                .await
                .is_err()
        );
        assert_eq!(*closed.lock().unwrap(), ["one"]);
        assert!(
            start_service(
                vec![
                    fake("dup", &closed, 0, false),
                    fake("dup", &closed, 0, false)
                ],
                &path,
                0,
                false
            )
            .await
            .is_err()
        );
        assert_eq!(*closed.lock().unwrap(), ["one", "dup", "dup"]);
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(feature = "slcan")]
    #[tokio::test]
    async fn two_simulated_serial_backends_keep_independent_fresh_epochs() {
        struct SerialFake {
            input: std::collections::VecDeque<u8>,
            commands: Arc<Mutex<Vec<Vec<u8>>>>,
        }
        impl canharbor_slcan::serial::Transport for SerialFake {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                assert_eq!(bytes, b"V\r", "startup must only request the version");
                self.commands.lock().unwrap().push(bytes.to_vec());
                self.input.extend(b"V0234\r");
                Ok(bytes.len())
            }
            fn read(&mut self, bytes: &mut [u8], _: std::time::Duration) -> std::io::Result<usize> {
                let count = bytes.len().min(self.input.len());
                for slot in &mut bytes[..count] {
                    *slot = self.input.pop_front().unwrap();
                }
                Ok(count)
            }
        }
        let commands = Arc::new(Mutex::new(vec![]));
        let first = open_all(vec!["serial-a", "serial-b"], |name| {
            let transport = SerialFake {
                input: Default::default(),
                commands: commands.clone(),
            };
            async move {
                Ok(Arc::new(
                    canharbor_slcan::backend::SlcanBackend::open_with_profile(
                        transport,
                        fresh_epoch(name),
                        false,
                        canharbor_slcan::Profile::Lawicel,
                    )
                    .await?,
                ) as Arc<dyn Backend>)
            }
        })
        .await
        .unwrap();
        let old_id = first[0].channels()[0].id.clone();
        assert!(old_id.starts_with("slcan:serial-a-"));
        assert!(first[1].channels()[0].id.starts_with("slcan:serial-b-"));
        let root = test_temp_dir().join(format!("ch-two-serial-{}", uuid::Uuid::new_v4()));
        let service = start_service(first, &root, 0, false).await.unwrap();
        assert_eq!(service.state.runtime.channels().len(), 2);
        let summary = service.shutdown().await;
        assert!(summary.failures.is_empty());
        assert_eq!(summary.unknown_transmissions, 0);
        let transport = SerialFake {
            input: Default::default(),
            commands: commands.clone(),
        };
        let reopened = canharbor_slcan::backend::SlcanBackend::open_with_profile(
            transport,
            fresh_epoch("serial-a"),
            false,
            canharbor_slcan::Profile::Lawicel,
        )
        .await
        .unwrap();
        assert_ne!(reopened.channels()[0].id, old_id);
        assert!(
            reopened
                .shutdown_report()
                .await
                .unwrap()
                .failures
                .is_empty()
        );
        assert_eq!(*commands.lock().unwrap(), vec![b"V\r".to_vec(); 3]);
        let epoch = fresh_epoch(&"x".repeat(31));
        assert_eq!(epoch.len(), 64);
        uuid::Uuid::parse_str(&epoch[32..]).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
