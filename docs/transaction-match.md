# Response matching component

`canharbor_tasks::transaction_match` provides a public, synchronous matcher
over `canharbor_core::Frame`. A rule selects one CAN identifier (including its
11/29-bit width), Classic data, Classic remote or FD data with explicit BRS,
an exact length and up to 64 byte-mask checks. Data lengths are payload bytes;
a remote rule's length is its requested DLC, and it has no payload checks.

An optional sequence field selects one through eight payload bytes, in either
byte order. It must be a value that the device echoes from the request.
A device's independent response counter does not establish that relationship.

```rust
use canharbor_core::{CanId, Frame};
use canharbor_tasks::transaction_match::{
    ByteOrder, FrameKind, Matcher, Outcome, Rule, SequenceField,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id = CanId::new(0x321, false)?;
    let mut matcher = Matcher::new(Rule {
        id,
        kind: FrameKind::ClassicData,
        length: 2,
        bytes: vec![],
        sequence: Some(SequenceField {
            offset: 0,
            width: 2,
            byte_order: ByteOrder::BigEndian,
        }),
    }, 16)?;
    matcher.register(100, Some(7))?;
    let reply = Frame::classic(id, vec![0, 7])?;
    assert_eq!(matcher.observe(&reply)?, Outcome::Matched { request_id: 100 });
    Ok(())
}
```

Register before the caller submits a request and supply only observed RX frames,
in observation order, to `observe`. `retire` marks a request that the caller has
stopped waiting for, while retaining its possible late response. `Matched`
identifies the first matching response for a retained request, including a retired
request. `Duplicate` identifies a later one. The caller compares its own recorded
times with its business deadline to classify a response as valid or late.

The window is one through 1024 requests. A full window rejects a registration if
all entries are pending. Otherwise it evicts one completed entry. Sequence values
must increase strictly, fit the declared width and never wrap or be reused during
the matcher's lifetime. A reply for an evicted sequence is unrelated to the
retained window, and no longer has a duplicate classification.

Without an echoed sequence, a matching reply is ambiguous whenever multiple
requests could have produced it, including already answered or retired requests.
It also stays ambiguous after any history is evicted. Such an observation does
not consume a pending request or receive an arbitrary request ID. Consequently,
repeating the same unsequenced rule cannot establish reliable per-request RTT
after its first request. A timeout guard or constructing a new matcher does not
prove that earlier or unsolicited responses are absent from the bus. Callers
must retain these limits in their results.

`Ambiguous` still indicates that the frame passed the ID, shape and byte checks.
An executor can count it as a pattern-matched observation with unknown request
ownership. CAN frame-loss accounting and the decision to stop require their
own evidence and explicit policy.

The component owns no Runtime, subscription, clock or hardware connection.
It neither starts nor retries a request, identifies TX echoes, decodes signals
nor reads device health. A matched frame establishes correlation within the
declared observation scope. It does not prove physical delivery or a successful
device action. Invalid rules, sequence fields, windows and raw Frame variants
are rejected before changing state. Retained memory is bounded by the window and
the fixed rule limits, and observing frames allocates no retained payload storage.

Run the hardware-free component tests with:

```sh
cargo test -p canharbor-tasks --locked --test transaction_match
```
