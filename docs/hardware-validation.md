# Hardware validation

The controlled bench validation matrix below has no recorded results. A
[SLCAN smoke test](slcan.md#hardware-smoke-test) records a limited physical
observation on 2026-11-05. Its device, host, frame format and remaining
uncertainty are preserved in that record. This page defines the evidence levels,
the controlled validation procedure and the matrix for subsequent results.

## Support tiers

| Backend | Tier | Meaning |
| --- | --- | --- |
| Virtual bus | Supported | Default. Exercised by every test in CI. |
| gs_usb | Primary hardware target | The first backend to be validated on real adapters. Defects found there take priority. |
| SLCAN, PEAK, Kvaser | Experimental | Built in CI and exercised with simulated transports or libraries. The limited SLCAN smoke observation is documented separately. Controlled bench coverage has no matrix entries. Maintenance coverage is below gs_usb and these backends may change or be removed. |

A support tier describes maintenance coverage. A hardware result applies to its
recorded adapter, firmware, driver, host and configuration. Changes to support
tiers must cite controlled validation evidence and identify the supported scope.

## Evidence levels

| Level | Required record | Scope |
| --- | --- | --- |
| Software tests | Test command, code revision, simulated transport/library and result | Protocol, state and failure handling exercised by that test |
| Hardware smoke observation | Physical device, configuration, observed RX/TX results and unresolved outcomes | The specific observed path, including incomplete or failed checks |
| Controlled bench result | Frozen setup, explicit criteria, all attempts, raw evidence and independent verification | The adapter/firmware/driver/host/configuration combination and cases actually run |
| Supported combination | Controlled results referenced by a maintained support declaration | The explicitly listed combinations and acceptance coverage |

The SLCAN observation remains a smoke record. Its two unconfirmed submissions,
single independent RX and no-BRS configuration remain part of its result.

## Validation record

Keep a versioned result bundle or stable artifact link for each run, including
failed and interrupted attempts. Record the following fields beside the result.

| Field | Content |
| --- | --- |
| Identity | Run ID, date, CANHarbor commit and dirty state, executable/test-program hashes |
| Devices | Backend, model, serial or explicit physical label, firmware, open-epoch channel IDs and sender/receiver roles |
| Host and driver | OS, architecture, native library version/hash and driver version where available |
| Setup | Wiring and termination declaration, nominal/data bitrate, BRS, active/listen-only mode, identifier width, payload/CRC format and configuration readback scope |
| Method | Exact commands or plan, request/response matching, deadline and timing origin, planned load, duration, acceptance criteria and sampling coverage |
| Results | Planned, skipped, failed, unknown and submitted counts, unique peer RX, missing/duplicate/reordered/payload errors, diagnostics and cleanup |
| Evidence | Raw files, report, manifest/hashes, verification command/result, reviewer and observed gaps |

Use `unknown` with a reason when a value is unavailable. Preserve the distinction
between requested settings, driver observations and independently measured bus
behaviour. Keep the scope of a smoke observation when linking it from the matrix.
The matrix below is reserved for controlled bench records.

## Procedure: two gs_usb adapters

This checks the smallest claim that matters: frames sent through one adapter
arrive through another, in both directions, in Classic CAN and CAN FD.

Requirements:

- Two gs_usb adapters, or one adapter with two channels, connected to each
  other with CAN-H to CAN-H and CAN-L to CAN-L and a 120 Ω terminator at each
  end of the bus. With two nodes and no other ACK source, both must be active.
- A build with the web workbench or `--no-web`; see the [README](../README.md).
- On Linux, device access as described in
  [hardware access](hardware-service.md#host-prerequisites-on-linux).

The service opens one gs_usb device. For two separate adapters, run two
services with different runtime directories:

```sh
canharbor usb-list --vid-pid VENDOR:PRODUCT --show-serial

canharbor --runtime-dir /tmp/canharbor-a serve --no-web --log-requests \
  --usb VENDOR:PRODUCT --usb-serial SERIAL_A --usb-interface 0 --usb-alternate 0
canharbor --runtime-dir /tmp/canharbor-b serve --no-web --log-requests \
  --usb VENDOR:PRODUCT --usb-serial SERIAL_B --usb-interface 0 --usb-alternate 0
```

Find the channel IDs with `canharbor --runtime-dir DIR devices --json`, and
keep the output of `canharbor --runtime-dir DIR doctor --devices` from both
services with the results.

For each row of the table below, start the receiver, wait for its readiness
message, then start the sender. Use a new `--run-id` for every run.

```sh
canharbor --runtime-dir /tmp/canharbor-b sequence receive --active \
  --channel CHANNEL_B --id 123 --run-id 00000001 --count 1000 --timeout-ms 30000
canharbor --runtime-dir /tmp/canharbor-a sequence send --active \
  --channel CHANNEL_A --id 123 --run-id 00000001 --count 1000 --interval-ms 10
```

| Run | Direction | Options added at both ends |
| --- | --- | --- |
| 1 | A → B | none (Classic, 500 kbit/s, 11-bit identifier) |
| 2 | B → A | none |
| 3 | A → B | `--extended` |
| 4 | A → B | `--fd --data-bitrate 2000000` |
| 5 | A → B | `--fd --data-bitrate 2000000 --brs` |
| 6 | B → A | `--fd --data-bitrate 2000000 --brs` |

A run passes when the receiver reports `passed: true` with `missing`,
`duplicates` and `out_of_order` all zero and no diagnostics. Keep the final JSON
of both commands for every run, including failed ones.

Then, once, watch the bus from side B in listen-only mode:

```sh
canharbor --runtime-dir /tmp/canharbor-b monitor --channel CHANNEL_B \
  --listen-only --format text
```

- Unplug adapter A while idle. Service A should report the channel as
  disconnected, and reconnecting the adapter should not resume anything. A
  service does not reopen an adapter: stop service A, start it again with the
  same options, and read the channel ID again before the next check.
- Start a long `sequence send` through service A and stop **service A** with
  Ctrl-C while it runs. Frames submitted before the stop keep their submitted
  result. Requests after the stop must fail or report an unknown outcome; none
  may be reported as submitted. The service's shutdown report should list any
  transmissions it could not confirm.

Stopping service B instead is not a check of the sender: a hardware submission
means the adapter accepted the frame, not that a peer received or acknowledged
it, so the sender may correctly keep reporting submitted frames.

## What a pass does and does not show

A pass shows that this adapter model and firmware, on this host and operating
system, exchanged these frames at these bit rates on a short bench bus. It does
not show behaviour under bus load, with other nodes, at other bit rates, over
long runs, across sleep and wake, or for any other adapter or firmware.

## Matrix

Add one row per adapter, firmware, host and date. Link the saved outputs.

| Date | Adapter and firmware | Host and OS | CANHarbor revision | Runs passed | Notes and evidence |
| --- | --- | --- | --- | --- | --- |
| – | none recorded | – | – | – | – |
