# Explicit hardware service access

gs_usb is the primary hardware target. SLCAN, PEAK and Kvaser are experimental.
No adapter has been validated; the tiers, the validation procedure and recorded
results are in [hardware validation](hardware-validation.md).

The CLI composes the virtual bus with explicitly selected hardware adapters.
No USB device is opened by default. Enumeration alone sends no gs_usb commands:

```sh
canharbor usb-list --vid-pid 1d50:606f
canharbor serve --usb 1d50:606f --usb-index 0 --usb-interface 0 --usb-alternate 0
```

A candidate can also be selected by serial number, which does not change when
other devices are connected or removed:

```sh
canharbor usb-list --vid-pid 1d50:606f --show-serial
canharbor serve --usb 1d50:606f --usb-serial SERIAL --usb-interface 0 --usb-alternate 0
```

`--usb-serial` must match exactly one enumerated candidate. Devices may report
no serial number or share one; no match and several matches are both startup
errors, and the index form remains available for those devices. Serial numbers
are printed only with `--show-serial`.

These IDs and interface settings are illustrative, not a compatibility claim.
The index is valid only for that enumeration and is not a persistent identity.
Interface opening runs outside the async executor. Each open gets a fresh UUID;
reconnecting does not resume control or transmission. Multiple device composition
uses the core Registry, which rejects duplicate channel IDs.

## Multiple adapters in one service

Select a version 1 JSON startup manifest with:

```sh
canharbor serve --no-web --hardware-manifest /absolute/path/hardware.json
```

[hardware-multiple.json](../examples/hardware-multiple.json) shows one gs_usb and
two SLCAN registrations. Replace its illustrative USB serial, VID/PID, interface,
alternate and COM paths with explicit selections for your adapters. On Unix,
use the serial device paths from your host. Build with the `slcan` feature when
the manifest contains serial adapters. `--hardware-manifest` and the legacy
`--usb`, `--slcan`, PEAK and Kvaser selection flags are mutually exclusive.
The existing single-device commands remain available.

The document requires `version: 1` and an `adapters` array of 1–32 entries, and
is limited to 64 KiB. Unknown fields and unsupported versions are rejected.
Each entry has a unique `name` of 1–31 ASCII letters, digits, underscores or
hyphens. It names a registration's channel namespace. Every actual open appends
a fresh UUID epoch, including after a failure or service restart. Read the new
channel IDs from `canharbor devices --json` before creating sessions.

- `backend: "gs_usb"` requires `vid_pid`, exactly one of `index` or `serial`,
  and explicit numeric `interface` and `alternate` settings. USB candidates are
  resolved from one read-only discovery snapshot per VID/PID before any adapter
  opens. Selecting the same candidate twice, including through an index/serial
  alias or different interface settings, is rejected. Register each USB device
  once; its backend supplies the device's CAN channels.
- `backend: "slcan"` requires `path`, `serial_baud` and an explicit `profile`
  (`lawicel` or `canable-fd`). Optional `polling` defaults to false and is
  supported by LAWICEL only. Supported UART baud settings are 9600, 19200,
  38400, 57600, 115200 and 230400. The profile declares the firmware command
  contract described in [SLCAN](slcan.md). Windows COM aliases and Unix
  canonical-path aliases cannot register the same port twice.

All entries' fields, names, selectors and serial settings are validated before
opening transports. Startup opens only those transports and performs their
existing handshakes. CAN bitrate, FD mode and acquisition still require session
configuration and explicit control/start. If an open fails, startup waits for
previously opened backends' shutdown reports in reverse order before returning
the error. Registry or local-service startup failures also join cleanup. Cleanup
failures and unconfirmed transmissions are reported; writes are never retried.
The service and its runtime directory are created only after all hardware opens.

The manifest records startup selections. Its names and USB serial selectors do
not establish verified physical identity or adapter compatibility. Hardware
acceptance remains scoped by [hardware validation](hardware-validation.md).

The authenticated device API reports backend, simulation status and verification
status. Hardware channels are marked hardware_unverified. The UI shows hardware
access and accepts nominal/FD data bitrates; unsupported configurations are
rejected by the driver. Classic and FD monitor/session commands accept --fd and
--data-bitrate; send uses the same options. Without --fd a physical channel is
configured for Classic CAN. Virtual channels use the same explicit FD-mode selection, with their fixed
nominal/data rates.

Hardware TX receipts report backend_submitted, not virtual acceptance or bus
acknowledgement. Firmware echoes are observations and do not prove ECU delivery.
Only the virtual path has browser end-to-end evidence. A fake USB transport tests
the authenticated HTTP-to-runtime-to-actor path, metadata, receipts and cleanup.
Actual adapter/firmware, hotplug and sleep/wake acceptance remain unverified.

Capability queries, revision-guarded configuration, timestamp/diagnostic streams,
TX receipt querying and shutdown diagnostics are implemented. See
[channel configuration](channel-configuration.md), [device diagnostics](device-diagnostics.md)
and [transmission receipts](transmission-receipts.md). These interfaces report
backend declarations and observations; no physical CAN network was accessed during development.

## Host prerequisites on Linux

These notes follow from how the backends open devices. They have not been
exercised with a physical adapter.

**USB device access.** The gs_usb backend opens the USB device node directly, so
the account running `canharbor serve` needs read/write access to it. On most
distributions the node is writable by root only and opening fails with an access
error. A udev rule can grant access to the locally logged-in user for one
explicit vendor:product pair. Replace the illustrative IDs with those reported
by `canharbor usb-list`:

```text
# /etc/udev/rules.d/70-canharbor.rules
SUBSYSTEM=="usb", ATTR{idVendor}=="1d50", ATTR{idProduct}=="606f", TAG+="uaccess"
```

Reload with `sudo udevadm control --reload-rules`, then reconnect the adapter.
Running the service as root is not a substitute and is not recommended.

**Kernel gs_usb driver.** The Linux kernel includes a `gs_usb` driver that binds
matching adapters and exposes them as SocketCAN interfaces. CANHarbor does not
detach kernel drivers, so claiming the interface fails while that driver holds
it. Releasing the interface is a host administration step that CANHarbor does
not perform, for example by writing the interface's bus ID to
`/sys/bus/usb/drivers/gs_usb/unbind`. While released, the adapter is not
available through SocketCAN, and it is bound again on reconnect.

**Serial devices.** The SLCAN backend opens the given serial device path. The
account needs access to that device, which usually means membership in the group
that owns it (`dialout` on Debian-based systems, `uucp` on some others). Group
changes apply to new login sessions.

The workbench's **Connect a hardware adapter** guide explains discovery and
explicit startup options for each backend. Its list contains registered service
channels, not every attached USB device. Discovery commands and diagnostic export
are read-only; running a generated startup command explicitly opens the selected adapter.
