// SPDX-License-Identifier: AGPL-3.0-or-later
use canharbor_core::{CanId, Frame};
use canharbor_tasks::transaction_match::{
    ByteOrder, Error, FrameKind, MAX_WINDOW, MaskedByte, Matcher, Outcome, Rule, SequenceField,
};

fn id(extended: bool) -> CanId {
    CanId::new(0x123, extended).unwrap()
}
fn rule() -> Rule {
    Rule {
        id: id(false),
        kind: FrameKind::ClassicData,
        length: 3,
        bytes: vec![MaskedByte {
            offset: 0,
            value: 0xa0,
            mask: 0xf0,
        }],
        sequence: Some(SequenceField {
            offset: 1,
            width: 2,
            byte_order: ByteOrder::BigEndian,
        }),
    }
}
fn response(sequence: u16) -> Frame {
    let [a, b] = sequence.to_be_bytes();
    Frame::classic(id(false), vec![0xa5, a, b]).unwrap()
}

#[test]
fn matches_masks_identifier_width_kind_length_and_brs() {
    let mut matcher = Matcher::new(rule(), 4).unwrap();
    matcher.register(10, Some(7)).unwrap();
    let unrelated = [
        Frame::classic(id(true), vec![0xa5, 0, 7]).unwrap(),
        Frame::classic(CanId::new(0x124, false).unwrap(), vec![0xa5, 0, 7]).unwrap(),
        Frame::classic(id(false), vec![0xb5, 0, 7]).unwrap(),
        Frame::classic(id(false), vec![0xa5, 7]).unwrap(),
        Frame::fd(id(false), vec![0xa5, 0, 7], false).unwrap(),
        Frame::remote(id(false), 3).unwrap(),
        response(8),
    ];
    for frame in unrelated {
        assert_eq!(matcher.observe(&frame).unwrap(), Outcome::Unrelated);
    }
    assert_eq!(
        matcher.observe(&response(7)).unwrap(),
        Outcome::Matched { request_id: 10 }
    );
    assert_eq!(
        matcher.observe(&response(7)).unwrap(),
        Outcome::Duplicate { request_id: 10 }
    );

    let mut fd = rule();
    fd.kind = FrameKind::FdData { brs: true };
    let mut matcher = Matcher::new(fd, 1).unwrap();
    matcher.register(11, Some(7)).unwrap();
    assert_eq!(
        matcher
            .observe(&Frame::fd(id(false), vec![0xa5, 0, 7], false).unwrap())
            .unwrap(),
        Outcome::Unrelated
    );
    assert_eq!(
        matcher
            .observe(&Frame::fd(id(false), vec![0xa5, 0, 7], true).unwrap())
            .unwrap(),
        Outcome::Matched { request_id: 11 }
    );
}

#[test]
fn validates_rule_offsets_lengths_and_resource_limits() {
    for window in [0, MAX_WINDOW + 1, usize::MAX] {
        assert!(matches!(
            Matcher::new(rule(), window),
            Err(Error::InvalidWindow)
        ));
    }
    for field in [
        SequenceField {
            offset: 1,
            width: 0,
            byte_order: ByteOrder::BigEndian,
        },
        SequenceField {
            offset: 0,
            width: 9,
            byte_order: ByteOrder::BigEndian,
        },
        SequenceField {
            offset: usize::MAX,
            width: 2,
            byte_order: ByteOrder::BigEndian,
        },
        SequenceField {
            offset: 2,
            width: 2,
            byte_order: ByteOrder::BigEndian,
        },
    ] {
        let mut invalid = rule();
        invalid.sequence = Some(field);
        assert!(matches!(Matcher::new(invalid, 1), Err(Error::InvalidRule)));
    }
    let mut invalid = rule();
    invalid.bytes[0].offset = 3;
    assert!(matches!(Matcher::new(invalid, 1), Err(Error::InvalidRule)));
    let mut invalid = rule();
    invalid.bytes = vec![
        MaskedByte {
            offset: 0,
            value: 0,
            mask: 0
        };
        65
    ];
    assert!(matches!(Matcher::new(invalid, 1), Err(Error::InvalidRule)));
    for (kind, length) in [
        (FrameKind::ClassicData, 9),
        (FrameKind::ClassicRemote, 9),
        (FrameKind::FdData { brs: true }, 9),
    ] {
        let mut invalid = rule();
        invalid.kind = kind;
        invalid.length = length;
        assert!(matches!(Matcher::new(invalid, 1), Err(Error::InvalidRule)));
    }
}

#[test]
fn accepts_zero_length_data_and_remote_dlc_without_payload_matching() {
    for (kind, length, frame) in [
        (
            FrameKind::ClassicData,
            0,
            Frame::classic(id(false), vec![]).unwrap(),
        ),
        (
            FrameKind::ClassicRemote,
            8,
            Frame::remote(id(false), 8).unwrap(),
        ),
        (
            FrameKind::FdData { brs: false },
            0,
            Frame::fd(id(false), vec![], false).unwrap(),
        ),
    ] {
        let mut matcher = Matcher::new(
            Rule {
                id: id(false),
                kind,
                length,
                bytes: vec![],
                sequence: None,
            },
            1,
        )
        .unwrap();
        matcher.register(1, None).unwrap();
        assert_eq!(
            matcher.observe(&frame).unwrap(),
            Outcome::Matched { request_id: 1 }
        );
    }
    let mut invalid = rule();
    invalid.kind = FrameKind::ClassicRemote;
    assert!(matches!(Matcher::new(invalid, 1), Err(Error::InvalidRule)));
}

#[test]
fn validates_frames_before_matching_without_mutating_requests() {
    let mut matcher = Matcher::new(rule(), 1).unwrap();
    matcher.register(1, Some(7)).unwrap();
    for frame in [
        Frame::ClassicData {
            id: id(false),
            data: vec![0; 9],
        },
        Frame::FdData {
            id: id(false),
            data: vec![0; 65],
            brs: true,
        },
        Frame::ClassicRemote {
            id: id(false),
            dlc: 255,
        },
    ] {
        assert_eq!(matcher.observe(&frame), Err(Error::InvalidFrame));
    }
    assert_eq!(
        matcher.observe(&response(7)).unwrap(),
        Outcome::Matched { request_id: 1 }
    );
}

#[test]
fn sequence_responses_can_arrive_out_of_order_and_after_retirement() {
    let mut matcher = Matcher::new(rule(), 3).unwrap();
    matcher.register(1, Some(1)).unwrap();
    matcher.register(2, Some(2)).unwrap();
    matcher.retire(1).unwrap();
    assert_eq!(
        matcher.observe(&response(2)).unwrap(),
        Outcome::Matched { request_id: 2 }
    );
    assert_eq!(
        matcher.observe(&response(1)).unwrap(),
        Outcome::Matched { request_id: 1 }
    );
    matcher.retire(1).unwrap();
    assert_eq!(
        matcher.observe(&response(1)).unwrap(),
        Outcome::Duplicate { request_id: 1 }
    );
    assert_eq!(matcher.retire(9), Err(Error::UnknownRequest));
}

#[test]
fn sequence_values_are_exact_for_both_byte_orders_and_full_u64() {
    for byte_order in [ByteOrder::BigEndian, ByteOrder::LittleEndian] {
        let value = u64::MAX;
        let data = match byte_order {
            ByteOrder::BigEndian => value.to_be_bytes(),
            ByteOrder::LittleEndian => value.to_le_bytes(),
        };
        let rule = Rule {
            id: id(true),
            kind: FrameKind::FdData { brs: false },
            length: 8,
            bytes: vec![],
            sequence: Some(SequenceField {
                offset: 0,
                width: 8,
                byte_order,
            }),
        };
        let mut matcher = Matcher::new(rule, 1).unwrap();
        matcher.register(9, Some(value)).unwrap();
        assert_eq!(
            matcher
                .observe(&Frame::fd(id(true), data.to_vec(), false).unwrap())
                .unwrap(),
            Outcome::Matched { request_id: 9 }
        );
    }
    let mut little = rule();
    little.sequence.as_mut().unwrap().byte_order = ByteOrder::LittleEndian;
    let mut matcher = Matcher::new(little, 1).unwrap();
    matcher.register(1, Some(0x1234)).unwrap();
    assert_eq!(
        matcher
            .observe(&Frame::classic(id(false), vec![0xa5, 0x34, 0x12]).unwrap())
            .unwrap(),
        Outcome::Matched { request_id: 1 }
    );
}

#[test]
fn rejects_sequence_reuse_wrap_missing_values_and_duplicate_ids() {
    let mut matcher = Matcher::new(rule(), 2).unwrap();
    assert_eq!(matcher.register(1, None), Err(Error::InvalidSequence));
    assert_eq!(
        matcher.register(1, Some(65536)),
        Err(Error::InvalidSequence)
    );
    matcher.register(1, Some(65534)).unwrap();
    assert_eq!(
        matcher.register(1, Some(65535)),
        Err(Error::DuplicateRequest)
    );
    matcher.register(2, Some(65535)).unwrap();
    matcher.retire(1).unwrap();
    assert_eq!(matcher.register(3, Some(0)), Err(Error::SequenceReused));
    assert_eq!(matcher.register(3, Some(65534)), Err(Error::SequenceReused));
    assert_eq!(matcher.retained_requests(), 2);
}

#[test]
fn pending_window_is_not_evicted_and_completed_history_is_bounded() {
    let mut matcher = Matcher::new(rule(), 2).unwrap();
    matcher.register(1, Some(1)).unwrap();
    matcher.register(2, Some(2)).unwrap();
    assert_eq!(matcher.register(3, Some(3)), Err(Error::WindowFull));
    matcher.retire(1).unwrap();
    matcher.register(3, Some(3)).unwrap();
    assert_eq!(matcher.observe(&response(1)).unwrap(), Outcome::Unrelated);
    assert_eq!(
        matcher.observe(&response(2)).unwrap(),
        Outcome::Matched { request_id: 2 }
    );
    for sequence in 4u64..2000 {
        matcher.retire(sequence - 1).unwrap();
        matcher.register(sequence, Some(sequence)).unwrap();
        assert_eq!(matcher.retained_requests(), 2);
    }
}

#[test]
fn unsequenced_late_or_duplicate_replies_cannot_be_assigned_to_the_next_request() {
    let mut unsequenced = rule();
    unsequenced.sequence = None;
    let mut matcher = Matcher::new(unsequenced.clone(), 2).unwrap();
    assert_eq!(matcher.register(1, Some(1)), Err(Error::InvalidSequence));
    matcher.register(1, None).unwrap();
    matcher.retire(1).unwrap();
    matcher.register(2, None).unwrap();
    assert_eq!(matcher.observe(&response(7)).unwrap(), Outcome::Ambiguous);

    let mut matcher = Matcher::new(unsequenced.clone(), 2).unwrap();
    matcher.register(1, None).unwrap();
    assert_eq!(
        matcher.observe(&response(7)).unwrap(),
        Outcome::Matched { request_id: 1 }
    );
    assert_eq!(
        matcher.observe(&response(7)).unwrap(),
        Outcome::Duplicate { request_id: 1 }
    );
    matcher.register(2, None).unwrap();
    assert_eq!(matcher.observe(&response(7)).unwrap(), Outcome::Ambiguous);

    let mut matcher = Matcher::new(unsequenced, 1).unwrap();
    matcher.register(1, None).unwrap();
    matcher.retire(1).unwrap();
    matcher.register(2, None).unwrap();
    assert_eq!(matcher.retained_requests(), 1);
    assert_eq!(matcher.observe(&response(7)).unwrap(), Outcome::Ambiguous);
}
