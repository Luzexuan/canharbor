// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded offline inspection. This module does not open devices or submit frames.
use canharbor_api::{BusEvent, Decimal, Direction, FrameDto, FrameKind};
pub use canharbor_capture::Error;
use canharbor_capture::{Header, Reason, Record, Scan};
use serde::Serialize;
use std::{
    collections::VecDeque,
    io::{Read, Write},
};
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub header: Header,
    pub complete: bool,
    pub truncated: bool,
    pub valid_bytes: Decimal,
    pub events: Decimal,
    pub frames: Decimal,
    pub diagnostics: Decimal,
    pub dropped: Decimal,
    pub unknown_loss: bool,
    pub reason: Option<Reason>,
    pub preview: Vec<BusEvent>,
}
/// Retain only the last `limit` events (0–500); scan the entire bounded source.
/// Raw DLC, direction, ESI and timestamps remain in the preview. Diagnostics
/// are counted separately and never interpreted as transmit data.
pub fn inspect<R: Read>(reader: R, limit: usize) -> Result<Inspection, Error> {
    if limit > 500 {
        return Err(Error::Invalid("preview limit exceeds 500"));
    }
    let mut preview = VecDeque::with_capacity(limit);
    let mut frames = 0u64;
    let mut diagnostics = 0u64;
    let scan = validated(reader, |event| {
        match event {
            BusEvent::FrameReceived { .. } => frames += 1,
            BusEvent::BusDiagnostic { .. } => diagnostics += 1,
        }
        if limit != 0 {
            if preview.len() == limit {
                preview.pop_front();
            }
            preview.push_back(event);
        }
        Ok(())
    })?;
    Ok(Inspection {
        header: scan.header,
        complete: scan.complete,
        truncated: scan.truncated,
        valid_bytes: scan.valid_bytes.into(),
        events: scan.events.into(),
        frames: frames.into(),
        diagnostics: diagnostics.into(),
        dropped: scan.dropped.into(),
        unknown_loss: scan.unknown_loss,
        reason: scan.reason,
        preview: preview.into(),
    })
}
/// Scan a capture and pass each event to `visit` after checking frame
/// invariants, metadata, diagnostic bytes and sequence/time order.
fn validated<R: Read>(
    reader: R,
    mut visit: impl FnMut(BusEvent) -> Result<(), Error>,
) -> Result<Scan, Error> {
    validated_with_header(reader, |item| match item {
        Item::Header(_) => Ok(()),
        Item::Event(event) => visit(event),
    })
}
enum Item<'a> {
    /// The capture header; passed once, before any event.
    Header(&'a Header),
    Event(BusEvent),
}
fn validated_with_header<R: Read>(
    reader: R,
    mut visit: impl FnMut(Item) -> Result<(), Error>,
) -> Result<Scan, Error> {
    let mut previous = None;
    canharbor_capture::scan_with(reader, |record| {
        let event = match record {
            Record::Header { metadata, .. } => return visit(Item::Header(metadata)),
            Record::Event { event } => event,
            _ => return Ok(()),
        };
        let event: BusEvent = serde_json::from_value(event.clone())?;
        let (sequence, time) = match &event {
            BusEvent::FrameReceived {
                sequence,
                host_monotonic_ns,
                raw_dlc,
                frame,
                ..
            } => {
                FrameDto {
                    kind: frame.kind,
                    id: frame.id,
                    extended: frame.extended,
                    brs: frame.brs,
                    data_hex: frame.data_hex.clone(),
                    dlc: Some(frame.dlc),
                }
                .into_frame()
                .map_err(|_| Error::Invalid("invalid captured frame"))?;
                if raw_dlc.is_some_and(|dlc| {
                    dlc > 15
                        || (if matches!(frame.kind, FrameKind::FdData) {
                            dlc
                        } else {
                            dlc.min(8)
                        }) != frame.dlc
                }) || (frame.esi && !matches!(frame.kind, FrameKind::FdData))
                {
                    return Err(Error::Invalid("invalid captured frame metadata"));
                }
                (sequence, host_monotonic_ns)
            }
            BusEvent::BusDiagnostic {
                sequence,
                host_monotonic_ns,
                raw_data_hex,
                ..
            } => {
                if raw_data_hex.len() > 128
                    || !raw_data_hex.len().is_multiple_of(2)
                    || !raw_data_hex.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(Error::Invalid("invalid diagnostic bytes"));
                }
                (sequence, host_monotonic_ns)
            }
        };
        let sequence = sequence.as_str().parse::<u64>().unwrap();
        let time = time.as_str().parse::<u64>().unwrap();
        if previous.is_some_and(|(seq, ns)| sequence <= seq || time < ns) {
            return Err(Error::Invalid("capture sequence or time moved backwards"));
        }
        previous = Some((sequence, time));
        visit(Item::Event(event))
    })
}

#[derive(Debug, Serialize)]
pub struct Export {
    pub complete: bool,
    pub truncated: bool,
    pub frames: Decimal,
    pub skipped_diagnostics: Decimal,
    pub dropped: Decimal,
    pub unknown_loss: bool,
    pub reason: Option<Reason>,
}
/// Write the frames of a capture as a can-utils log (`candump -l` syntax):
/// `(seconds.microseconds) interface ID#DATA`, `ID#R<len>` for remote frames
/// and `ID##<flags>DATA` for CAN FD, where flags bit 0 is BRS and bit 1 is ESI.
///
/// The input gets the same validation as [`inspect`]. Timestamps are the
/// service's monotonic clock, not wall-clock time. The log syntax has no place
/// for direction, raw DLC, timestamp quality, diagnostics or gaps: diagnostics
/// are counted and skipped, and the summary reports whether the source was
/// complete. An error can leave partial output in `writer`.
pub fn export_candump<R: Read, W: Write>(
    reader: R,
    mut writer: W,
    interface: &str,
) -> Result<Export, Error> {
    if interface.is_empty()
        || interface.len() > 15
        || !interface
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(Error::Invalid("invalid candump interface name"));
    }
    let mut frames = 0u64;
    let mut skipped = 0u64;
    let scan = validated(reader, |event| {
        let BusEvent::FrameReceived {
            host_monotonic_ns,
            frame,
            ..
        } = event
        else {
            skipped += 1;
            return Ok(());
        };
        let ns = host_monotonic_ns.as_str().parse::<u64>().unwrap();
        write!(
            writer,
            "({}.{:06}) {interface} ",
            ns / 1_000_000_000,
            ns % 1_000_000_000 / 1_000
        )?;
        if frame.extended {
            write!(writer, "{:08X}", frame.id)?;
        } else {
            write!(writer, "{:03X}", frame.id)?;
        }
        match frame.kind {
            FrameKind::ClassicData => write!(writer, "#{}", frame.data_hex.to_ascii_uppercase())?,
            FrameKind::ClassicRemote if frame.dlc == 0 => write!(writer, "#R")?,
            FrameKind::ClassicRemote => write!(writer, "#R{}", frame.dlc)?,
            FrameKind::FdData => write!(
                writer,
                "##{:X}{}",
                u8::from(frame.brs) | u8::from(frame.esi) << 1,
                frame.data_hex.to_ascii_uppercase()
            )?,
        }
        writeln!(writer)?;
        frames += 1;
        Ok(())
    })?;
    writer.flush()?;
    Ok(Export {
        complete: scan.complete,
        truncated: scan.truncated,
        frames: frames.into(),
        skipped_diagnostics: skipped.into(),
        dropped: scan.dropped.into(),
        unknown_loss: scan.unknown_loss,
        reason: scan.reason,
    })
}

/// `Www Mmm dd HH:MM:SS.mmm yyyy` in UTC, the date form of an ASC header.
fn asc_date(unix_ms: u64) -> Result<String, Error> {
    let days = unix_ms / 86_400_000;
    let ms = unix_ms % 86_400_000;
    // Civil date from a day count (proleptic Gregorian, days since 1970-01-01).
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    if year > 9999 {
        return Err(Error::Invalid("capture creation time out of range"));
    }
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    Ok(format!(
        "{} {} {:02} {:02}:{:02}:{:02}.{:03} {}",
        WEEKDAYS[(days % 7) as usize],
        MONTHS[(month - 1) as usize],
        day,
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000,
        year
    ))
}
/// Write the frames of a capture as a Vector ASCII log (`.asc`).
///
/// The layout follows the one python-can writes and reads: a `date` line, hex
/// base with absolute timestamps, one trigger block, classic frames as
/// `time channel id dir d|r dlc data` and CAN FD frames as `CANFD` lines.
/// Unlike the candump log this keeps the direction of each frame.
///
/// The input gets the same validation as [`inspect`]. The `date` is the
/// capture's creation time in UTC. Event times are seconds since the first
/// event of the capture, taken from the service's monotonic clock, so they are
/// offsets within the recording rather than offsets from that date. Raw DLC,
/// timestamp quality and device timestamps are dropped; diagnostics are
/// counted and skipped, and gaps are not marked. An error can leave partial
/// output in `writer`.
pub fn export_asc<R: Read, W: Write>(
    reader: R,
    mut writer: W,
    channel: u8,
) -> Result<Export, Error> {
    if channel == 0 {
        return Err(Error::Invalid("ASC channel numbers start at 1"));
    }
    let mut frames = 0u64;
    let mut skipped = 0u64;
    let mut origin = None;
    let scan = validated_with_header(reader, |item| {
        let event = match item {
            Item::Header(header) => {
                // The scanner has already checked that this is a canonical u64.
                let date = asc_date(header.created_unix_ms.parse::<u64>().unwrap())?;
                writeln!(writer, "date {date}")?;
                writeln!(writer, "base hex  timestamps absolute")?;
                writeln!(writer, "no internal events logged")?;
                writeln!(writer, "Begin Triggerblock {date}")?;
                writeln!(writer, "   0.000000 Start of measurement")?;
                return Ok(());
            }
            Item::Event(event) => event,
        };
        let (BusEvent::FrameReceived {
            host_monotonic_ns, ..
        }
        | BusEvent::BusDiagnostic {
            host_monotonic_ns, ..
        }) = &event;
        let ns = host_monotonic_ns.as_str().parse::<u64>().unwrap();
        // Times never move backwards within a validated capture.
        let elapsed = ns - *origin.get_or_insert(ns);
        let BusEvent::FrameReceived {
            direction, frame, ..
        } = event
        else {
            skipped += 1;
            return Ok(());
        };
        let time = format!(
            "{:>4}.{:06}",
            elapsed / 1_000_000_000,
            elapsed % 1_000_000_000 / 1_000
        );
        let id = format!("{:X}{}", frame.id, if frame.extended { "x" } else { "" });
        let direction = match direction {
            Direction::Rx => "Rx",
            Direction::Tx => "Tx",
        };
        let mut data = String::with_capacity(frame.data_hex.len() / 2 * 3);
        for byte in frame.data_hex.as_bytes().chunks(2) {
            data.push(' ');
            data.push(byte[0].to_ascii_uppercase() as char);
            data.push(byte[1].to_ascii_uppercase() as char);
        }
        match frame.kind {
            FrameKind::ClassicData => writeln!(
                writer,
                "{time} {channel}  {id:<15} {direction:<4} d {:x}{data}",
                frame.dlc
            )?,
            FrameKind::ClassicRemote => writeln!(
                writer,
                "{time} {channel}  {id:<15} {direction:<4} r {:x}",
                frame.dlc
            )?,
            FrameKind::FdData => writeln!(
                writer,
                "{time} CANFD {channel:>3} {direction:<4} {id:>8}  {:>32} {} {} {:x} {:>2}{data} {:>8} {:>4} {:>8X} {:>8} {:>8} {:>8} {:>8} {:>8}",
                "",
                u8::from(frame.brs),
                u8::from(frame.esi),
                frame.dlc,
                frame.data_hex.len() / 2,
                0,
                0,
                // Bit 12 marks a CAN FD frame, bit 13 BRS and bit 14 ESI.
                1u32 << 12 | u32::from(frame.brs) << 13 | u32::from(frame.esi) << 14,
                0,
                0,
                0,
                0,
                0
            )?,
        }
        frames += 1;
        Ok(())
    })?;
    writeln!(writer, "End TriggerBlock")?;
    writer.flush()?;
    Ok(Export {
        complete: scan.complete,
        truncated: scan.truncated,
        frames: frames.into(),
        skipped_diagnostics: skipped.into(),
        dropped: scan.dropped.into(),
        unknown_loss: scan.unknown_loss,
        reason: scan.reason,
    })
}
