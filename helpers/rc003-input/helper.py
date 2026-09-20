"""Optional elevated SayAll three-button reader. GPL-3.0-only.

The ordinary main process owns mappings, UI state and fail-safe button release.
The helper observes only its independently verified selected RC003 container.
"""
import argparse
import os
import re
import socket
import threading
import time

from capture import FridaCapture, CaptureError, CAPTURE_DIAGNOSTICS
from controller import InputState
import guard
from protocol import LineDecoder, Mailbox, ProtocolError, encode, token_value

CLEANUP_ERROR_BITS = {"script_stop_failed": 1, "script_unload_failed": 2,
                      "session_detach_failed": 4, "target_close_failed": 8}


def safe_code(error):
    code = getattr(error, "code", None)
    if isinstance(code, str) and re.fullmatch(r"[a-z_]{1,80}", code):
        return code
    return "capture_failed"


class Runtime:
    def __init__(self, mailbox, emit, capture_factory=FridaCapture, clock=time.monotonic):
        self.mailbox = mailbox
        self.emit = emit
        self.factory = capture_factory
        self.clock = clock
        self.state = InputState(emit)
        self.capture = None
        self.capture_lock = threading.Lock()
        self.selection = None
        self.next_attempt = 0.0
        self.next_renew = 0.0
        self.final_cleanup_mask = 0

    def abort(self):
        with self.capture_lock:
            capture = self.capture
        if capture is not None:
            capture.abort()

    def publish_lease(self, lease):
        # Publishing a selection and selecting its old capture is atomic with
        # registration below. Cancel only that captured object after unlocking;
        # a newly registered capture must never inherit this cancellation.
        with self.capture_lock:
            previous = self.mailbox.snapshot()
            self.mailbox.update(lease)
            previous_capture = self.capture if lease != previous else None
        if previous_capture is not None:
            previous_capture.abort()

    def cleanup(self, reason, final=False):
        self.state.stop(reason)
        with self.capture_lock:
            capture, self.capture = self.capture, None
        if capture is not None:
            errors = capture.close()
            # A stop may arrive while step() is cleaning up. Keep that final
            # result when run() subsequently finds no capture left to close.
            if final or self.mailbox.stopped.is_set():
                for code in errors:
                    self.final_cleanup_mask |= CLEANUP_ERROR_BITS.get(code, 16)
            for code in errors:
                self.emit({"type": "diagnostic", "generation": self.state.generation or 0, "code": code})

    def drain_events(self, allow_input=True):
        if self.capture is None:
            return
        for kind, value in self.capture.drain():
            if kind == "diagnostic" and value in CAPTURE_DIAGNOSTICS:
                self.emit({"type": "diagnostic", "generation": self.state.generation or 0, "code": value})
            elif allow_input and kind == "mask":
                self.state.observe(value)
            elif allow_input and kind == "fault":
                raise CaptureError(value)

    def step(self):
        lease = self.mailbox.snapshot()
        if self.mailbox.stopped.is_set() or lease is None:
            return
        selection = (lease.generation, lease.enabled, lease.device_id)
        if selection != self.selection:
            self.cleanup("selection_changed")
            self.selection = selection
            if self.state.generation != lease.generation:
                self.state.sequence = 0
            self.state.generation = lease.generation
            self.next_attempt = 0
            if not lease.enabled:
                self.state.status("waiting", "disabled")
        if not lease.enabled:
            return
        try:
            if self.capture is None:
                if self.clock() < self.next_attempt:
                    return
                self.state.begin(lease.generation)
                capture = self.factory(lease.device_id)
                with self.capture_lock:
                    current = self.mailbox.snapshot()
                    register = not self.mailbox.stopped.is_set() and current == lease
                    if register:
                        self.capture = capture
                if not register:
                    # The factory may have yielded while a new lease arrived.
                    # This object was never attached; dispose outside the lock.
                    for code in capture.close():
                        self.emit({"type": "diagnostic", "generation": lease.generation, "code": code})
                    return
                capture.start()
                self.next_renew = self.clock()
            current = self.mailbox.snapshot()
            if self.mailbox.stopped.is_set() or current != lease:
                self.cleanup("selection_changed")
                return
            self.drain_events()
            if self.clock() >= self.next_renew:
                self.capture.renew()
                self.next_renew = self.clock() + 1.0
        except Exception as error:
            # start() may fail after queuing stage diagnostics. Preserve those
            # facts, but never forward pending input from a failed attachment.
            try:
                self.drain_events(allow_input=False)
            except Exception:
                pass
            self.cleanup(safe_code(error))
            self.next_attempt = self.clock() + 2.0

    def run(self):
        try:
            while not self.mailbox.stopped.wait(0.05):
                self.step()
        finally:
            self.cleanup(self.mailbox.reason, final=True)


def receive(sock, mailbox, emit, parent, runtime):
    decoder = LineDecoder()
    heartbeat_at = 0.0
    try:
        while not mailbox.stopped.is_set():
            lease = mailbox.snapshot()
            if mailbox.stopped.is_set():
                break
            if not parent.alive():
                mailbox.stop("parent_exited")
                break
            try:
                data = sock.recv(4096)
                if not data:
                    mailbox.stop("parent_eof")
                    break
                for incoming in decoder.feed(data):
                    runtime.publish_lease(incoming)
            except socket.timeout:
                pass
            now = time.monotonic()
            if now >= heartbeat_at and not mailbox.stopped.is_set():
                lease = mailbox.snapshot()
                emit({"type": "heartbeat", "generation": lease.generation if lease else 0})
                heartbeat_at = now + 1.0
    except (ProtocolError, OSError, guard.GuardError) as error:
        mailbox.stop(safe_code(error))
    finally:
        runtime.abort()


def arguments(argv=None):
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--token", required=True)
    parser.add_argument("--parent-pid", type=int, required=True)
    args = parser.parse_args(argv)
    if not 0 < args.port <= 65535 or not 0 < args.parent_pid <= 0xffffffff:
        raise ProtocolError("invalid_arguments")
    args.token = token_value(args.token)
    return args


def main(argv=None):
    parent = sock = runtime = reader = None
    mailbox = Mailbox()
    try:
        args = arguments(argv)
        parent = guard.ParentGuard(args.parent_pid)
        guard.verify_listener(args.port, parent.pid)
        sock = socket.create_connection(("127.0.0.1", args.port), timeout=3)
        sock.settimeout(0.25)
        guard.verify_listener(args.port, parent.pid, peer_port=sock.getsockname()[1])
        write_lock = threading.Lock()

        def emit(message):
            if mailbox.stopped.is_set():
                return
            try:
                with write_lock:
                    sock.sendall(encode(message))
            except (OSError, ProtocolError):
                mailbox.stop("ipc_send_failed")

        emit({"type": "hello", "protocol": 1, "token": args.token, "pid": os.getpid()})
        args.token = ""
        guard.enable_debug_privilege()
        runtime = Runtime(mailbox, emit)
        reader = threading.Thread(target=receive, args=(sock, mailbox, emit, parent, runtime), daemon=True)
        reader.start()
        runtime.run()
        return 64 | runtime.final_cleanup_mask if runtime.final_cleanup_mask else 0
    except (Exception, SystemExit) as error:
        # Deliberately no exception body, device identity, token, or file log.
        if sock is not None and not mailbox.stopped.is_set():
            try:
                lease = mailbox.snapshot()
                sock.sendall(encode({"type": "diagnostic", "generation": lease.generation if lease else 0,
                                     "code": safe_code(error)}))
            except Exception:
                pass
        return 1
    finally:
        mailbox.stop("helper_exit")
        if runtime is not None:
            runtime.abort()
        if sock is not None:
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            sock.close()
        if reader is not None:
            reader.join(timeout=1)
        if parent is not None:
            parent.close()


if __name__ == "__main__":
    raise SystemExit(main())
