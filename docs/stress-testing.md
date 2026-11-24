# Stress testing

Stress tasks run in the local service, independently of browser rendering. Plans
are validated before explicit transmission authorization. Hardware compatibility
and physical performance remain unverified; the examples below use virtual CAN.

## Run a virtual plan

Build the application as described in the [README](../README.md). Start an
isolated service with no hardware selection arguments:

```sh
./target/debug/canharbor --runtime-dir /tmp/canharbor-stress-demo serve --open
```

In another terminal:

```sh
./target/debug/canharbor --runtime-dir /tmp/canharbor-stress-demo stress validate --plan examples/stress/virtual-classic.json
./target/debug/canharbor --runtime-dir /tmp/canharbor-stress-demo stress run --plan examples/stress/virtual-classic.json --active --request-id demo-001
./target/debug/canharbor --runtime-dir /tmp/canharbor-stress-demo stress report RUN_UUID --output /tmp/stress-report.json
./target/debug/canharbor --runtime-dir /tmp/canharbor-stress-demo stress delete RUN_UUID
```

Replace `RUN_UUID` with the returned run ID. Report export creates a new file and
refuses to overwrite an existing destination. The browser supports plan
validation, explicit start, status, stop, report and evidence downloads.

The same request ID and unchanged plan recover the original run instead of
executing it again. If the creation response is lost, retain that request ID.
CLI disconnection does not cancel an authorized service task; use `stress status`
and `stress stop` to manage it. Ctrl-C requests a stop. No automatic restart or
unknown-outcome retry is performed.

## Plans and reports

### Generate an all-to-all plan offline

`stress generate` writes an existing Classic `Plan` JSON to stdout and checks
static and resource bounds without contacting a service. Select 2–8 distinct
channels. Each selected channel sends to every other selected channel, so six
channels produce six flows and 30 directed peer links.

For a fixed **aggregate** 600 frames/s load lasting 600 seconds:

```sh
canharbor stress generate --channel virtual:0 --channel virtual:1 --aggregate-fps 600 --duration-seconds 600 > plan.json
```

Here each of the two senders is assigned 300 frames/s. With six selected
channels, the same aggregate rate assigns 100 frames/s to each sender. Rates
must divide evenly by the number of channels; the generator rejects rounding.
Fixed loads are represented as one existing stage. For a staircase, repeat
`--stage AGGREGATE_FPS:SECONDS` instead of the fixed-rate options:

```sh
canharbor stress generate --channel virtual:0 --channel virtual:1 --channel virtual:2 --channel virtual:3 --channel virtual:4 --channel virtual:5 --bitrate 1000000 --stage 600:30 --stage 1500:30 --stage 3000:30 --stage 4500:30 --stage 6000:30 > six-node.json
```

The checked-in [six-node example](../examples/stress/six-node-staircase.json)
matches this command. Its channel IDs are replaceable roles: the default
service exposes only `virtual:0` and `virtual:1`. Use the actual selected channel
IDs from `canharbor devices` before validating a hardware plan.

`--first-id` selects the first hexadecimal CAN ID (default `100`); `--extended`
selects 29-bit identifiers. `--first-run-tag` selects the first decimal run tag
(default `1`). IDs and tags increase by one in channel order, with range checks.
Optionally repeat `--alias` once per channel, in the same order, to include
unique role aliases. The generated plan enables ACK-capable receivers and uses
the existing 8-byte Classic tag/sequence payload, scheduler and evidence defaults.

Run `stress validate --plan plan.json` against the intended service to check
channel availability and bitrate capabilities. Starting still requires
`stress run --plan plan.json --active`.

Fixed-count, staged and soak plans freeze configuration, resource budgets and
criteria before execution. Configuration changes require a new validation hash.
The service reserves the required device set before starting acquisition;
partial setup failure rolls back only resources created by that attempt.

Reports distinguish scheduled slots, known submissions, unknown outcomes, receiver
observations, evidence completeness and verdict. Skipped slots do not count as
hardware loss. TX echo does not count as peer RX. A completed driver call is not
proof of physical delivery. Tolerances cannot turn stopped, interrupted or
incompletely observed runs into a pass.

See the feature references:

- [Criteria and verdicts](stress-criteria.md)
- [Timing and software provenance](stress-timing.md)
- [Bidirectional multi-flow plans](stress-multiflow.md)
- [Classic patterns and checkpoints](stress-pattern.md)
- [CAN FD, CRC, RTT and multi-host coordination](stress-protocols.md)
- [Adaptive plans and load shapes](stress-adaptive.md)
- [Tracking windows, queues and benchmark evidence](stress-tracking.md)
- [Stopping, persistence and read-only recovery](stress-recovery.md)

## API

The control surface includes `POST /api/v1/stress/validate`,
`POST /api/v1/stress/runs`, `GET /api/v1/stress/runs`,
`GET /api/v1/stress/requests/{request}`, `GET/DELETE /api/v1/stress/runs/{id}`,
`POST /api/v1/stress/runs/{id}/stop` and report/artifact downloads below the run.
Browser mutations require the existing pairing, Origin and CSRF checks. Native
same-user clients can administer runs; browser access is scoped to its identity.
See the generated schema for payloads and the CLI help for command arguments.

## Verification limits

Software tests use virtual CAN, simulated drivers and local services. Stored
benchmark records include failures and do not establish sustained adapter
throughput. Real-time long runs, physical error injection and external-host
coordination require separate acceptance. See [implementation status](status.md).

## Workbench templates

Short receive check, Rate staircase and One-minute soak reset the editable plan
to a bounded Classic CAN setup. They preserve channel selection, clear prior
validation and transmission authorization, and do not start a run. Review the
sender and expected receivers before validating. Advanced criteria, tracking,
host observations and concurrent flows are collapsed by default; their values
remain part of the plan while closed.
