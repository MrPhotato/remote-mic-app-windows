"""Bounded Frida attachment. No logging, arbitrary targets, or arbitrary scripts."""
import json
from pathlib import Path
import queue
import threading

import guard

FRIDA_VERSION = "17.18.0"
CAPTURE_DIAGNOSTICS = frozenset({"capture_attach_started", "capture_attached", "capture_loaded",
                                 "tv_capture_ready", "tv_report_cleared", "tv_preheld_passthrough",
                                 "tv_buffer_changed", "ok_capture_ready", "ok_report_cleared",
                                 "ok_preheld_passthrough", "ok_buffer_changed"})
CAPTURE_FAULTS = frozenset({"report_read_failed", "lease_expired", "source_initialization_failed",
                          "client_cleanup", "tv_report_write_failed", "tv_report_verify_failed",
                          "tv_report_submit_failed", "tv_report_restore_failed", "tv_source_lost",
                          "tv_report_overlap", "tv_report_invalid",
                          "ok_report_write_failed", "ok_report_verify_failed",
                          "ok_report_submit_failed", "ok_report_restore_failed", "ok_source_lost",
                          "ok_report_overlap", "ok_report_invalid"})


class CaptureError(RuntimeError):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


class FridaCapture:
    def __init__(self, device_id, tv_enabled, ok_enabled):
        if type(tv_enabled) is not bool:
            raise CaptureError("invalid_tv_configuration")
        if type(ok_enabled) is not bool:
            raise CaptureError("invalid_ok_configuration")
        import frida
        if frida.__version__ != FRIDA_VERSION:
            raise CaptureError("frida_version_mismatch")
        self.frida = frida
        self.device_id = device_id
        self.tv_enabled = tv_enabled
        self.ok_enabled = ok_enabled
        self.target = self.session = self.script = None
        self.events = queue.Queue(maxsize=256)
        self.failed = threading.Event()
        self.aborted = threading.Event()
        self.lock = threading.Lock()
        self.cancellable = None
        self.closed = False
        self.cleanup_pending = False
        self.script_load_attempted = False

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
        with self.lock:
            first_abort = not self.aborted.is_set()
            self.aborted.set()
            current = None if self.closed or self.cleanup_pending else self.cancellable
            script = self.script if first_abort and not self.closed else None
        if current is not None:
            current.cancel()
        if script is not None:
            try:
                # Do not block the lease receiver on a synchronous stop RPC.
                # close() still awaits stop before unload and detach.
                script.post({"type": "stop_capture"})
            except Exception:
                self._put(("fault", "capture_stop_signal_failed"))

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
        allowed = 7 | (8 if self.tv_enabled else 0) | (16 if self.ok_enabled else 0)
        if (kind == "mask" and type(payload.get("mask")) is int and payload["mask"] >= 0
                and not payload["mask"] & ~allowed):
            self._put(("mask", payload["mask"]))
        elif kind == "loaded":
            self._put(("diagnostic", "capture_loaded"))
        elif kind == "diagnostic" and payload.get("reason") in CAPTURE_DIAGNOSTICS:
            self._put(("diagnostic", payload["reason"]))
        elif kind in ("fault", "stopped"):
            code = payload.get("reason")
            self._put(("fault", code if code in CAPTURE_FAULTS else "script_stopped"))
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
        source = source.replace("__TV_CAPTURE_ENABLED__", json.dumps(self.tv_enabled))
        source = source.replace("__OK_CAPTURE_ENABLED__", json.dumps(self.ok_enabled))
        self.script = self._call(lambda: self.session.create_script(source),
                                 failure_code="capture_script_create_failed")
        self.script.on("message", self._on_message)
        def load_script():
            self.script_load_attempted = True
            return self.script.load()
        self._call(load_script, failure_code="capture_script_load_failed")

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

    def _stop_failure_cleanup_proof(self):
        try:
            if self.target is not None and guard.target_exited(self.target):
                return "capture_host_exited"
        except Exception:
            pass
        try:
            exports = self._call(self.script.list_exports_sync, seconds=2, cleanup=True)
            if (isinstance(exports, list) and all(isinstance(name, str) for name in exports)
                    and "stop" not in exports):
                # The fixed observer installs stop before any hook. A failed
                # source_binding evaluation therefore has no borrowed TV report.
                return "capture_observer_not_installed"
        except Exception:
            pass
        return None

    def close(self):
        with self.lock:
            if self.closed:
                return []
            self.aborted.set()
            self.cleanup_pending = True
        errors = []
        if self.script is not None and self.script_load_attempted:
            try:
                result = self._call(self.script.exports_sync.stop, seconds=3, cleanup=True)
                if (not isinstance(result, dict) or set(result) != {"quiescent", "cleanup_error"} or
                        result["quiescent"] is not True or
                        result["cleanup_error"] not in (None, "tv_report_restore_failed",
                                                         "ok_report_restore_failed",
                                                         "capture_cleanup_failed")):
                    raise CaptureError("script_stop_failed")
                if result["cleanup_error"] is not None:
                    errors.append(result["cleanup_error"])
            except Exception:
                # The injected observer may still own a temporarily modified
                # report. Retain it and retry stop; never unload its onLeave.
                proof = self._stop_failure_cleanup_proof()
                if proof is None:
                    return ["script_stop_failed"]
                errors.append(proof)
        if self.script is not None:
            try:
                self._call(self.script.unload, seconds=3, cleanup=True)
            except Exception:
                errors.append("script_unload_failed")
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
        with self.lock:
            self.closed = True
            self.cleanup_pending = False
        return errors
