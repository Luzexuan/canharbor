# Offline capture inspection

`canharbor capture inspect capture.ndjson --limit 100` reads a local recording
without contacting the service, acquiring control or opening a device. It
returns a JSON summary and the last requested events, limited to 500.

The reader scans one bounded line at a time: 8192 bytes per record and 64 MiB
per file. It validates record order, schema version, counters, footer counts,
frame invariants, raw DLC and ESI metadata, diagnostic bytes, event sequence and
monotonic timestamp order. Partial final records are excluded; complete valid
prefixes remain inspectable as incomplete. Malformed complete records are
rejected. No incomplete recording is silently labeled complete.

Preview entries retain received/transmitted direction, raw DLC, timestamp
quality and epoch, hardware counters, ESI and diagnostics. Diagnostics are
counted separately from frames. Inspection does not turn observed traffic into
a transmit plan or discard metadata to make a frame sendable. Active replay
requires a separate explicit operation and policy.

## Export to a can-utils log

`canharbor capture export capture.ndjson --output capture.log` converts a
recording to the text log syntax written by `candump -l`. It runs without the
service and applies the same validation as `inspect`; a rejected recording
produces no output file. An existing destination is not replaced.

```text
(1.500000) can0 123#0102030405060708
(1.500210) can0 7FF#R4
(1.500420) can0 1ABCDEF0##1AABBCCDD
```

Each line carries the service's monotonic time in seconds, an interface name
(`--interface`, default `can0`), and the frame. Extended identifiers use eight
digits, remote frames use `#R` with the requested length, and CAN FD frames use
`##` followed by one flag digit (bit 0 BRS, bit 1 ESI).

The log syntax cannot carry everything a recording holds. Direction, raw DLC,
timestamp quality and epoch, and device timestamps are dropped. Diagnostic
events are skipped and counted in the JSON summary; gaps are not marked in the
log. The summary also reports whether the source recording was complete, so an
exported prefix is not mistaken for a full recording. Timestamps are not
wall-clock time. The output has not been checked against other tools' parsers;
tests compare it with the documented syntax only.

## Export to a Vector ASCII log

`canharbor capture export capture.ndjson --output capture.asc --format asc`
writes the same frames as an ASC log. Validation, the JSON summary and the
handling of the destination file are the same as for the can-utils log.

```text
date Sun Nov 15 10:00:00.123 2026
base hex  timestamps absolute
no internal events logged
Begin Triggerblock Sun Nov 15 10:00:00.123 2026
   0.000000 Start of measurement
   0.000000 1  123             Rx   d 8 01 02 03 04 05 06 07 08
   1.234567 1  7FF             Rx   r 4
   2.469134 CANFD   1 Tx   1ABCDEF0x  ... 1 0 9 12 AA BB ...
End TriggerBlock
```

Unlike the can-utils log, each line keeps the frame's direction. The channel
number on every line is `--asc-channel` (default 1); a recording covers one
channel.

- The `date` is the recording's creation time in UTC. The format has no time
  zone field, and readers generally take the date as local time.
- Event times are seconds since the first event in the recording, from the
  service's monotonic clock. They are offsets within the recording, not
  offsets from the `date`: the first event is written at 0.000000 however long
  after creation it arrived.
- Raw DLC, timestamp quality and device timestamps are dropped. Diagnostic
  events are skipped and counted, not written as `ErrorFrame` lines; gaps are
  not marked.
- CAN FD lines carry zero for the fields a recording does not hold (message
  duration, bit count, CRC and bit timing).

The layout follows the one python-can 4.6 writes, and a test reads the output
back with python-can's reader. A CAN FD frame with no data is the one case that
reader reports differently: it returns such a line as a remote frame. The
output has not been opened in Vector's tools. BLF is not supported.

The shared synthetic capture corpus covers Classic raw DLC, CAN FD BRS/ESI,
64-bit precision, diagnostics, recoverable prefixes and invalid frames. Tests
also use a three-byte reader, verify bounded previews over 1000 events, and
invoke the CLI with no service running. Hardware was not used.

The Web UI provides a local file viewer for recordings up to 1 GiB. Its initial
scan validates every record before publishing either an integrity summary or a
page index. A malformed late record or invalid footer rejects the scan even if
the first page itself is valid. Valid incomplete prefixes retain an explicit
incomplete status.

The index stores at most 256 byte-offset checkpoints, initially one per 16
matching events. When full, the reader doubles checkpoint spacing and compacts
the index. Pages hold at most 100 events; record buffers remain limited to 8192
bytes. Previous/next and page-number jumps read from the nearest checkpoint in
the immutable local file instead of revalidating from byte zero. Page reads do
not change the separately displayed whole-file integrity result. Sparse matches
or widely spaced checkpoints can still require a longer forward scan.

ID substring and RX/TX filters cover the whole recording. Applying different
filters performs a new full scan and builds a replacement bounded index; Reset
filters restores all events and page one. The CLI retains its independent
64 MiB limit and last-event preview behavior.

Scanning reports byte progress and can be cancelled. Files remain local and
are never uploaded. Loading, filtering, paging or cancelling does not acquire
control or issue API mutations. Rendering uses text nodes and preserves 64-bit
values as decimal strings.
