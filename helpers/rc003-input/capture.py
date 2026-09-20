"""Bounded Frida attachment. No logging, arbitrary targets, or arbitrary scripts."""
import json
from pathlib import Path
import queue
import threading

import guard

FRIDA_VERSION = "17.18.0"
CAPTURE_DIAGNOSTICS = frozenset({"capture_attach_started", "capture_attached", "capture_loaded"})


class CaptureError(RuntimeError):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


class FridaCapture:
    def __init__(self, device_id):
        import frida
        if frida.__version__ != FRIDA_VERSION:
            raise CaptureError("frida_version_mismatch")
        self.frida = frida
        self.device_id = device_id
        self.target = self.session = self.script = None
        self.events = queue.Queue(maxsize=256)
        self.failed = threading.Event()
        self.aborted = threading.Event()
        self.lock = threading.Lock()
        self.cancellable = None
        self.closed = False

    def _call(self, call, seconds=5, cleanup=False, failure_code=None):
        cancellation = self.frida.Cancellable()
        with self.lock:
            if self.aborted.is_set() and not cleanup:
                raise CaptureError("capture_cancelled")
            self.cancellable = cancellation
        timer = threading.Timer(seconds, cancellation.cancel)
        timer.daemon = True
        timer.start()
        try:
            with cancellation:
                return call()
        except Exception:
            if self.aborted.is_set() and not cleanup:
                raise CaptureError("capture_cancelled") from None
            if failure_code is not None:
                raise CaptureError(failure_code) from None
            raise
        finally:
            timer.cancel()
            with self.lock:
                if self.cancellable is cancellation:
                    self.cancellable = None

    def abort(self):
        self.aborted.set()
        with self.lock:
            current = None if self.closed else self.cancellable
        if current is not None:
            current.cancel()

    def _put(self, value):
        try:
            self.events.put_nowait(value)
        except queue.Full:
            self.failed.set()

    def _on_message(self, message, _data):
        if message.get("type") != "send" or not isinstance(message.get("payload"), dict):
            self._put(("fault", "script_error"))
            return
        payload = message["payload"]
        kind = payload.get("type")
        if kind == "mask" and type(payload.get("mask")) is int and 0 <= payload["mask"] <= 7:
            self._put(("mask", payload["mask"]))
        elif kind == "loaded":
            self._put(("diagnostic", "capture_loaded"))
        elif kind in ("fault", "stopped"):
            code = payload.get("reason")
            allowed = {"report_read_failed", "lease_expired", "source_initialization_failed", "client_cleanup"}
            self._put(("fault", code if code in allowed else "script_stopped"))
        else:
            self._put(("fault", "invalid_script_message"))

    def start(self):
        self.target = guard.select_for_device(self.device_id)
        guard.revalidate(self.target)
        self._put(("diagnostic", "capture_attach_started"))
        self.session = self._call(lambda: self.frida.get_local_device().attach(self.target.pid),
                                  failure_code="capture_attach_failed")
        self._put(("diagnostic", "capture_attached"))
        self.session.on("detached", lambda _reason, _crash: self._put(("fault", "session_detached")))
        guard.revalidate(self.target)
        root = Path(__file__).resolve().parent
        source = (root / "source_binding.js").read_text(encoding="utf-8") + "\n" + (root / "observer.js").read_text(encoding="utf-8")
        source = source.replace("__SELECTED_SOURCE_KEY__", json.dumps(self.target.binding_key()))
        self.script = self._call(lambda: self.session.create_script(source),
                                 failure_code="capture_script_create_failed")
        self.script.on("message", self._on_message)
        self._call(self.script.load, failure_code="capture_script_load_failed")

    def renew(self):
        if self.failed.is_set():
            raise CaptureError("capture_queue_overflow")
        guard.revalidate(self.target)
        if guard.container_for_device(self.device_id) != self.target._container:
            raise CaptureError("selected_device_container_changed")
        if not self._call(self.script.exports_sync.renew, seconds=2, failure_code="capture_renew_failed"):
            raise CaptureError("script_lease_lost")

    def drain(self):
        if self.failed.is_set():
            raise CaptureError("capture_queue_overflow")
        while True:
            try:
                yield self.events.get_nowait()
            except queue.Empty:
                return

    def close(self):
        with self.lock:
            if self.closed:
                return []
            self.closed = True
        errors = []
        if self.script is not None:
            for code, call in (("script_stop_failed", self.script.exports_sync.stop),
                               ("script_unload_failed", self.script.unload)):
                try:
                    self._call(call, seconds=3, cleanup=True)
                except Exception:
                    errors.append(code)
            self.script = None
        if self.session is not None:
            try:
                self._call(self.session.detach, seconds=3, cleanup=True)
            except Exception:
                errors.append("session_detach_failed")
            self.session = None
        if self.target is not None:
            try:
                guard.close_target(self.target)
            except Exception:
                errors.append("target_close_failed")
            self.target = None
        self.device_id = ""
        return errors
