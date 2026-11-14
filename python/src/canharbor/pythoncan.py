# SPDX-License-Identifier: AGPL-3.0-or-later
"""python-can interface for a running CANHarbor service.

    import can
    with can.Bus(interface="canharbor", channel="virtual:0", active=True) as bus:
        bus.send(can.Message(arbitration_id=0x123, data=[1, 2], is_extended_id=False))
        print(bus.recv(timeout=1.0))

The bus opens one session on the channel, acquires control and starts it. It
keeps the service's rules: a bus is listen-only unless `active=True`, nothing is
retried, and control is renewed only from `send()` and `recv()`. A program that
stops calling them lets the lease expire, which stops the channel.

A background thread reads the event stream into a bounded buffer. The service
disconnects a subscriber that stops reading while traffic continues, so the
stream cannot wait for `recv()`. The thread only reads: it never renews
control and never sends.
"""
from __future__ import annotations

import threading
import time
import uuid
from collections import deque
from typing import Any, Optional

import can

from . import CanHarborError, Client

__all__ = ["CanHarborBus"]

_RENEW_SECONDS = 5.0
# Frames buffered while the program is not calling recv(). Older ones are
# discarded first, as a full driver receive buffer would.
_PENDING_LIMIT = 16384


class CanHarborBus(can.BusABC):
    def __init__(self, channel: str, *, bitrate: int = 500000, fd: bool = False,
                 data_bitrate: int = 2000000, active: bool = False,
                 receive_own_messages: bool = False,
                 runtime_dir: Optional[str] = None, library: Optional[str] = None,
                 **kwargs: Any):
        if not isinstance(channel, str) or not channel:
            raise can.CanInitializationError("channel must be a CANHarbor channel ID such as 'virtual:0'")
        self._client: Optional[Client] = None
        self._stream = None
        self._session: Optional[str] = None
        self._generation: Optional[str] = None
        self._channel = channel
        self._active = bool(active)
        self._fd = bool(fd)
        self._own = bool(receive_own_messages)
        self._pending: deque = deque(maxlen=_PENDING_LIMIT)
        # Guards the buffer and the stream state shared with the reader thread.
        self._arrived = threading.Condition()
        self._stream_error: Optional[str] = None
        self._closing = False
        self._reader: Optional[threading.Thread] = None
        self._renewed = 0.0
        self.channel_info = f"CANHarbor {channel}"
        try:
            self._client = Client(runtime_dir, library=library)
            created = self._client.create(channel, bitrate=bitrate, active=self._active,
                                          data_bitrate=data_bitrate if self._fd else None)
            self._session = created["id"]
            self._generation = self._client.acquire(self._session)["generation"]
            self._renewed = time.monotonic()
            # Subscribe before starting so that no frame precedes the stream.
            self._stream = self._client.subscribe(self._session)
            hello = self._stream.next(timeout_ms=1000)
            if not hello or hello.get("type") != "hello":
                raise can.CanInitializationError("CANHarbor event stream did not start")
            self._client.start(self._session, self._generation)
            self._reader = threading.Thread(
                target=self._read_stream, args=(self._stream,),
                name="canharbor-bus-stream", daemon=True)
            self._reader.start()
        except (CanHarborError, ValueError, OSError) as error:
            self._teardown()
            raise can.CanInitializationError(f"cannot open CANHarbor channel {channel}: {error}") from error
        except Exception:
            self._teardown()
            raise
        super().__init__(channel=channel, **kwargs)

    def _renew(self) -> None:
        if time.monotonic() - self._renewed < _RENEW_SECONDS:
            return
        try:
            self._client.renew(self._session, self._generation)
        except CanHarborError as error:
            raise can.CanOperationError(f"CANHarbor control was lost: {error}") from error
        self._renewed = time.monotonic()

    def _recv_internal(self, timeout: Optional[float]):
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            # Renew on every pass: a consumer can spend longer than the lease
            # on frames that are already buffered.
            self._renew()
            with self._arrived:
                while self._pending:
                    message = self._message(self._pending.popleft())
                    if message is not None:
                        return message, False
                if self._stream_error is not None:
                    raise can.CanOperationError(self._stream_error)
                remaining = 1.0 if deadline is None else deadline - time.monotonic()
                if remaining <= 0:
                    return None, False
                # Wake at least once a second so that control is renewed while
                # waiting without a timeout.
                self._arrived.wait(min(remaining, 1.0))

    def _read_stream(self, stream) -> None:
        """Reader thread: move stream messages into the buffer until the stream ends."""
        error = None
        while not self._closing:
            try:
                batch = stream.next(timeout_ms=200)
            except CanHarborError as failure:
                if not self._closing:
                    error = f"CANHarbor event stream failed: {failure}"
                break
            if batch is None:
                continue
            kind = batch.get("type")
            if kind == "events":
                with self._arrived:
                    self._pending.extend(batch.get("events", ()))
                    self._arrived.notify_all()
            elif kind == "session_fault":
                error = f"CANHarbor channel fault: {batch.get('code')}"
                break
            elif kind == "resume_unavailable":
                error = "CANHarbor event stream history is unavailable"
                break
            # A gap means observations were dropped for this subscriber; later
            # frames are still valid, so reception continues.
        with self._arrived:
            # Frames already buffered are still delivered before the error.
            self._stream_error = error if error is not None else self._stream_error
            if error is None and not self._closing:
                self._stream_error = "CANHarbor event stream ended"
            self._arrived.notify_all()

    def _message(self, event: dict) -> Optional[can.Message]:
        if event.get("type") != "frame_received":
            return None
        own = event.get("direction") == "tx"
        if own and not self._own:
            return None
        frame = event["frame"]
        kind = frame["kind"]
        data = bytes.fromhex(frame.get("data_hex", ""))
        return can.Message(
            timestamp=int(event["host_monotonic_ns"]) / 1e9,
            arbitration_id=frame["id"],
            is_extended_id=bool(frame["extended"]),
            is_remote_frame=kind == "classic_remote",
            is_fd=kind == "fd_data",
            bitrate_switch=bool(frame.get("brs")),
            error_state_indicator=bool(frame.get("esi")),
            # python-can's dlc is a byte count; the service reports the DLC code,
            # which differs for CAN FD payloads longer than eight bytes.
            dlc=frame["dlc"] if kind == "classic_remote" else len(data),
            data=data,
            channel=self._channel,
            is_rx=not own,
            check=False,
        )

    def send(self, msg: can.Message, timeout: Optional[float] = None) -> None:
        if not self._active:
            raise can.CanOperationError("this bus is listen-only; open it with active=True to send")
        if msg.is_error_frame:
            raise can.CanOperationError("error frames cannot be transmitted")
        if msg.is_fd and not self._fd:
            raise can.CanOperationError("open the bus with fd=True to send CAN FD frames")
        if msg.is_remote_frame:
            frame = {"kind": "classic_remote", "data_hex": "", "dlc": msg.dlc}
        else:
            frame = {"kind": "fd_data" if msg.is_fd else "classic_data",
                     "data_hex": bytes(msg.data).hex(), "brs": bool(msg.bitrate_switch)}
        frame.update(id=msg.arbitration_id, extended=bool(msg.is_extended_id))
        self._renew()
        request = uuid.uuid4().hex
        try:
            self._client.submit(self._session, self._generation, request_id=request, frame=frame)
        except CanHarborError as error:
            # Status 7 means the outcome is unknown. It is reported, never retried.
            outcome = "outcome unknown; not retried" if error.status == 7 else "not sent"
            raise can.CanOperationError(
                f"CANHarbor submission {outcome} (session {self._session}, request {request}): {error}"
            ) from error

    def _teardown(self) -> None:
        client, self._client = self._client, None
        stream, self._stream = self._stream, None
        if client is None:
            return
        # Closing the stream interrupts the reader's pending read.
        self._closing = True
        reader, self._reader = self._reader, None
        for step in (
            lambda: stream and stream.close(),
            lambda: reader and reader.join(timeout=5),
            lambda: self._session and self._generation
            and client.request("DELETE", f"/sessions/{self._session}", {}, generation=self._generation),
            client.close,
        ):
            try:
                step()
            except (CanHarborError, OSError):
                # Best effort: an expired lease has already stopped the channel.
                pass

    def shutdown(self) -> None:
        super().shutdown()
        self._teardown()
