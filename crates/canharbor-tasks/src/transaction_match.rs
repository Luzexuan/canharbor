// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded response correlation over validated CAN frames.
//!
//! The caller supplies requests and RX frames in observation order. This module
//! does not send, read a clock, enforce deadlines, or establish a fresh bus epoch.
//! See `docs/transaction-match.md` for correlation and history limits.
use canharbor_core::{CanId, Frame};
use std::collections::VecDeque;

pub const MAX_WINDOW: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    ClassicData,
    ClassicRemote,
    FdData { brs: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskedByte {
    pub offset: usize,
    pub value: u8,
    pub mask: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    BigEndian,
    LittleEndian,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceField {
    pub offset: usize,
    /// Number of whole bytes, from one through eight.
    pub width: u8,
    pub byte_order: ByteOrder,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: CanId,
    pub kind: FrameKind,
    /// Payload bytes for data frames; requested DLC for remote frames.
    pub length: u8,
    pub bytes: Vec<MaskedByte>,
    pub sequence: Option<SequenceField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Matched {
        request_id: u64,
    },
    Duplicate {
        request_id: u64,
    },
    Unrelated,
    /// The rule matches, but request ownership is uncertain. Loss and stop
    /// decisions belong to the caller's evidence and explicit policy.
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid response frame shape, byte mask, or sequence field")]
    InvalidRule,
    #[error("response window must be between one and 1024 requests")]
    InvalidWindow,
    #[error("request sequence is absent, unexpected, or exceeds its field width")]
    InvalidSequence,
    #[error("request sequences must increase without reuse or wrap")]
    SequenceReused,
    #[error("request ID is already retained")]
    DuplicateRequest,
    #[error("response window is full of pending requests")]
    WindowFull,
    #[error("request is not retained")]
    UnknownRequest,
    #[error("invalid CAN frame")]
    InvalidFrame,
}

#[derive(Clone, Copy)]
enum State {
    Pending,
    Retired,
    Answered,
}
struct Request {
    id: u64,
    sequence: Option<u64>,
    state: State,
}

pub struct Matcher {
    rule: Rule,
    window: usize,
    requests: VecDeque<Request>,
    last_sequence: Option<u64>,
    history_lost: bool,
}

impl Matcher {
    pub fn new(mut rule: Rule, window: usize) -> Result<Self, Error> {
        if !(1..=MAX_WINDOW).contains(&window) {
            return Err(Error::InvalidWindow);
        }
        let length = usize::from(rule.length);
        let shape = match rule.kind {
            FrameKind::ClassicData => Frame::classic(rule.id, vec![0; length]),
            FrameKind::ClassicRemote => Frame::remote(rule.id, rule.length),
            FrameKind::FdData { brs } => Frame::fd(rule.id, vec![0; length], brs),
        };
        if shape.is_err() || rule.bytes.len() > 64 {
            return Err(Error::InvalidRule);
        }
        let data_length = if rule.kind == FrameKind::ClassicRemote {
            0
        } else {
            length
        };
        if rule.bytes.iter().any(|byte| byte.offset >= data_length) {
            return Err(Error::InvalidRule);
        }
        if let Some(field) = rule.sequence
            && (!(1..=8).contains(&field.width)
                || !field
                    .offset
                    .checked_add(usize::from(field.width))
                    .is_some_and(|end| end <= data_length))
        {
            return Err(Error::InvalidRule);
        }
        // Discard any excess capacity reserved by the caller before retaining the rule.
        rule.bytes = rule.bytes.into_boxed_slice().into_vec();
        Ok(Self {
            rule,
            window,
            requests: VecDeque::with_capacity(window),
            last_sequence: None,
            history_lost: false,
        })
    }

    /// Register before submitting a request. A sequence must be echoed by the
    /// response, fit its field, and increase strictly for this matcher's lifetime.
    /// Completed history may be evicted; pending requests are never evicted.
    pub fn register(&mut self, request_id: u64, sequence: Option<u64>) -> Result<(), Error> {
        match (self.rule.sequence, sequence) {
            (Some(field), Some(value)) => {
                if field.width < 8 && value >= 1u64 << (field.width * 8) {
                    return Err(Error::InvalidSequence);
                }
                if self.last_sequence.is_some_and(|last| value <= last) {
                    return Err(Error::SequenceReused);
                }
            }
            (None, None) => {}
            _ => return Err(Error::InvalidSequence),
        }
        if self.requests.iter().any(|request| request.id == request_id) {
            return Err(Error::DuplicateRequest);
        }
        if self.requests.len() == self.window {
            let index = self
                .requests
                .iter()
                .position(|request| !matches!(request.state, State::Pending))
                .ok_or(Error::WindowFull)?;
            self.requests.remove(index);
            self.history_lost = true;
        }
        self.requests.push_back(Request {
            id: request_id,
            sequence,
            state: State::Pending,
        });
        self.last_sequence = sequence;
        Ok(())
    }

    /// Stop waiting without forgetting a possible late response. Already answered
    /// requests keep their duplicate history.
    pub fn retire(&mut self, request_id: u64) -> Result<(), Error> {
        let request = self
            .requests
            .iter_mut()
            .find(|request| request.id == request_id)
            .ok_or(Error::UnknownRequest)?;
        if matches!(request.state, State::Pending) {
            request.state = State::Retired;
        }
        Ok(())
    }

    pub fn retained_requests(&self) -> usize {
        self.requests.len()
    }

    /// Correlate only RX data supplied by the caller. A match on a retired request
    /// remains attributable, but this result does not classify it as on time.
    pub fn observe(&mut self, frame: &Frame) -> Result<Outcome, Error> {
        if frame.data().len() > 64 || frame.validate().is_err() {
            return Err(Error::InvalidFrame);
        }
        let (kind, length) = match frame {
            Frame::ClassicData { data, .. } => (FrameKind::ClassicData, data.len()),
            Frame::ClassicRemote { dlc, .. } => (FrameKind::ClassicRemote, usize::from(*dlc)),
            Frame::FdData { data, brs, .. } => (FrameKind::FdData { brs: *brs }, data.len()),
        };
        if frame.id() != self.rule.id
            || kind != self.rule.kind
            || length != usize::from(self.rule.length)
            || self
                .rule
                .bytes
                .iter()
                .any(|byte| frame.data()[byte.offset] & byte.mask != byte.value & byte.mask)
        {
            return Ok(Outcome::Unrelated);
        }
        let index = if let Some(field) = self.rule.sequence {
            let bytes = &frame.data()[field.offset..field.offset + usize::from(field.width)];
            let sequence = match field.byte_order {
                ByteOrder::BigEndian => bytes
                    .iter()
                    .fold(0, |value, byte| (value << 8) | u64::from(*byte)),
                ByteOrder::LittleEndian => bytes
                    .iter()
                    .rev()
                    .fold(0, |value, byte| (value << 8) | u64::from(*byte)),
            };
            self.requests
                .iter()
                .position(|request| request.sequence == Some(sequence))
        } else {
            if self.history_lost || self.requests.len() > 1 {
                return Ok(Outcome::Ambiguous);
            }
            (!self.requests.is_empty()).then_some(0)
        };
        let Some(index) = index else {
            return Ok(Outcome::Unrelated);
        };
        let request = &mut self.requests[index];
        if matches!(request.state, State::Answered) {
            return Ok(Outcome::Duplicate {
                request_id: request.id,
            });
        }
        request.state = State::Answered;
        Ok(Outcome::Matched {
            request_id: request.id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_mask_allocation_is_not_retained() {
        let bytes = Vec::with_capacity(4096);
        let matcher = Matcher::new(
            Rule {
                id: CanId::new(1, false).unwrap(),
                kind: FrameKind::ClassicData,
                length: 0,
                bytes,
                sequence: None,
            },
            MAX_WINDOW,
        )
        .unwrap();
        assert_eq!(matcher.rule.bytes.capacity(), 0);
        assert!(matcher.requests.capacity() <= MAX_WINDOW);
    }
}
