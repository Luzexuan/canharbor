// SPDX-License-Identifier: AGPL-3.0-or-later
#![cfg(all(windows, feature = "dynamic"))]
use canharbor_driver::library::Library;
use std::{io, path::PathBuf, process::Command, sync::Arc};

#[test]
fn selected_windows_image_is_resident_deduplicated_and_exclusively_claimed() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../target/loader-fixtures").join(format!(
        "unicode-测试-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    let image = root.join("selected.dll");
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--crate-type", "cdylib", "--edition", "2024"])
        .arg(manifest.join("tests/fixtures/loader.rs"))
        .arg("-o")
        .arg(&image)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The cache intentionally retains the loaded fixture in Cargo's ignored target tree.
    let library = unsafe { Library::load(&image) }.unwrap();
    assert_eq!(library.path(), image.canonicalize().unwrap());
    let source = library.source_evidence();
    assert_eq!(source.bytes, Some(std::fs::metadata(&image).unwrap().len()));
    assert_eq!(source.sha256.as_ref().unwrap().len(), 64);
    let alias = root.join("hard-link.dll");
    std::fs::hard_link(&image, &alias).unwrap();
    let aliased = unsafe { Library::load(&alias) }.unwrap();
    assert!(Arc::ptr_eq(&library, &aliased));
    let answer =
        unsafe { library.symbol::<unsafe extern "C" fn() -> u32>(c"canharbor_loader_answer") }
            .unwrap();
    assert_eq!(unsafe { answer() }, 42);
    assert!(unsafe { library.symbol::<unsafe extern "C" fn()>(c"missing_symbol") }.is_err());
    assert_eq!(
        unsafe { library.symbol::<u8>(c"canharbor_loader_answer") }
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let claim = library.claim(&[7]).unwrap();
    assert_eq!(
        aliased.claim(&[7]).err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(library.claim(&[8, 8]).is_err());
    drop(claim);
    assert!(aliased.claim(&[7]).is_ok());
    assert!(unsafe { Library::load(std::path::Path::new("selected.dll")) }.is_err());
    assert!(unsafe { Library::load(&root) }.is_err());
    let invalid = root.join("invalid.dll");
    std::fs::write(&invalid, b"not a PE image").unwrap();
    assert!(unsafe { Library::load(&invalid) }.is_err());
    let x86 = root.join("wrong-architecture.dll");
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args([
            "--crate-type",
            "cdylib",
            "--edition",
            "2024",
            "--target",
            "i686-pc-windows-msvc",
        ])
        .arg(manifest.join("tests/fixtures/loader.rs"))
        .arg("-o")
        .arg(&x86)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        unsafe { Library::load(&x86) }.err().unwrap().raw_os_error(),
        Some(193)
    );
    let original_hash = source.sha256;
    let replacement = root.join("replacement.dll");
    std::fs::write(&replacement, b"replacement disk contents").unwrap();
    // Windows may refuse replacing an image that is mapped in this process.
    if let Err(error) = std::fs::rename(&replacement, &image) {
        assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
    }
    drop(aliased);
    let reloaded = unsafe { Library::load(&image) }.unwrap();
    assert!(Arc::ptr_eq(&library, &reloaded));
    assert_eq!(reloaded.source_evidence().sha256, original_hash);
    let claim = library.claim(&[7]).unwrap();
    assert!(reloaded.claim(&[7]).is_err());
    assert_eq!(unsafe { answer() }, 42);
    drop(claim);
}
