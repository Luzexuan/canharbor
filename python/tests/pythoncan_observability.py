# SPDX-License-Identifier: AGPL-3.0-or-later
"""Bridge observation tests using a fake client and event stream, without a service."""
import queue
import threading
import time
import unittest
from unittest.mock import patch

import can

from canharbor import CanHarborError
from canharbor.pythoncan import CanHarborBus, _DIAGNOSTIC_LIMIT, _PENDING_LIMIT


class FakeStream:
    def __init__(self):
        self.messages = queue.Queue()
        self.messages.put({"type": "hello"})

    def next(self, *, timeout_ms):
        try:
            message = self.messages.get(timeout=timeout_ms / 1000)
        except queue.Empty:
            return None
        if isinstance(message, Exception):
            raise message
        return message

    def close(self):
        self.messages.put(CanHarborError(4, "stream closed"))

    def push(self, *messages):
        for message in messages:
            self.messages.put(message)


class FakeClient:
    def __init__(self):
        self.stream = FakeStream()
        self.renewals = 0
        self.submissions = []
        self.submit_error = None
        self.closed = False

    def create(self, *args, **kwargs):
        return {"id": "session"}

    def acquire(self, *args):
        return {"generation": "1"}

    def subscribe(self, *args):
        return self.stream

    def start(self, *args):
        pass

    def renew(self, *args):
        self.renewals += 1

    def submit(self, *args, **kwargs):
        self.submissions.append(kwargs)
        if self.submit_error is not None:
            raise self.submit_error

    def request(self, *args, **kwargs):
        pass

    def close(self):
        self.closed = True


def frame(number, *, can_id=0x123, direction="rx"):
    return {"type": "frame_received", "sequence": str(number),
            "host_monotonic_ns": str(1000 + number), "direction": direction,
            "frame": {"kind": "classic_data", "id": can_id, "extended": False,
                      "dlc": 4, "data_hex": number.to_bytes(4, "big").hex()}}


def diagnostic(number, *, overflow=False):
    return {"type": "bus_diagnostic", "sequence": str(number),
            "host_monotonic_ns": str(1000 + number),
            "timestamp_quality": "hardware_unmapped", "timestamp_epoch": "2",
            "device_raw_timestamp_us": 42, "device_timestamp_us": None,
            "error_classes": 0x81, "raw_data_hex": "0102030405060708",
            "overflow": overflow}


def wait_for(condition):
    deadline = time.monotonic() + 5
    while not condition():
        if time.monotonic() >= deadline:
            raise AssertionError("reader did not process the injected events")
        time.sleep(0.001)


class ObservabilityTests(unittest.TestCase):
    def open_bus(self, **kwargs):
        client = FakeClient()
        with patch("canharbor.pythoncan.Client", return_value=client):
            bus = CanHarborBus("virtual:0", **kwargs)
        self.addCleanup(bus.shutdown)
        return bus, client

    def test_gap_diagnostics_filters_and_receive_order(self):
        bus, client = self.open_bus(can_filters=[{"can_id": 0x123, "can_mask": 0x7ff}])
        notice = diagnostic(2)
        first_gap = {"type": "gap", "cause": "subscriber_lag", "count": "9007199254740993"}
        second_gap = {"type": "gap", "cause": "subscriber_lag", "count": "5"}
        client.stream.push(
            {"type": "events", "events": [frame(0, can_id=0x124), frame(1), notice,
                                           frame(2, direction="tx")]},
            first_gap, second_gap, {"type": "events", "events": [frame(3)]})
        wait_for(lambda: bus.health_snapshot()["stream_gaps"] == 2)
        received = [bus.recv(timeout=1), bus.recv(timeout=1)]
        self.assertEqual([bytes(message.data) for message in received],
                         [bytes.fromhex("00000001"), bytes.fromhex("00000003")])
        self.assertIsNone(bus.recv(timeout=0))
        self.assertEqual(bus.read_diagnostics(), [notice, first_gap, second_gap])
        self.assertEqual(bus.read_diagnostics(), [])
        health = bus.health_snapshot()
        self.assertEqual(health["stream_dropped_events"], 9007199254740998)
        self.assertEqual(health["bus_diagnostics"], 1)
        self.assertEqual(health["local_dropped_events"], 0)
        self.assertFalse(health["observation_complete"])
        self.assertFalse(health["unknown_loss"])

    def test_local_buffer_drops_are_exact_across_batches(self):
        bus, client = self.open_bus()
        client.stream.push({"type": "events", "events": [frame(n) for n in range(_PENDING_LIMIT + 3)]})
        wait_for(lambda: bus.health_snapshot()["local_dropped_events"] == 3)
        self.assertEqual(int.from_bytes(bus.recv(timeout=0).data, "big"), 3)
        client.stream.push({"type": "events", "events": [frame(n) for n in range(5)]})
        wait_for(lambda: bus.health_snapshot()["local_dropped_events"] == 7)
        self.assertEqual(int.from_bytes(bus.recv(timeout=0).data, "big"), 8)
        self.assertEqual(len(bus._pending), _PENDING_LIMIT - 1)
        health = bus.health_snapshot()
        self.assertEqual(health["stream_dropped_events"], 0)
        self.assertFalse(health["observation_complete"])
        self.assertFalse(health["unknown_loss"])

    def test_diagnostic_queue_is_bounded_and_overflow_remains_visible(self):
        bus, client = self.open_bus()
        notices = [diagnostic(n, overflow=n == 1) for n in range(_DIAGNOSTIC_LIMIT + 7)]
        client.stream.push({"type": "events", "events": notices})
        wait_for(lambda: bus.health_snapshot()["bus_diagnostics"] == len(notices))
        health = bus.health_snapshot()
        self.assertEqual(health["overflow_diagnostics"], 1)
        self.assertEqual(health["diagnostic_dropped_events"], 7)
        self.assertTrue(health["unknown_loss"])
        self.assertFalse(health["observation_complete"])
        self.assertEqual(bus.read_diagnostics(), notices[7:])
        self.assertIsNone(bus.recv(timeout=0))
        self.assertEqual(bus.health_snapshot(), health)

    def test_session_fault_preserves_buffer_and_raw_diagnostic(self):
        bus, client = self.open_bus()
        fault = {"type": "session_fault", "state": "faulted", "code": "bus_off"}
        client.stream.push({"type": "events", "events": [frame(9)]}, fault)
        wait_for(lambda: bus.health_snapshot()["stream_error"] is not None)
        self.assertEqual(int.from_bytes(bus.recv(timeout=0).data, "big"), 9)
        with self.assertRaises(can.CanOperationError):
            bus.recv(timeout=0)
        health = bus.health_snapshot()
        self.assertEqual(health["session_faults"], 1)
        self.assertEqual(health["last_fault"], "bus_off")
        self.assertTrue(health["unknown_loss"])
        self.assertEqual(bus.read_diagnostics(), [fault])
        bus.shutdown()
        self.assertEqual(bus.health_snapshot(), health)

    def test_unknown_gap_count_and_resume_failure_are_not_zero_loss(self):
        for gap in ({"type": "gap"}, {"type": "gap", "count": "unknown"},
                    {"type": "gap", "count": "-1"}, {"type": "gap", "count": 1.5},
                    {"type": "gap", "count": "18446744073709551616"}):
            with self.subTest(gap=gap):
                bus, client = self.open_bus()
                resume = {"type": "resume_unavailable"}
                client.stream.push(gap, resume)
                wait_for(lambda: bus.health_snapshot()["stream_error"] is not None)
                health = bus.health_snapshot()
                self.assertEqual(health["stream_gaps"], 1)
                self.assertEqual(health["stream_dropped_events"], 0)
                self.assertTrue(health["unknown_loss"])
                self.assertFalse(health["observation_complete"])
                self.assertEqual(bus.read_diagnostics(), [gap, resume])

    def test_snapshot_and_drain_do_not_renew_or_modify_counters(self):
        bus, client = self.open_bus()
        bus._renewed -= 6
        snapshot = bus.health_snapshot()
        self.assertTrue(snapshot["observation_complete"])
        snapshot["local_dropped_events"] = 900
        self.assertEqual(bus.health_snapshot()["local_dropped_events"], 0)
        self.assertEqual(bus.read_diagnostics(), [])
        self.assertEqual(client.renewals, 0)
        self.assertEqual(client.submissions, [])
        self.assertIsNone(bus.recv(timeout=0))
        self.assertEqual(client.renewals, 1)
        reader = bus._reader
        bus.shutdown()
        self.assertFalse(reader.is_alive())
        self.assertTrue(client.closed)
        self.assertTrue(bus.health_snapshot()["observation_complete"])

    def test_send_receive_and_health_reads_are_concurrent(self):
        bus, client = self.open_bus(active=True)
        count = 100
        received, notices, failures = [], [], []
        finished = threading.Event()

        def consume():
            try:
                for _ in range(count):
                    message = bus.recv(timeout=5)
                    self.assertIsNotNone(message)
                    received.append(int.from_bytes(message.data, "big"))
            except Exception as error:
                failures.append(error)

        def inspect():
            previous = 0
            try:
                while not finished.is_set():
                    health = bus.health_snapshot()
                    self.assertGreaterEqual(health["stream_dropped_events"], previous)
                    previous = health["stream_dropped_events"]
                    notices.extend(bus.read_diagnostics())
                    finished.wait(0.001)
            except Exception as error:
                failures.append(error)

        receiver = threading.Thread(target=consume)
        observer = threading.Thread(target=inspect)
        receiver.start()
        observer.start()
        try:
            for n in range(count):
                bus.send(can.Message(arbitration_id=0x321, is_extended_id=False))
                client.stream.push(
                    {"type": "events", "events": [frame(n), diagnostic(n)]},
                    {"type": "gap", "cause": "subscriber_lag", "count": "2"})
            wait_for(lambda: bus.health_snapshot()["stream_gaps"] == count)
            receiver.join(timeout=5)
        finally:
            finished.set()
            observer.join(timeout=5)
        self.assertFalse(receiver.is_alive())
        self.assertFalse(observer.is_alive())
        self.assertEqual(failures, [])
        self.assertEqual(received, list(range(count)))
        self.assertEqual(len(client.submissions), count)
        health = bus.health_snapshot()
        self.assertEqual(health["stream_dropped_events"], count * 2)
        self.assertEqual(health["bus_diagnostics"], count)
        self.assertEqual(health["local_dropped_events"], 0)
        notices.extend(bus.read_diagnostics())
        self.assertEqual(len(notices) + health["diagnostic_dropped_events"], count * 2)

    def test_unknown_submission_still_has_one_attempt(self):
        bus, client = self.open_bus(active=True)
        client.submit_error = CanHarborError(7)
        with self.assertRaisesRegex(can.CanOperationError, "outcome unknown; not retried"):
            bus.send(can.Message(arbitration_id=0x123, is_extended_id=False))
        self.assertEqual(len(client.submissions), 1)


if __name__ == "__main__":
    unittest.main()
