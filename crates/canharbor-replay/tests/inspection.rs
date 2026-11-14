// SPDX-License-Identifier: AGPL-3.0-or-later
use canharbor_replay::{export_asc, export_blf, export_candump, inspect};
use serde_json::Value;
use std::io::{Cursor, Read};
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../../api/capture-fixtures.json")).unwrap()
}
fn bytes(records: &Value) -> Vec<u8> {
    records
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| format!("{r}\n").into_bytes())
        .collect()
}
struct Tiny(Cursor<Vec<u8>>);
impl Read for Tiny {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out.len().min(3);
        self.0.read(&mut out[..n])
    }
}
#[test]
fn shared_corpus_preserves_metadata_and_rejects_invalid_complete_records() {
    let fixtures = fixtures();
    for case in fixtures["valid"].as_array().unwrap() {
        let report = inspect(Tiny(Cursor::new(bytes(&case["records"]))), 500).unwrap();
        let report = serde_json::to_value(report).unwrap();
        assert_eq!(report["frames"], case["frames"], "{}", case["name"]);
        assert_eq!(report["diagnostics"], case["diagnostics"]);
        assert_eq!(report["complete"], case["complete"]);
        assert_eq!(report["preview"][0], case["records"][1]["event"]);
    }
    for case in fixtures["invalid"].as_array().unwrap() {
        assert!(
            inspect(bytes(&case["records"]).as_slice(), 500).is_err(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn preview_is_bounded_and_partial_final_record_is_never_exposed() {
    let fixtures = fixtures();
    let base = &fixtures["valid"][0]["records"];
    let mut records = vec![base[0].clone()];
    for i in 1..=1000 {
        let mut e = base[1].clone();
        e["event"]["sequence"] = i.to_string().into();
        e["event"]["host_monotonic_ns"] = i.to_string().into();
        records.push(e);
    }
    let mut data = bytes(&records.into());
    data.extend_from_slice(b"{\"record\":\"event\"");
    let report = inspect(data.as_slice(), 5).unwrap();
    assert_eq!(report.events.as_str(), "1000");
    assert_eq!(report.preview.len(), 5);
    assert!(report.truncated);
    assert!(!report.complete);
    assert_eq!(
        serde_json::to_value(&report.preview[0]).unwrap()["sequence"],
        "996"
    );
    assert!(inspect(data.as_slice(), 501).is_err());
    assert!(inspect(data.as_slice(), 0).unwrap().preview.is_empty());
}
#[test]
fn candump_export_writes_frames_and_reports_what_it_skipped() {
    let fixtures = fixtures();
    let export = |name: &str| {
        let case = fixtures["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let mut log = Vec::new();
        let summary =
            export_candump(Tiny(Cursor::new(bytes(&case["records"]))), &mut log, "can0").unwrap();
        (String::from_utf8(log).unwrap(), summary)
    };
    let (log, summary) = export("classic_raw_dlc_and_u64");
    assert_eq!(log, "(18446744073.709551) can0 123#0102030405060708\n");
    assert!(summary.complete);
    assert_eq!(summary.frames.as_str(), "1");
    let (log, _) = export("fd_brs_esi");
    assert!(
        log.ends_with(" can0 123##3AAAAAAAAAAAAAAAAAAAAAAAA\n"),
        "{log}"
    );
    let (log, summary) = export("diagnostic_only");
    assert!(log.is_empty());
    assert_eq!(summary.skipped_diagnostics.as_str(), "1");
    let (log, summary) = export("recoverable_prefix");
    assert_eq!(log.lines().count(), 1);
    assert!(!summary.complete);

    let base = &fixtures["valid"][0]["records"];
    let mut records = vec![base[0].clone()];
    for (i, (kind, id, extended, data, dlc)) in [
        ("classic_remote", 0x7ff, false, "", 4),
        ("classic_remote", 0x12, false, "", 0),
        ("classic_data", 0x1abc_def0, true, "", 0),
        ("fd_data", 0x12, false, "0a", 1),
    ]
    .into_iter()
    .enumerate()
    {
        let mut e = base[1].clone();
        e["event"]["sequence"] = (i + 1).to_string().into();
        e["event"]["host_monotonic_ns"] = (1_500_000_000 + i).to_string().into();
        e["event"]["raw_dlc"] = Value::Null;
        e["event"]["frame"] = serde_json::json!({
            "kind": kind, "id": id, "extended": extended, "brs": false,
            "esi": false, "data_hex": data, "dlc": dlc
        });
        records.push(e);
    }
    let mut log = Vec::new();
    export_candump(bytes(&records.into()).as_slice(), &mut log, "vcan-1").unwrap();
    assert_eq!(
        String::from_utf8(log).unwrap(),
        "(1.500000) vcan-1 7FF#R4\n(1.500000) vcan-1 012#R\n\
         (1.500000) vcan-1 1ABCDEF0#\n(1.500000) vcan-1 012##00A\n"
    );

    for case in fixtures["invalid"].as_array().unwrap() {
        assert!(
            export_candump(bytes(&case["records"]).as_slice(), Vec::new(), "can0").is_err(),
            "{}",
            case["name"]
        );
    }
    for interface in ["", "can 0", "0123456789abcdef", "can0\n"] {
        let capture = bytes(&fixtures["valid"][0]["records"]);
        assert!(export_candump(capture.as_slice(), Vec::new(), interface).is_err());
    }
}
#[test]
fn asc_export_keeps_direction_and_counts_time_from_the_first_event() {
    let fixtures = fixtures();
    let base = &fixtures["valid"][0]["records"];
    let mut header = base[0].clone();
    // 2026-11-15 10:00:00.123 UTC, a Sunday.
    header["created_unix_ms"] = "1794736800123".into();
    let mut records = vec![header];
    for (i, (kind, id, extended, data, dlc, direction, brs, esi)) in [
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
        ("fd_data", 0x12, false, "0a", 1, "tx", true, true),
        (
            "fd_data",
            0x100,
            false,
            "bbbbbbbbbbbbbbbbbbbbbbbb",
            9,
            "rx",
            true,
            false,
        ),
        ("fd_data", 0x101, false, "", 0, "rx", false, false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut e = base[1].clone();
        e["event"]["sequence"] = (i + 1).to_string().into();
        e["event"]["host_monotonic_ns"] = (5_000_000_000 + i as u64 * 1_234_567_000)
            .to_string()
            .into();
        e["event"]["raw_dlc"] = Value::Null;
        e["event"]["direction"] = direction.into();
        e["event"]["frame"] = serde_json::json!({
            "kind": kind, "id": id, "extended": extended, "brs": brs,
            "esi": esi, "data_hex": data, "dlc": dlc
        });
        records.push(e);
    }
    let mut log = Vec::new();
    let summary = export_asc(Tiny(Cursor::new(bytes(&records.into()))), &mut log, 2).unwrap();
    assert_eq!(summary.frames.as_str(), "6");
    let tail = "       0    0";
    let zeros = "       0        0        0        0        0";
    let name = " ".repeat(32);
    assert_eq!(
        String::from_utf8(log).unwrap(),
        format!(
            "date Sun Nov 15 10:00:00.123 2026\n\
             base hex  timestamps absolute\n\
             no internal events logged\n\
             Begin Triggerblock Sun Nov 15 10:00:00.123 2026\n\
             \x20  0.000000 Start of measurement\n\
             \x20  0.000000 2  123             Rx   d 8 01 02 03 04 05 06 07 08\n\
             \x20  1.234567 2  1ABCDEF0x       Tx   d 0\n\
             \x20  2.469134 2  7FF             Rx   r 4\n\
             \x20  3.703701 CANFD   2 Tx         12  {name} 1 1 1  1 0A {tail}     7000 {zeros}\n\
             \x20  4.938268 CANFD   2 Rx        100  {name} 1 0 9 12 BB BB BB BB BB BB BB BB BB BB BB BB {tail}     3000 {zeros}\n\
             \x20  6.172835 CANFD   2 Rx        101  {name} 0 0 0  0 {tail}     1000 {zeros}\n\
             End TriggerBlock\n"
        )
    );

    // Time is counted from the first event even when it is not a frame, and
    // diagnostics are skipped.
    let diagnostic = fixtures["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "diagnostic_only")
        .unwrap();
    let mut log = Vec::new();
    let summary = export_asc(bytes(&diagnostic["records"]).as_slice(), &mut log, 1).unwrap();
    assert_eq!(summary.skipped_diagnostics.as_str(), "1");
    let log = String::from_utf8(log).unwrap();
    assert_eq!(log.lines().count(), 6, "{log}");
    assert!(
        log.starts_with("date Thu Jan 01 00:00:00.001 1970\n"),
        "{log}"
    );

    for case in fixtures["invalid"].as_array().unwrap() {
        assert!(
            export_asc(bytes(&case["records"]).as_slice(), Vec::new(), 1).is_err(),
            "{}",
            case["name"]
        );
    }
    let capture = bytes(&fixtures["valid"][0]["records"]);
    assert!(export_asc(capture.as_slice(), Vec::new(), 0).is_err());
    // A creation time past year 9999 has no ASC date.
    let mut late = base.clone();
    late[0]["created_unix_ms"] = "253402300800000".into();
    assert!(export_asc(bytes(&late).as_slice(), Vec::new(), 1).is_err());
    late[0]["created_unix_ms"] = "253402300799999".into();
    let mut log = Vec::new();
    export_asc(bytes(&late).as_slice(), &mut log, 1).unwrap();
    assert!(log.starts_with(b"date Fri Dec 31 23:59:59.999 9999\n"));
}
#[test]
fn blf_export_writes_whole_objects_in_uncompressed_containers() {
    let fixtures = fixtures();
    let base = &fixtures["valid"][0]["records"];
    let u16_at = |log: &[u8], at: usize| u16::from_le_bytes(log[at..at + 2].try_into().unwrap());
    let u32_at = |log: &[u8], at: usize| u32::from_le_bytes(log[at..at + 4].try_into().unwrap());
    let u64_at = |log: &[u8], at: usize| u64::from_le_bytes(log[at..at + 8].try_into().unwrap());
    let capture = |count: usize| {
        let mut header = base[0].clone();
        // 2026-11-15 10:00:00.123 UTC, a Sunday.
        header["created_unix_ms"] = "1794736800123".into();
        let mut records = vec![header];
        for i in 0..count {
            let mut e = base[1].clone();
            e["event"]["sequence"] = (i + 1).to_string().into();
            e["event"]["host_monotonic_ns"] = (5_000_000_000 + i as u64 * 2_000_000_000)
                .to_string()
                .into();
            e["event"]["raw_dlc"] = Value::Null;
            e["event"]["direction"] = if i % 2 == 0 { "rx" } else { "tx" }.into();
            e["event"]["frame"] = if i % 2 == 0 {
                serde_json::json!({
                    "kind": "classic_data", "id": 0x1abc_def0, "extended": true,
                    "brs": false, "esi": false, "data_hex": "0102", "dlc": 2
                })
            } else {
                serde_json::json!({
                    "kind": "fd_data", "id": 0x12, "extended": false,
                    "brs": true, "esi": true, "data_hex": "aabbcc", "dlc": 3
                })
            };
            records.push(e);
        }
        bytes(&records.into())
    };

    let mut log = Cursor::new(Vec::new());
    let summary = export_blf(Tiny(Cursor::new(capture(2))), &mut log, 3).unwrap();
    assert_eq!(summary.frames.as_str(), "2");
    let log = log.into_inner();
    // File header: sizes, object count and the two times.
    assert_eq!(&log[..4], b"LOGG");
    assert_eq!(u32_at(&log, 4), 144);
    assert_eq!(u64_at(&log, 16), log.len() as u64);
    assert_eq!(u64_at(&log, 24), log.len() as u64);
    assert_eq!(u32_at(&log, 32), 2);
    let time = |at: usize| (0..8).map(|i| u16_at(&log, at + 2 * i)).collect::<Vec<_>>();
    assert_eq!(time(40), [2026, 11, 0, 15, 10, 0, 0, 123]);
    // The second frame is two seconds after the first.
    assert_eq!(time(56), [2026, 11, 0, 15, 10, 0, 2, 123]);
    assert!(log[72..144].iter().all(|byte| *byte == 0));
    // One container holding a classic object (48 bytes) and an FD one (116).
    assert_eq!(log.len(), 144 + 32 + 48 + 116);
    assert_eq!(&log[144..148], b"LOBJ");
    assert_eq!((u16_at(&log, 148), u16_at(&log, 150)), (16, 1));
    assert_eq!((u32_at(&log, 152), u32_at(&log, 156)), (32 + 164, 10));
    assert_eq!((u16_at(&log, 160), u32_at(&log, 168)), (0, 164));
    let classic = &log[176..224];
    assert_eq!(&classic[..4], b"LOBJ");
    assert_eq!((u16_at(classic, 4), u16_at(classic, 6)), (32, 1));
    assert_eq!((u32_at(classic, 8), u32_at(classic, 12)), (48, 1));
    assert_eq!((u32_at(classic, 16), u64_at(classic, 24)), (2, 0));
    // Channel, flags (received, data), DLC, identifier with the extended bit.
    assert_eq!(u16_at(classic, 32), 3);
    assert_eq!(classic[34..36], [0, 2]);
    assert_eq!(u32_at(classic, 36), 0x9abc_def0);
    assert_eq!(classic[40..48], [1, 2, 0, 0, 0, 0, 0, 0]);
    let fd = &log[224..];
    assert_eq!((u32_at(fd, 8), u32_at(fd, 12)), (116, 100));
    assert_eq!(u64_at(fd, 24), 2_000_000_000);
    // Transmitted; FD flags EDL, BRS and ESI; three valid bytes.
    assert_eq!(fd[34..36], [1, 3]);
    assert_eq!(u32_at(fd, 36), 0x12);
    assert_eq!(fd[45..47], [0b111, 3]);
    assert_eq!(fd[52..56], [0xaa, 0xbb, 0xcc, 0]);

    // Enough frames for several containers: every container starts with an
    // object header, so no object is split, and the sizes add up.
    let mut log = Cursor::new(Vec::new());
    let summary = export_blf(capture(4000).as_slice(), &mut log, 1).unwrap();
    assert_eq!(summary.frames.as_str(), "4000");
    let log = log.into_inner();
    assert_eq!(u64_at(&log, 16), log.len() as u64);
    assert_eq!(u32_at(&log, 32), 4000);
    let (mut at, mut containers, mut objects) = (144, 0, 0);
    while at < log.len() {
        assert_eq!(&log[at..at + 4], b"LOBJ");
        assert_eq!(u32_at(&log, at + 12), 10);
        let end = at + u32_at(&log, at + 8) as usize;
        assert_eq!(u32_at(&log, at + 24) as usize, end - at - 32);
        let mut inner = at + 32;
        while inner < end {
            assert_eq!(&log[inner..inner + 4], b"LOBJ");
            inner += u32_at(&log, inner + 8) as usize;
            objects += 1;
        }
        assert_eq!(inner, end);
        at = end;
        containers += 1;
    }
    assert_eq!(objects, 4000);
    assert!(containers > 1, "{containers}");

    // A capture without frames is a header and nothing else.
    let diagnostic = fixtures["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "diagnostic_only")
        .unwrap();
    let mut log = Cursor::new(Vec::new());
    let summary = export_blf(bytes(&diagnostic["records"]).as_slice(), &mut log, 1).unwrap();
    assert_eq!(summary.skipped_diagnostics.as_str(), "1");
    let log = log.into_inner();
    assert_eq!(log.len(), 144);
    assert_eq!((u64_at(&log, 16), u32_at(&log, 32)), (144, 0));

    for case in fixtures["invalid"].as_array().unwrap() {
        assert!(
            export_blf(
                bytes(&case["records"]).as_slice(),
                Cursor::new(Vec::new()),
                1
            )
            .is_err(),
            "{}",
            case["name"]
        );
    }
    let valid = bytes(&fixtures["valid"][0]["records"]);
    assert!(export_blf(valid.as_slice(), Cursor::new(Vec::new()), 0).is_err());
}
