"""Small bounded local IPC protocol. Device selection stays in memory only."""
from dataclasses import dataclass
import json
import re
import threading
import time

MAX_LINE = 4096
LEASE_SECONDS = 5.0
U64_MAX = (1 << 64) - 1


class ProtocolError(RuntimeError):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def token_value(value):
    if not isinstance(value, str) or re.fullmatch(r"[a-fA-F0-9]{64}", value) is None:
        raise ProtocolError("invalid_token")
    return value


@dataclass(frozen=True, repr=False)
class Lease:
    generation: int
    enabled: bool
    device_id: str


def decode_line(line):
    if len(line) > MAX_LINE or b"\x00" in line:
        raise ProtocolError("invalid_frame")
    try:
        message = json.loads(line.decode("utf-8"))
    except (ValueError, UnicodeError):
        raise ProtocolError("invalid_json") from None
    if not isinstance(message, dict):
        raise ProtocolError("invalid_message")
    if message == {"type": "stop"}:
        return None
    if set(message) != {"type", "generation", "enabled", "device_id"} or message["type"] != "lease":
        raise ProtocolError("invalid_message")
    generation, enabled, device = message["generation"], message["enabled"], message["device_id"]
    if type(generation) is not int or not 0 <= generation <= U64_MAX or type(enabled) is not bool:
        raise ProtocolError("invalid_lease")
    if not enabled and device is None:
        device = ""
    if not isinstance(device, str) or len(device) > 1024 or "\x00" in device or (enabled and not device):
        raise ProtocolError("invalid_device_selection")
    return Lease(generation, enabled, device)


class LineDecoder:
    def __init__(self):
        self.buffer = bytearray()

    def feed(self, data):
        self.buffer.extend(data)
        lines = []
        while b"\n" in self.buffer:
            split = self.buffer.index(10)
            if split + 1 > MAX_LINE:
                raise ProtocolError("frame_too_large")
            lines.append(decode_line(bytes(self.buffer[:split])))
            del self.buffer[:split + 1]
        if len(self.buffer) >= MAX_LINE:
            raise ProtocolError("frame_too_large")
        return lines


class Mailbox:
    def __init__(self, clock=time.monotonic):
        self.clock = clock
        self.lock = threading.Lock()
        self.lease = None
        self.received = clock()
        self.stopped = threading.Event()
        self.reason = "stopped"

    def update(self, lease):
        with self.lock:
            if lease is None:
                self.reason = "parent_stop"
                self.stopped.set()
                return
            if self.lease is not None and lease.generation < self.lease.generation:
                raise ProtocolError("stale_generation")
            self.lease = lease
            self.received = self.clock()

    def snapshot(self):
        with self.lock:
            if self.clock() - self.received >= LEASE_SECONDS:
                self.reason = "parent_lease_expired"
                self.stopped.set()
            return self.lease

    def stop(self, reason):
        with self.lock:
            self.reason = reason
            self.stopped.set()


def encode(message):
    data = json.dumps(message, separators=(",", ":"), ensure_ascii=True).encode("ascii") + b"\n"
    if len(data) > MAX_LINE:
        raise ProtocolError("frame_too_large")
    return data
