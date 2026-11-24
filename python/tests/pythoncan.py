# SPDX-License-Identifier: AGPL-3.0-or-later
"""Executed by the Rust ABI integration test against a virtual service.

Exits successfully without testing when python-can is not installed.
"""
import sys
import threading
import time
import unittest

try:
    import can
except ImportError:
    print("skipped: python-can is not installed")
    raise SystemExit(0)

from canharbor.pythoncan import CanHarborBus
from pythoncan_observability import ObservabilityTests

checks = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(ObservabilityTests))
assert checks.wasSuccessful(), "python-can observation checks failed"

runtime, library = sys.argv[1:]
common = {"runtime_dir": runtime, "library": library}


def expect(error, call):
    try:
        call()
    except error:
        return
    raise AssertionError(f"expected {error.__name__}")


expect(can.CanInitializationError, lambda: CanHarborBus("virtual:9", **common))
expect(can.CanInitializationError, lambda: CanHarborBus("", **common))

with CanHarborBus("virtual:1", **common) as listener, \
        CanHarborBus("virtual:0", active=True, receive_own_messages=True, **common) as sender:
    assert listener.recv(timeout=0.05) is None
    assert listener.health_snapshot()["observation_complete"]
    assert listener.read_diagnostics() == []
    # A listen-only bus refuses to send instead of changing mode.
    expect(can.CanOperationError, lambda: listener.send(can.Message(arbitration_id=1, is_extended_id=False)))
    # CAN FD needs a bus opened for it.
    expect(can.CanOperationError, lambda: sender.send(
        can.Message(arbitration_id=1, is_extended_id=False, is_fd=True, data=bytes(12))))
    sent = [
        can.Message(arbitration_id=0x123, is_extended_id=False, data=[1, 2, 3]),
        can.Message(arbitration_id=0x1ABCDEF0, is_extended_id=True, data=b""),
        can.Message(arbitration_id=0x7FF, is_extended_id=False, is_remote_frame=True, dlc=4),
    ]
    for message in sent:
        sender.send(message)
    for expected in sent:
        received = listener.recv(timeout=10.0)
        assert received is not None, expected
        assert received.is_rx and not received.is_fd and not received.is_error_frame
        assert received.arbitration_id == expected.arbitration_id
        assert received.is_extended_id == expected.is_extended_id
        assert received.is_remote_frame == expected.is_remote_frame
        assert received.dlc == expected.dlc
        assert bytes(received.data) == bytes(expected.data)
        assert received.channel == "virtual:1"
        assert received.timestamp > 0
        # The sender asked for its own transmissions and sees them marked as such.
        echoed = sender.recv(timeout=10.0)
        assert echoed is not None and not echoed.is_rx
        assert echoed.arbitration_id == expected.arbitration_id
    assert listener.recv(timeout=0.05) is None

with CanHarborBus("virtual:1", fd=True, **common) as listener, \
        CanHarborBus("virtual:0", fd=True, active=True, **common) as sender:
    sender.send(can.Message(arbitration_id=0x321, is_extended_id=False, is_fd=True,
                            bitrate_switch=True, data=bytes(range(12))))
    sender.send(can.Message(arbitration_id=0x322, is_extended_id=False, data=[9]))
    fd = listener.recv(timeout=10.0)
    assert fd is not None and fd.is_fd and fd.bitrate_switch
    assert fd.dlc == 12 and bytes(fd.data) == bytes(range(12))
    classic = listener.recv(timeout=10.0)
    assert classic is not None and not classic.is_fd and bytes(classic.data) == b"\x09"
    # Own transmissions are not delivered unless requested.
    assert sender.recv(timeout=0.2) is None
    # A payload length that is not a CAN FD wire length is rejected, not padded.
    expect(can.CanOperationError, lambda: sender.send(
        can.Message(arbitration_id=1, is_extended_id=False, is_fd=True, data=bytes(9), check=False)))

def wait_for(condition, what):
    deadline = time.monotonic() + 30
    while not condition():
        assert time.monotonic() < deadline, f"timed out waiting for {what}"
        time.sleep(0.01)


# Non-blocking polls deliver frames in order, and control is renewed while
# frames that are already buffered are being consumed.
with CanHarborBus("virtual:1", **common) as listener, \
        CanHarborBus("virtual:0", active=True, **common) as sender:
    for value in range(3):
        sender.send(can.Message(arbitration_id=0x200 + value, is_extended_id=False))
    polled = []
    deadline = time.monotonic() + 30
    while len(polled) < 3 and time.monotonic() < deadline:
        message = listener.recv(timeout=0)
        if message is not None:
            polled.append(message.arbitration_id)
    assert polled == [0x200, 0x201, 0x202], polled
    assert listener.recv(timeout=0) is None

    renewals = []
    renew = listener._client.renew
    listener._client.renew = lambda *args: (renewals.append(args), renew(*args))[1]
    for value in range(2):
        sender.send(can.Message(arbitration_id=0x300 + value, is_extended_id=False))
    wait_for(lambda: len(listener._pending) == 2, "two buffered frames")
    assert listener.recv(timeout=0).arbitration_id == 0x300
    listener._renewed -= 6  # as if the previous frame took six seconds to process
    before = len(renewals)
    assert listener.recv(timeout=0).arbitration_id == 0x301
    assert len(renewals) == before + 1, renewals

# A bus that only sends keeps its subscription. The service disconnects a
# subscriber that does not read for two seconds while it has events to deliver,
# and on macOS the local socket holds only 8 KiB of them. The peer's frames
# and the echoes of this bus's own frames exceed that several times over here.
with CanHarborBus("virtual:1", active=True, **common) as peer, \
        CanHarborBus("virtual:0", active=True, **common) as sender:
    peer.send(can.Message(arbitration_id=0x400, is_extended_id=False))
    started = time.monotonic()
    rounds = 0
    while time.monotonic() - started < 3.5:
        sender.send(can.Message(arbitration_id=0x401, is_extended_id=False, data=bytes(8)))
        peer.send(can.Message(arbitration_id=0x402, is_extended_id=False, data=bytes(8)))
        peer.recv(timeout=0)  # the peer is not under test; keep its buffer small
        rounds += 1
        time.sleep(0.03)  # stay under the service's transmit rate limit
    # How many rounds fit depends on the host, so there is no floor on it. What
    # matters is the time without a read, which the loop above fixes.
    # Nothing was read by the program so far; everything is still delivered.
    first = sender.recv(timeout=10.0)
    assert first is not None and first.arbitration_id == 0x400
    received = 1
    while received < rounds + 1:
        message = sender.recv(timeout=10.0)
        assert message is not None and message.arbitration_id == 0x402, (received, message)
        received += 1
    assert sender.recv(timeout=0) is None

# send() on one thread and recv() on another.
with CanHarborBus("virtual:1", active=True, **common) as peer, \
        CanHarborBus("virtual:0", active=True, **common) as bus:
    failures, received = [], []
    rounds = 40

    def receive():
        # Keep reading until every frame the peer sent has arrived. The wait is
        # bounded generously: a loaded host must not turn it into a failure.
        try:
            deadline = time.monotonic() + 60
            while received.count(0x601) < rounds and time.monotonic() < deadline:
                message = bus.recv(timeout=0.2)
                if message is not None:
                    received.append(message.arbitration_id)
        except Exception as error:  # reported by the main thread
            failures.append(error)

    reader = threading.Thread(target=receive)
    reader.start()
    for _ in range(rounds):
        bus.send(can.Message(arbitration_id=0x600, is_extended_id=False))
        peer.send(can.Message(arbitration_id=0x601, is_extended_id=False))
        time.sleep(0.03)  # stay under the service's transmit rate limit
    reader.join()
    assert not failures, failures
    assert received.count(0x601) == rounds, (rounds, len(received))

# A stream that ends is reported by recv(), after the frames already buffered.
with CanHarborBus("virtual:1", **common) as listener, \
        CanHarborBus("virtual:0", active=True, **common) as sender:
    sender.send(can.Message(arbitration_id=0x700, is_extended_id=False))
    wait_for(lambda: len(listener._pending) == 1, "one buffered frame")
    listener._stream.close()
    wait_for(lambda: listener._stream_error is not None, "the reader to stop")
    assert listener.health_snapshot()["unknown_loss"]
    assert not listener.health_snapshot()["observation_complete"]
    assert listener.recv(timeout=0).arbitration_id == 0x700
    expect(can.CanOperationError, lambda: listener.recv(timeout=0))

# Both channels are free again after the buses were shut down.
with CanHarborBus("virtual:0", **common), CanHarborBus("virtual:1", **common):
    pass
print("python-can interface checks passed")
