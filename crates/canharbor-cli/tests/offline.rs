// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::Value;
use std::process::Command;
#[test]
fn inspect_needs_no_service_and_rejects_semantically_invalid_frames() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../api/capture-fixtures.json")).unwrap();
    let input = test_temp_dir().join(format!(
        "ch-inspect-{}.ndjson",
        uuid::Uuid::new_v4().simple()
    ));
    for (records, code) in [
        (&fixtures["valid"][0]["records"], 0),
        (&fixtures["invalid"][0]["records"], 2),
    ] {
        let data: String = records
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!("{r}\n"))
            .collect();
        std::fs::write(&input, data).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_canharbor"))
            .args([
                "--runtime-dir",
                "/tmp/canharbor-absent-offline",
                "capture",
                "inspect",
            ])
            .arg(&input)
            .args(["--limit", "1"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if code == 0 {
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["frames"], "1");
            assert_eq!(report["preview"].as_array().unwrap().len(), 1);
        } else {
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(input).unwrap();
}

#[test]
fn export_writes_a_candump_log_once_and_leaves_nothing_after_invalid_input() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../api/capture-fixtures.json")).unwrap();
    let root = test_temp_dir().join(format!("ch-export-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir(&root).unwrap();
    let input = root.join("capture.ndjson");
    let output = root.join("capture.log");
    let export = |records: &Value| {
        let data: String = records
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!("{r}\n"))
            .collect();
        std::fs::write(&input, data).unwrap();
        Command::new(env!("CARGO_BIN_EXE_canharbor"))
            .args([
                "--runtime-dir",
                "/tmp/canharbor-absent-offline",
                "capture",
                "export",
            ])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    let rejected = export(&fixtures["invalid"][0]["records"]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(rejected.stdout.is_empty());
    assert!(!output.exists());

    let written = export(&fixtures["valid"][0]["records"]);
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    let summary: Value = serde_json::from_slice(&written.stdout).unwrap();
    assert_eq!(summary["frames"], "1");
    assert_eq!(summary["complete"], true);
    let log = std::fs::read_to_string(&output).unwrap();
    assert_eq!(log, "(18446744073.709551) can0 123#0102030405060708\n");

    // A second run must not replace or remove the existing log.
    let repeated = export(&fixtures["valid"][0]["records"]);
    assert!(!repeated.status.success());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), log);
    std::fs::remove_dir_all(root).unwrap();
}

/// The ASC and BLF logs are read back with python-can, the parser these
/// exports were written against. Skipped when the interpreter or python-can
/// is missing.
#[test]
fn asc_and_blf_exports_are_read_back_by_python_can() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../api/capture-fixtures.json")).unwrap();
    let base = &fixtures["valid"][0]["records"];
    let frames = [
        (
            "classic_data",
            0x123,
            false,
            "0102030405060708",
            8,
            "rx",
            false,
            false,
        ),
        ("classic_data", 0x1abc_def0, true, "", 0, "tx", false, false),
        ("classic_remote", 0x7ff, false, "", 4, "rx", false, false),
        ("classic_remote", 0x12, false, "", 0, "tx", false, false),
        ("fd_data", 0x12, false, "0a", 1, "rx", false, false),
        (
            "fd_data",
            0x1fff_ffff,
            true,
            &"aa".repeat(64),
            15,
            "tx",
            true,
            true,
        ),
        (
            "fd_data",
            0x100,
            false,
            &"bb".repeat(12),
            9,
            "rx",
            true,
            false,
        ),
        // Last: python-can's ASC reader takes a CAN FD line without data for
        // a remote frame, so the ASC comparison leaves this frame out.
        ("fd_data", 0x101, false, "", 0, "tx", false, false),
    ];
    let mut data = format!("{}\n", base[0]);
    for (i, (kind, id, extended, payload, dlc, direction, brs, esi)) in frames.iter().enumerate() {
        let mut e = base[1].clone();
        e["event"]["sequence"] = (i + 1).to_string().into();
        e["event"]["host_monotonic_ns"] = (5_000_000_000 + i as u64 * 1_234_567_000)
            .to_string()
            .into();
        e["event"]["raw_dlc"] = Value::Null;
        e["event"]["direction"] = (*direction).into();
        e["event"]["frame"] = serde_json::json!({
            "kind": kind, "id": id, "extended": extended, "brs": brs,
            "esi": esi, "data_hex": payload, "dlc": dlc
        });
        data.push_str(&format!("{e}\n"));
    }
    let root = test_temp_dir().join(format!("ch-log-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir(&root).unwrap();
    let input = root.join("capture.ndjson");
    std::fs::write(&input, data).unwrap();
    let python = std::env::var_os("CANHARBOR_TEST_PYTHON").unwrap_or_else(|| "python3".into());
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../python/tests/log_export.py");
    for format in ["asc", "blf"] {
        let output = root.join(format!("capture.{format}"));
        let written = Command::new(env!("CARGO_BIN_EXE_canharbor"))
            .args([
                "--runtime-dir",
                "/tmp/canharbor-absent-offline",
                "capture",
                "export",
                "--format",
                format,
                "--channel-number",
                "3",
            ])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            written.status.success(),
            "{}",
            String::from_utf8_lossy(&written.stderr)
        );
        let summary: Value = serde_json::from_slice(&written.stdout).unwrap();
        assert_eq!(summary["frames"], "8");

        let read = match Command::new(&python).arg(&script).arg(&output).output() {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("skipped: no Python interpreter");
                break;
            }
            Err(error) => panic!("{error}"),
        };
        assert!(
            read.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&read.stderr)
        );
        if read.stdout.starts_with(b"skipped") {
            eprintln!("{}", String::from_utf8_lossy(&read.stdout));
            break;
        }
        let messages: Value = serde_json::from_slice(&read.stdout).unwrap();
        let compared = if format == "asc" { 7 } else { 8 };
        let expected: Vec<Value> = frames[..compared]
            .iter()
            .enumerate()
            .map(
                |(i, (kind, id, extended, payload, dlc, direction, brs, esi))| {
                    serde_json::json!({
                        "us": i as u64 * 1_234_567,
                        "id": id,
                        "extended": extended,
                        "rx": *direction == "rx",
                        "remote": *kind == "classic_remote",
                        "fd": *kind == "fd_data",
                        "brs": brs,
                        "esi": esi,
                        // python-can reports the payload length, or the requested
                        // length of a remote frame.
                        "dlc": if *kind == "classic_remote" { *dlc } else { payload.len() / 2 },
                        "data": payload,
                        // python-can numbers channels from zero.
                        "channel": 2,
                    })
                },
            )
            .collect();
        assert_eq!(
            messages.as_array().unwrap()[..compared],
            expected[..],
            "{format}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

fn test_temp_dir() -> std::path::PathBuf {
    if cfg!(windows) {
        std::env::temp_dir()
    } else {
        std::path::PathBuf::from("/tmp")
    }
}

#[test]
fn non_regular_capture_is_an_input_error() {
    let root = test_temp_dir().join(format!("ch-nonregular-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let inputs = vec![root.clone()];
    #[cfg(unix)]
    let mut inputs = inputs;
    #[cfg(unix)]
    {
        let fifo = root.join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        inputs.push(fifo);
    }
    for input in inputs {
        let out = Command::new(env!("CARGO_BIN_EXE_canharbor"))
            .args(["capture", "inspect"])
            .arg(input)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
    }
    std::fs::remove_dir_all(root).unwrap();
}
