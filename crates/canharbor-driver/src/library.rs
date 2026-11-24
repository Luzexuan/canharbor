// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit, process-resident native libraries. No implicit search or unloading.
use canharbor_platform::files;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, c_void},
    io,
    path::{Path, PathBuf},
    ptr::NonNull,
    sync::{Arc, Mutex, OnceLock},
};
#[cfg(unix)]
use std::{ffi::CString, os::unix::ffi::OsStrExt};
const MAX_LIBRARIES: usize = 16;
static LOADED: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Library>>>> = OnceLock::new();
pub struct Library {
    source: canharbor_core::measurement::SourceEvidence,
    handle: NonNull<c_void>,
    path: PathBuf,
    identity: (u64, u64),
    claims: Mutex<BTreeSet<u32>>,
}
// SAFETY: native loader handles are process-wide and thread-safe. Function calls
// retain their own ABI/threading requirements; resolving a symbol does not call it.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}
#[cfg(unix)]
fn loader_error() -> io::Error {
    // SAFETY: dlerror returns NULL or a NUL-terminated thread-local diagnostic.
    let message = unsafe {
        let pointer = libc::dlerror();
        if pointer.is_null() {
            "native loader failed".into()
        } else {
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        }
    };
    io::Error::other(message)
}
impl Library {
    /// Load only a caller-selected absolute library path. Images stay resident
    /// because foreign background threads may survive a failed teardown.
    ///
    /// # Safety
    /// The image and its dependencies must be trusted native code. Its constructors
    /// must be safe to run in this process. This is not a sandbox for plugins.
    pub unsafe fn load(path: &Path) -> io::Result<Arc<Self>> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native library path must be absolute",
            ));
        }
        let path = path.canonicalize()?;
        let selected = files::open_regular(&path)?;
        let before = files::stamp(&selected)?;
        let mut loaded = LOADED
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| io::Error::other("native library cache unavailable"))?;
        if let Some(library) = loaded.get(&path) {
            return Ok(library.clone());
        }
        if loaded.len() >= MAX_LIBRARIES {
            return Err(io::Error::other("native library limit reached"));
        }
        let identity = before.identity();
        if let Some(library) = loaded
            .values()
            .find(|library| library.identity == identity)
            .cloned()
        {
            loaded.insert(path, library.clone());
            return Ok(library);
        }
        let mut source = source_digest(&path);
        #[cfg(unix)]
        let handle = {
            let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "native library path contains NUL",
                )
            })?;
            // SAFETY: the caller authorizes constructors from this selected image.
            NonNull::new(unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) })
                .ok_or_else(loader_error)?
        };
        #[cfg(windows)]
        let handle = {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::System::LibraryLoader::*;
            let mut name: Vec<u16> = path.as_os_str().encode_wide().collect();
            if name.contains(&0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "native library path contains NUL",
                ));
            }
            name.push(0);
            // SAFETY: an absolute, terminated UTF-16 path to the caller-selected image.
            // Dependencies are searched only alongside it and in the system directory.
            NonNull::new(unsafe {
                LoadLibraryExW(
                    name.as_ptr(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            })
            .ok_or_else(io::Error::last_os_error)?
        };
        if !files::stamp(&selected).is_ok_and(|after| after == before)
            || !files::open_regular(&path)
                .and_then(|file| files::stamp(&file))
                .is_ok_and(|after| after == before)
        {
            source.sha256 = None;
            source.bytes = None;
            source.status = "file_changed_during_load".into();
        }
        let library = Arc::new(Self {
            source,
            handle,
            path: path.clone(),
            identity,
            claims: Mutex::new(BTreeSet::new()),
        });
        loaded.insert(path, library.clone());
        Ok(library)
    }
    /// Reserve selected handles before configuration can mutate foreign globals.
    pub fn claim(self: &Arc<Self>, keys: &[u32]) -> io::Result<Claim> {
        let unique = keys.iter().copied().collect::<BTreeSet<_>>();
        if keys.is_empty() || keys.len() > 32 || unique.len() != keys.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid native channel selection",
            ));
        }
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| io::Error::other("native channel ownership unavailable"))?;
        if unique.iter().any(|key| claims.contains(key)) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "native channel is already owned",
            ));
        }
        claims.extend(unique.iter().copied());
        Ok(Claim {
            library: self.clone(),
            keys: unique,
        })
    }
    pub fn source_evidence(&self) -> canharbor_core::measurement::SourceEvidence {
        self.source.clone()
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Resolve a function from the selected image, retaining this library while
    /// the returned function pointer is in use.
    ///
    /// # Safety
    /// `T` must be the exact native function-pointer type for this symbol. Calls
    /// must satisfy that API's pointer, ownership, and threading contracts.
    pub unsafe fn symbol<T: Copy>(&self, name: &CStr) -> io::Result<T> {
        if std::mem::size_of::<T>() != std::mem::size_of::<*mut c_void>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "symbol type must be pointer-sized",
            ));
        }
        // SAFETY: the image is pinned, name is terminated, and the caller supplies
        // the exact function type. POSIX supports converting dlsym function pointers.
        #[cfg(unix)]
        unsafe {
            libc::dlerror();
            let pointer = libc::dlsym(self.handle.as_ptr(), name.as_ptr());
            if pointer.is_null() {
                return Err(loader_error());
            }
            Ok(std::mem::transmute_copy(&pointer))
        }
        #[cfg(windows)]
        unsafe {
            // SAFETY: the selected image stays resident and name is NUL terminated.
            let function = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                self.handle.as_ptr(),
                name.as_ptr().cast(),
            )
            .ok_or_else(io::Error::last_os_error)?;
            Ok(std::mem::transmute_copy(&function))
        }
    }
}

/// Releases only this adapter's Rust ownership reservation, after driver cleanup.
pub struct Claim {
    library: Arc<Library>,
    keys: BTreeSet<u32>,
}
impl Drop for Claim {
    fn drop(&mut self) {
        if let Ok(mut claims) = self.library.claims.lock() {
            for key in &self.keys {
                claims.remove(key);
            }
        }
    }
}

fn source_digest(path: &Path) -> canharbor_core::measurement::SourceEvidence {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut source = canharbor_core::measurement::SourceEvidence {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .take(128)
            .collect(),
        sha256: None,
        bytes: None,
        status: "unavailable".into(),
    };
    let result = (|| -> io::Result<(u64, String)> {
        let mut f = files::open_regular(path)?;
        let before = files::stamp(&f)?;
        if before.bytes > 512 * 1024 * 1024 {
            return Err(io::Error::other("source size/type bound"));
        }
        let mut hasher = Sha256::new();
        let mut bytes = 0;
        let mut buffer = [0u8; 65536];
        loop {
            let n = f.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            if bytes > before.bytes {
                return Err(io::Error::other("source grew"));
            }
            hasher.update(&buffer[..n]);
        }
        if bytes != before.bytes || before != files::stamp(&f)? {
            return Err(io::Error::other("source changed"));
        }
        Ok((
            bytes,
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        ))
    })();
    if let Ok((bytes, hash)) = result {
        source.bytes = Some(bytes);
        source.sha256 = Some(hash);
        source.status =
            "sha256_selected_disk_image_at_load; dependencies_and_memory_unverified".into();
    }
    source
}

#[cfg(all(test, unix))]
mod source_tests {
    use super::*;
    #[test]
    fn disk_source_hash_has_a_golden_vector_and_does_not_follow_symlinks_or_block_on_fifo() {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let root = std::env::temp_dir().join(format!(
            "ch-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let path = root.join("selected.dylib");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        f.write_all(b"abc").unwrap();
        drop(f);
        let source = source_digest(&path);
        assert_eq!(source.bytes, Some(3));
        assert_eq!(
            source.sha256.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        let link = root.join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(source_digest(&link).sha256.is_none());
        let fifo = root.join("fifo");
        let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: a private test-only FIFO path and valid mode, no device I/O.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(source_digest(&fifo).sha256.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod portable_source_tests {
    #[test]
    fn selected_source_has_an_exact_digest_and_rejects_directories() {
        let root = std::env::temp_dir().join(format!(
            "canharbor-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("image");
        std::fs::write(&path, b"abc").unwrap();
        let source = super::source_digest(&path);
        assert_eq!(source.bytes, Some(3));
        assert_eq!(
            source.sha256.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert!(super::source_digest(&root).sha256.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
