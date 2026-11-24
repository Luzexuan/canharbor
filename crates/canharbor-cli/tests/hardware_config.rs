// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{Value, json};
use std::process::{Command, Output};

fn usb(name: &str, index: usize) -> Value {
    json!({"backend":"gs_usb", "name":name, "vid_pid":"FFFF:FFFE", "index":index,
        "interface":0, "alternate":0})
}

fn invoke(bytes: &[u8], extra: &[&str]) -> Output {
    let base = std::env::temp_dir().join(format!("ch-manifest-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&base).unwrap();
    let manifest = base.join("hardware.json");
    let runtime = base.join("runtime");
    std::fs::write(&manifest, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_canharbor"))
        .env("CANHARBOR_CONFIG_DIR", base.join("preferences"))
        .args(["--lang", "en", "--runtime-dir"])
        .arg(&runtime)
        .args(["serve", "--no-web", "--hardware-manifest"])
        .arg(&manifest)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        !runtime.exists(),
        "invalid manifest created a runtime directory"
    );
    std::fs::remove_dir_all(base).unwrap();
    output
}

#[test]
fn manifest_rejects_unknown_fields_duplicates_versions_and_bad_selectors_before_open() {
    let mut extra = usb("one", 0);
    extra["guess_firmware"] = json!(true);
    let mut both = usb("one", 0);
    both["serial"] = json!("SERIAL");
    let mut no_selector = usb("one", 0);
    no_selector.as_object_mut().unwrap().remove("index");
    let mut bad_id = usb("one", 0);
    bad_id["vid_pid"] = json!("ZZZZ:606f");
    for value in [
        json!({"version":2,"adapters":[usb("one",0)]}),
        json!({"version":1,"adapters":[]}),
        json!({"version":1,"adapters":[usb("one",0)],"unknown":true}),
        json!({"version":1,"adapters":[extra]}),
        json!({"version":1,"adapters":[usb("same",0),usb("same",1)]}),
        json!({"version":1,"adapters":[usb("one",0),usb("two",0)]}),
        json!({"version":1,"adapters":[usb("one",0),both]}),
        json!({"version":1,"adapters":[usb("one",0),no_selector]}),
        json!({"version":1,"adapters":[usb("one",0),bad_id]}),
        json!({"version":1,"adapters":[{"backend":"virtual","name":"one"}]}),
    ] {
        let output = invoke(&serde_json::to_vec(&value).unwrap(), &[]);
        assert_eq!(output.status.code(), Some(2), "{value}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("USB candidate"));
    }
}

#[test]
fn malformed_or_oversized_manifest_and_mixed_legacy_flags_fail_without_service() {
    for bytes in [b"{".to_vec(), vec![b' '; 65537]] {
        assert_eq!(invoke(&bytes, &[]).status.code(), Some(2));
    }
    let manifest = json!({"version":1,"adapters":[usb("one",0)]});
    let output = invoke(
        &serde_json::to_vec(&manifest).unwrap(),
        &[
            "--usb",
            "ffff:fffe",
            "--usb-index",
            "0",
            "--usb-interface",
            "0",
            "--usb-alternate",
            "0",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicts with legacy"));
}

#[cfg(feature = "slcan")]
#[test]
fn every_serial_setting_is_checked_before_the_first_usb_selection() {
    let path = if cfg!(windows) {
        "COM3"
    } else {
        "/dev/explicit-fixture"
    };
    for (baud, profile, polling) in [
        (12345, "lawicel", false),
        (115200, "unknown", false),
        (115200, "canable-fd", true),
    ] {
        let value = json!({"version":1,"adapters":[usb("usb",0),{
            "backend":"slcan","name":"serial","path":path,"serial_baud":baud,
            "profile":profile,"polling":polling}]});
        let output = invoke(&serde_json::to_vec(&value).unwrap(), &[]);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
}

#[test]
fn mixed_example_uses_the_documented_version_and_per_adapter_settings() {
    let manifest: Value =
        serde_json::from_str(include_str!("../../../examples/hardware-multiple.json")).unwrap();
    assert_eq!(manifest["version"], 1);
    let adapters = manifest["adapters"].as_array().unwrap();
    assert_eq!(adapters.len(), 3);
    assert_eq!(adapters[0]["backend"], "gs_usb");
    assert!(adapters[0]["serial"].is_string());
    for adapter in &adapters[1..] {
        assert_eq!(adapter["backend"], "slcan");
        assert_eq!(adapter["serial_baud"], 115200);
        assert!(!adapter["path"].as_str().unwrap().is_empty());
        assert!(adapter["profile"].is_string());
    }
}
