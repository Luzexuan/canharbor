# Hardware validation

No adapter has been validated yet. This page is the procedure for the first
validation and the place where results are recorded. Until a row exists in the
matrix below, every statement about hardware elsewhere in the documentation
describes software behaviour only.

## Support tiers

| Backend | Tier | Meaning |
| --- | --- | --- |
| Virtual bus | Supported | Default. Exercised by every test in CI. |
| gs_usb | Primary hardware target | The first backend to be validated on real adapters. Defects found there take priority. |
| SLCAN, PEAK, Kvaser | Experimental | Built in CI and tested against simulated transports or libraries only. Not validated, not maintained to the level of gs_usb, and may change or be removed. |

A backend moves up a tier only with a recorded validation run, never because a
build or a simulated test passes.

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
  disconnected, and reconnecting the adapter should not resume anything.
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
