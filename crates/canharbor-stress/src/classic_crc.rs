// SPDX-License-Identifier: AGPL-3.0-or-later
//! Classic CRC32-v1: u32 sequence + zlib-compatible IEEE CRC32 of UUID:source and sequence.
use super::*;

const fn table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut n = 0;
    while n < 256 {
        let mut crc = n as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ (0xedb88320u32.wrapping_mul(crc & 1));
            bit += 1;
        }
        table[n] = crc;
        n += 1;
    }
    table
}
const TABLE: [u32; 256] = table();

/// IEEE CRC32 (zlib), not the Castagnoli CRC32C used by FD-v1.
pub fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc = (crc >> 8) ^ TABLE[((crc ^ u32::from(*byte)) & 255) as usize];
    }
    !crc
}

impl Traffic {
    pub fn is_classic_crc(&self) -> bool {
        matches!(self, Self::ClassicCrc32V1 { .. })
    }
    pub fn classic_crc_identity(&self) -> Option<(&str, u32)> {
        match self {
            Self::ClassicCrc32V1 { run_uuid, source } => Some((run_uuid, *source)),
            _ => None,
        }
    }
    pub(crate) fn classic_crc_payload(&self, sequence: u32) -> Option<Vec<u8>> {
        let (run_uuid, source) = self.classic_crc_identity()?;
        let mut body = format!("{run_uuid}:{source}").into_bytes();
        body.extend(sequence.to_be_bytes());
        let mut data = sequence.to_be_bytes().to_vec();
        data.extend(crc32_ieee(&body).to_be_bytes());
        Some(data)
    }
}
impl Plan {
    pub fn has_classic_crc(&self) -> bool {
        self.traffic.is_classic_crc() || self.flows.iter().any(|f| f.traffic.is_classic_crc())
    }
    pub(crate) fn classic_crc_id_matches(&self, frame: &Frame) -> bool {
        self.traffic.is_classic_crc()
            && frame.id().value() == self.can_id
            && frame.id().extended() == self.extended
            && matches!(frame, Frame::ClassicData { .. } | Frame::FdData { .. })
    }
    pub(crate) fn decode_classic_crc(&self, frame: &Frame) -> Option<Result<u32, PayloadError>> {
        if !self.traffic.is_classic_crc() {
            return None;
        }
        Some(if !self.classic_crc_id_matches(frame) {
            Err(PayloadError::Unrelated)
        } else if !matches!(frame, Frame::ClassicData { .. }) || frame.data().len() != 8 {
            Err(PayloadError::Envelope)
        } else {
            let n = u32::from_be_bytes(frame.data()[..4].try_into().unwrap());
            if self.traffic.classic_crc_payload(n).unwrap() != frame.data() {
                Err(PayloadError::Crc)
            } else {
                Ok(n)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan() -> Plan {
        serde_json::from_str(include_str!("../../../examples/stress/classic-crc32.json")).unwrap()
    }
    fn hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02X}")).collect()
    }
    #[test]
    fn ieee_crc_and_python_can_test_golden_payloads_match() {
        assert_eq!(crc32_ieee(b"123456789"), 0xcbf43926);
        assert_eq!(crc32_ieee(b""), 0);
        assert_ne!(crc32_ieee(b"123456789"), crc32c(b"123456789"));
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../../api/classic-crc-fixtures.json")).unwrap();
        for v in vectors.as_array().unwrap() {
            let mut p = plan();
            p.traffic = serde_json::from_value(serde_json::json!({
                "format":"classic_crc32_v1", "run_uuid":v["run_uuid"], "source":v["source"]
            }))
            .unwrap();
            let n = v["sequence"].as_u64().unwrap() as u32;
            assert_eq!(hex(p.frame(n).data()), v["data_hex"].as_str().unwrap());
            assert_eq!(p.decode(&p.frame(n), false), Ok(n));
            assert_eq!(p.frame(n).data().len(), 8);
        }
    }
    #[test]
    fn identities_and_every_payload_byte_are_checked_before_acceptance() {
        let p = plan();
        let frame = p.frame(1);
        for i in 0..8 {
            let mut bytes = frame.data().to_vec();
            bytes[i] ^= 1;
            assert_eq!(
                p.decode(&Frame::classic(frame.id(), bytes).unwrap(), false),
                Err(PayloadError::Crc)
            );
        }
        for source in 1..8 {
            let mut other = p.clone();
            if let Traffic::ClassicCrc32V1 { source: s, .. } = &mut other.traffic {
                *s = source;
            }
            assert_eq!(p.decode(&other.frame(1), false), Err(PayloadError::Crc));
        }
        let mut other = p.clone();
        if let Traffic::ClassicCrc32V1 { run_uuid, .. } = &mut other.traffic {
            *run_uuid = "10112233-4455-4677-8899-aabbccddeeff".into();
        }
        assert_eq!(p.decode(&other.frame(1), false), Err(PayloadError::Crc));
        assert_eq!(
            p.decode(
                &Frame::fd(frame.id(), frame.data().to_vec(), false).unwrap(),
                false
            ),
            Err(PayloadError::Envelope)
        );
        for n in 0..8 {
            assert_eq!(
                p.decode(&Frame::classic(frame.id(), vec![0; n]).unwrap(), false),
                Err(PayloadError::Envelope)
            );
        }
    }
    #[test]
    fn raw_replay_reconstructs_sequence_crc_duplicates_reordering_and_missing() {
        let mut p = plan();
        p.count = 6;
        let mut verifier = Verifier::for_plan("virtual:1".into(), &p, &p.compile().unwrap());
        let mut replay = Replay::new(&p).unwrap();
        let mut frames = [0, 2, 2, 1, 5, 99].map(|n| p.frame(n)).to_vec();
        let mut bad = p.frame(3).data().to_vec();
        bad[7] ^= 1;
        frames.push(Frame::classic(p.frame(0).id(), bad).unwrap());
        frames.push(Frame::classic(p.frame(0).id(), vec![]).unwrap());
        for frame in frames {
            verifier.event(
                &p,
                Notice::Frame(canharbor_core::runtime::Event {
                    sequence: 0,
                    elapsed_ns: 1,
                    frame: frame.clone(),
                    direction: "rx",
                    metadata: Default::default(),
                }),
            );
            let raw = EvidenceEvent::ClassicCrcFrame {
                channel: "virtual:1".into(),
                data_hex: hex(frame.data()),
                wire: SampleWire::of(&frame),
                elapsed_ns: 1,
            };
            assert!(
                serde_json::to_value(&raw)
                    .unwrap()
                    .get("sequence")
                    .is_none()
            );
            replay.event(&raw).unwrap();
        }
        let rx = replay.sample_receivers().remove(0);
        assert_eq!(
            (
                rx.unique,
                rx.duplicates,
                rx.reordered,
                rx.out_of_range,
                rx.crc_errors,
                rx.pattern_errors,
                rx.missing_expected
            ),
            (4, 1, 1, 1, 1, 1, 2)
        );
        assert_eq!(rx.short_sequence_only, 0);
        for (a, b) in [
            (rx.unique, verifier.report.unique),
            (rx.crc_errors, verifier.report.crc_errors),
            (rx.pattern_errors, verifier.report.pattern_errors),
        ] {
            assert_eq!(a, b);
        }
        let mut submitted = vec![0; 1];
        for n in 0..6 {
            bitmap_set(&mut submitted, n);
        }
        assert_eq!(verifier.missing_submitted(&submitted), 2);
    }
    #[test]
    fn canonical_identity_and_legacy_serialization_are_preserved() {
        let p = plan();
        for value in [
            "",
            "00112233445546778899aabbccddeeff",
            "00112233-4455-4677-8899-AABBCCDDEEFF",
        ] {
            let mut bad = p.clone();
            if let Traffic::ClassicCrc32V1 { run_uuid, .. } = &mut bad.traffic {
                *run_uuid = value.into();
            }
            assert!(bad.compile().is_err());
        }
        let mut legacy = p;
        legacy.traffic = Traffic::Classic;
        let value = serde_json::to_value(&legacy).unwrap();
        assert!(value.get("traffic").is_none());
        assert_eq!(legacy.frame(1).data(), &[0, 0, 0, 17, 0, 0, 0, 1]);
        let mut explicit = value;
        explicit["traffic"] = serde_json::json!({"format":"classic"});
        let explicit: Plan = serde_json::from_value(explicit).unwrap();
        assert_eq!(legacy.fingerprint(), explicit.fingerprint());
        assert_eq!(
            Report::initial(&legacy, true, "legacy".into())
                .unwrap()
                .version,
            8
        );
    }
}
