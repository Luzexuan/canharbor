# Implementation status

This page describes the current source tree. Design documents describe intended
behavior and may include work that has not been implemented.

## Available features

- Local Rust service with same-user Unix transport and a paired loopback Web UI.
- Two virtual channels on one simulated bus; no generated background traffic.
- Classic data, Classic RTR and CAN FD frame validation.
- Channel configuration, capability queries, exclusive sessions, expiring control
  leases and generation fencing. Refresh and reconnect do not resume sending.
- CLI and browser monitoring, one-shot transmission, request receipts and bounded
  periodic tasks. The CLI accepts `cansend` frame notation, prints events as
  JSON or text, filters by identifier and generates shell completions.
- Signal decoding in `monitor --format text` from a [DBC file](dbc.md),
  including value tables. The supported subset is listed on that page; the
  browser does not decode signals.
- Service-owned recordings, downloads and a bounded browser offline viewer.
  `capture export` writes a recording as a can-utils candump log or a Vector
  ASCII (ASC) log; BLF is not supported, and no log format can be imported.
- Rust client, C ABI and Python wrapper, including optional native streaming and
  capture download extensions. The Python package registers a
  [python-can interface](../python/README.md#python-can-interface) and is
  installed from the source tree; no wheels are published.
- Optional request and stream logging on the service's standard error
  (`serve --log-requests`), for diagnosing a client or a dropped subscription.
- Stress plans, multi-flow execution, explicit criteria, persisted evidence,
  read-only recovery, FD/RTT, adaptive plans and multi-host coordination.
- Opt-in gs_usb, SLCAN, PEAK and Kvaser backends. Native vendor backends require
  explicit library and channel selection; discovery is not compatibility proof.
  gs_usb is the primary hardware target; SLCAN, PEAK and Kvaser are experimental.
  See [hardware validation](hardware-validation.md) for the tiers, the
  validation procedure and the results recorded so far (none).

See the [README](../README.md) for build instructions and feature documentation.

## Validation boundaries

Virtual CAN models connectivity, not physical arbitration, ACK behavior, wiring,
USB reliability or electrical faults. Driver submission and local TX echo do not
prove delivery to another physical device. Hardware compatibility remains
unverified; software tests use virtual devices and simulated native libraries.

Browser history and event queues are bounded. A gap indicates missing stream
observations. Display pause affects the table, not acquisition or recording.
Fault recovery requires explicit control and start; service restart never
restores transmission.

Stress evidence, receiver completeness and achieved workload are separate facts.
The checked-in [benchmark records](benchmarks/) include failed runs; stable
throughput still requires repeated validation on a controlled host. The tests
that run real-time plans to completion are outside the per-commit checks and run
in a separate nightly workflow, because their result depends on host scheduling;
a run there may repeat a case whose only failure is slots the host skipped.
Virtual-clock
long runs do not establish 24–72 hours of real elapsed-time reliability. Physical
FD/BRS, external SSH/network failures, USB unplug/sleep behavior and independent
analyzer measurements require separate hardware validation.

## Distribution limits

The repository does not include release binaries. The Package workflow can build
unsigned archives with launchers, matching source and checksums, and prepare a
draft for an existing version tag. Signing/notarization, supported macOS-version
validation, Safari acceptance and publication remain release work. CI includes
focused WebKit checks; these do not certify Safari. The service is local-only; remote hosting and LAN access are not
supported. CANHarbor does not implement the Linux SocketCAN kernel ABI or promise
hard real-time scheduling.
