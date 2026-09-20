import json
from pathlib import Path
import socket
import sys
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from controller import InputState
from bundle_manifest import without_system_ucrt, validate_runtime_files
from helper import Runtime, receive, arguments, main
from capture import FridaCapture, CaptureError, FRIDA_VERSION
import guard
from protocol import LineDecoder, Lease, Mailbox, ProtocolError, decode_line, encode, token_value


class PackagingTests(unittest.TestCase):
    def test_excludes_only_system_ucrt_basename(self):
        keep = [('VCRUNTIME140.dll', 'vc-runtime', 'BINARY'),
                ('api-ms-win-crt-runtime-l1-1-0.dll', 'api-set', 'BINARY'),
                ('nested/my-ucrtbase.dll', 'other-runtime', 'BINARY'),
                ('ucrtbase.dll/data.bin', 'data', 'BINARY')]
        remove = [('ucrtbase.dll', 'system-runtime', 'BINARY'),
                  ('nested\\UCRTBASE.DLL', 'system-runtime', 'BINARY')]
        original = keep + remove
        self.assertEqual(without_system_ucrt(original), keep)
        self.assertEqual(original, keep + remove)

    def test_manifest_requires_other_runtimes_and_rejects_system_ucrt(self):
        entries = [{'path': name} for name in
                   ('_internal/VCRUNTIME140.dll', '_internal/frida/_frida.pyd',
                    f'_internal/python{sys.version_info.major}{sys.version_info.minor}.dll')]
        validate_runtime_files(entries)
        for index in range(len(entries)):
            with self.assertRaisesRegex(ValueError, 'bundle_runtime_missing'):
                validate_runtime_files(entries[:index] + entries[index + 1:])
        with self.assertRaisesRegex(ValueError, 'system_ucrt_must_not_be_bundled'):
            validate_runtime_files(entries + [{'path': '_internal/UCRTBASE.DLL'}])


class StateTests(unittest.TestCase):
    def setUp(self):
        self.events = []
        self.state = InputState(self.events.append)
        self.state.begin(1)

    def test_real_neutral_required_then_first_press_and_duplicates(self):
        self.state.observe(2)
        self.state.observe(2)
        self.assertFalse(self.state.armed)
        self.assertFalse(any(x['type'] == 'state' for x in self.events))
        self.state.observe(0)
        self.state.observe(2)
        self.state.observe(2)
        self.state.observe(0)
        self.assertEqual([x['pressed_mask'] for x in self.events if x['type'] == 'state'], [0, 2, 0])
        self.assertEqual([x['sequence'] for x in self.events if x['type'] == 'state'], [1, 2, 3])

    def test_restart_and_generation_have_no_synthetic_neutral(self):
        self.state.observe(0)
        self.state.observe(7)
        self.state.stop('session_detached')
        self.assertEqual(self.events[-1]['type'], 'status')
        self.state.begin(2)
        self.state.observe(7)
        self.assertFalse(self.state.armed)
        self.state.observe(0)
        self.assertEqual(self.events[-2], dict(type='state', generation=2, sequence=1, pressed_mask=0))

    def test_mask_domain(self):
        for mask in (-1, 8, True, '1', None):
            with self.assertRaises(ValueError): self.state.observe(mask)


class ProtocolTests(unittest.TestCase):
    def test_token_and_cli_are_narrow(self):
        self.assertEqual(token_value('a' * 64), 'a' * 64)
        for token in ('a' * 63, 'g' * 64, None, '../script'):
            with self.assertRaises(ProtocolError): token_value(token)
        with self.assertRaises(ProtocolError): arguments(['--port', '0', '--token', 'a'*64, '--parent-pid', '1'])

    def test_stream_splits_multiple_and_limits(self):
        parser = LineDecoder()
        wire = encode(dict(type='lease', generation=3, enabled=True, device_id='test'))
        self.assertEqual(parser.feed(wire[:8]), [])
        self.assertEqual(parser.feed(wire[8:] + b'{"type":"stop"}\n'), [Lease(3, True, 'test'), None])
        with self.assertRaises(ProtocolError): LineDecoder().feed(b'x' * 4096)
        with self.assertRaises(ProtocolError): LineDecoder().feed(b'x' * 4096 + b'\n')

    def test_schema_rejects_arbitrary_operations(self):
        for payload in ({'type': 'exec', 'script': 'x'}, {'type': 'stop', 'pid': 1},
                        {'type': 'lease', 'generation': True, 'enabled': True, 'device_id': 'x'},
                        {'type': 'lease', 'generation': -1, 'enabled': False, 'device_id': None}):
            with self.assertRaises(ProtocolError): decode_line(json.dumps(payload).encode())
        self.assertEqual(decode_line(b'{"type":"lease","generation":0,"enabled":false,"device_id":null}'), Lease(0, False, ''))

    def test_lease_expiration_and_generation_replay(self):
        now = [0.0]
        box = Mailbox(lambda: now[0])
        box.update(Lease(2, True, 'test'))
        with self.assertRaises(ProtocolError): box.update(Lease(1, True, 'test'))
        now[0] = 4.9
        box.snapshot()
        self.assertFalse(box.stopped.is_set())
        now[0] = 5.0
        box.snapshot()
        self.assertTrue(box.stopped.is_set())

    def test_remote_address_is_last_peer_not_adapter(self):
        self.assertEqual(guard.remote_address('BluetoothLE#BluetoothLE00:11:22:33:44:55-66:77:88:99:AA:BB'), '66778899aabb')
        for value in ('prefix-aa:bb:cc:dd:ee:ff', 'BluetoothLE#00:11:22:33:44:55', 'BluetoothLE#x-66:77:88:99:aa:bb-suffix'):
            with self.assertRaises(guard.GuardError): guard.remote_address(value)


class FakeCapture:
    def __init__(self, selection):
        self.selection = selection
        self.events = []
        self.closes = 0
        self.aborts = 0
        self.starts = 0
        self.fail = False
    def start(self): self.starts += 1
    def renew(self):
        if self.fail: raise guard.GuardError('target_device_identity_changed')
    def drain(self):
        events, self.events = self.events, []
        return events
    def close(self): self.closes += 1; return []
    def abort(self): self.aborts += 1


class CaptureDiagnosticsTests(unittest.TestCase):
    class Cancellation:
        def __enter__(self): return self
        def __exit__(self, *args): return False
        def cancel(self): pass

    def setUp(self):
        frida = SimpleNamespace(__version__=FRIDA_VERSION, Cancellable=self.Cancellation)
        with patch.dict(sys.modules, {'frida': frida}):
            self.capture = FridaCapture('test')

    def test_stage_failures_hide_exception_and_keep_cancellation(self):
        def fail(): raise RuntimeError('private exception body')
        for code in ('capture_attach_failed', 'capture_script_create_failed',
                     'capture_script_load_failed', 'capture_renew_failed'):
            with self.assertRaises(CaptureError) as error:
                self.capture._call(fail, failure_code=code)
            self.assertEqual(str(error.exception), code)
        def cancel():
            self.capture.abort()
            fail()
        with self.assertRaises(CaptureError) as error:
            self.capture._call(cancel, failure_code='capture_attach_failed')
        self.assertEqual(error.exception.code, 'capture_cancelled')
        self.assertIsNone(self.capture.cancellable)

    def test_loaded_message_exports_only_fixed_diagnostic(self):
        self.capture._on_message({'type': 'send', 'payload': {'type': 'loaded'}}, None)
        self.assertEqual(list(self.capture.drain()), [('diagnostic', 'capture_loaded')])


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.now = [0.0]
        self.box = Mailbox(lambda: self.now[0])
        self.events, self.captures = [], []
        def factory(selection):
            capture = FakeCapture(selection)
            self.captures.append(capture)
            return capture
        self.runtime = Runtime(self.box, self.events.append, factory, lambda: self.now[0])

    def test_final_cleanup_records_each_failure_and_combines_bits(self):
        cases = [([], 0), (['script_stop_failed'], 1), (['script_unload_failed'], 2),
                 (['session_detach_failed'], 4), (['target_close_failed'], 8),
                 (['cleanup_failed'], 16),
                 (['script_stop_failed', 'script_unload_failed', 'session_detach_failed',
                   'target_close_failed', 'cleanup_failed'], 31)]
        for errors, mask in cases:
            with self.subTest(mask=mask):
                box = Mailbox()
                runtime = Runtime(box, lambda event: None)
                capture = FakeCapture('test')
                capture.close = Mock(return_value=errors)
                runtime.capture = capture
                box.stop('parent_stop')
                runtime.run()
                self.assertEqual(runtime.final_cleanup_mask, mask)
                capture.close.assert_called_once()

    def test_stop_during_step_cleanup_keeps_final_result(self):
        self.box.update(Lease(1, True, 'test'))
        self.runtime.step()
        def close():
            self.box.stop('parent_stop')
            return ['script_unload_failed']
        self.captures[0].close = close
        self.runtime.cleanup('selection_changed')
        self.runtime.run()
        self.assertEqual(self.runtime.final_cleanup_mask, 2)

    def test_earlier_retry_cleanup_error_does_not_pollute_clean_exit(self):
        self.box.update(Lease(1, True, 'test'))
        self.runtime.step()
        self.captures[0].close = Mock(return_value=['session_detach_failed'])
        self.runtime.cleanup('capture_failed')
        self.assertEqual(self.events[-1]['code'], 'session_detach_failed')
        self.runtime.step()
        self.box.stop('parent_stop')
        self.runtime.run()
        self.assertEqual(self.runtime.final_cleanup_mask, 0)

    def test_capture_diagnostics_keep_generation_without_arming(self):
        self.box.update(Lease(4, True, 'test'))
        self.runtime.step()
        self.captures[0].events = [('diagnostic', code) for code in
                                  ('capture_attach_started', 'capture_attached', 'capture_loaded')]
        self.runtime.step()
        diagnostics = [event for event in self.events if event['type'] == 'diagnostic']
        self.assertEqual([event['code'] for event in diagnostics],
                         ['capture_attach_started', 'capture_attached', 'capture_loaded'])
        self.assertTrue(all(event['generation'] == 4 for event in diagnostics))
        self.assertFalse(self.runtime.state.armed)
        self.assertFalse(any(event['type'] == 'state' for event in self.events))

    def test_failed_start_preserves_stages_and_discards_pending_input(self):
        original_factory = self.runtime.factory
        def factory(device):
            capture = original_factory(device)
            def start():
                capture.events = [('diagnostic', 'capture_attach_started'), ('mask', 0)]
                raise CaptureError('capture_attach_failed')
            capture.start = start
            return capture
        self.runtime.factory = factory
        self.box.update(Lease(5, True, 'test'))
        self.runtime.step()
        self.assertEqual(self.events[-2],
                         dict(type='diagnostic', generation=5, code='capture_attach_started'))
        self.assertEqual(self.events[-1]['reason'], 'capture_attach_failed')
        self.assertFalse(any(event['type'] == 'state' for event in self.events))
        self.assertEqual(self.captures[0].closes, 1)
        self.assertIsNone(self.runtime.capture)

    def test_selection_change_cleans_before_rebind(self):
        self.box.update(Lease(1, True, 'first'))
        self.runtime.step()
        first = self.captures[0]
        first.events = [('mask', 0), ('mask', 1)]
        self.runtime.step()
        self.box.update(Lease(2, True, 'second'))
        self.runtime.step()
        self.assertEqual(first.closes, 1)
        self.assertEqual(self.captures[1].selection, 'second')
        self.assertFalse(self.runtime.state.armed)
        self.assertEqual(self.runtime.state.generation, 2)
        self.assertEqual(self.events[-1]['reason'], 'awaiting_neutral')

    def test_failed_revalidation_cleans_and_backs_off(self):
        self.box.update(Lease(1, True, 'first'))
        self.runtime.step()
        self.captures[0].fail = True
        self.now[0] = 1.0
        self.runtime.step()
        self.assertEqual(self.captures[0].closes, 1)
        self.assertIsNone(self.runtime.capture)
        self.runtime.step()
        self.assertEqual(len(self.captures), 1)
        self.now[0] = 3.0
        self.runtime.step()
        self.assertEqual(len(self.captures), 2)

    def test_disable_does_not_attach_and_cleans_existing(self):
        self.box.update(Lease(1, False, ''))
        self.runtime.step()
        self.assertEqual(self.captures, [])
        self.box.update(Lease(2, True, 'first'))
        self.runtime.step()
        self.box.update(Lease(3, False, ''))
        self.runtime.step()
        self.assertEqual(self.captures[0].closes, 1)
        self.assertEqual(self.events[-1]['reason'], 'disabled')

    def test_parent_eof_aborts_and_cleans_without_injection(self):
        self.box.update(Lease(1, True, 'first'))
        self.runtime.step()
        left, right = socket.socketpair()
        left.settimeout(0.05)
        parent = type('Parent', (), {'alive': lambda self: True})()
        receiver = threading.Thread(target=receive, args=(left, self.box, self.events.append, parent, self.runtime))
        worker = threading.Thread(target=self.runtime.run)
        receiver.start(); worker.start()
        right.close()
        receiver.join(1); worker.join(1); left.close()
        self.assertFalse(receiver.is_alive())
        self.assertFalse(worker.is_alive())
        self.assertTrue(self.box.stopped.is_set())
        self.assertEqual(self.captures[0].closes, 1)
        self.assertGreaterEqual(self.captures[0].aborts, 1)

    def test_same_lease_renewal_never_cancels_capture(self):
        self.runtime.publish_lease(Lease(1, True, 'first'))
        self.runtime.step()
        self.runtime.publish_lease(Lease(1, True, 'first'))
        self.assertEqual(self.captures[0].aborts, 0)

    def test_new_capture_is_not_cancelled_after_lease_publication(self):
        self.runtime.publish_lease(Lease(1, True, 'first'))
        self.runtime.step()
        previous = self.captures[0]
        cancelling, finish_cancel = threading.Event(), threading.Event()
        original_abort = previous.abort
        def delayed_abort():
            original_abort()
            cancelling.set()
            self.assertTrue(finish_cancel.wait(1))
        previous.abort = delayed_abort
        publisher = threading.Thread(target=self.runtime.publish_lease, args=(Lease(2, True, 'second'),))
        publisher.start()
        try:
            self.assertTrue(cancelling.wait(1))
            # Cancellation is outside capture_lock; the new selection can start.
            self.runtime.step()
            self.assertEqual(len(self.captures), 2)
            self.assertEqual(self.captures[1].starts, 1)
            self.assertEqual(self.captures[1].aborts, 0)
            self.assertEqual(previous.closes, 1)
        finally:
            finish_cancel.set()
            publisher.join(1)
            self.runtime.cleanup('test_done')
        self.assertFalse(publisher.is_alive())

    def test_capture_created_for_replaced_lease_never_attaches(self):
        factory_entered, finish_factory = threading.Event(), threading.Event()
        original_factory = self.runtime.factory
        def delayed_factory(device):
            capture = original_factory(device)
            if device == 'first':
                factory_entered.set()
                self.assertTrue(finish_factory.wait(1))
            return capture
        self.runtime.factory = delayed_factory
        self.runtime.publish_lease(Lease(1, True, 'first'))
        worker = threading.Thread(target=self.runtime.step)
        worker.start()
        try:
            self.assertTrue(factory_entered.wait(1))
            self.runtime.publish_lease(Lease(2, True, 'second'))
        finally:
            finish_factory.set()
            worker.join(1)
        self.assertFalse(worker.is_alive())
        self.assertEqual(self.captures[0].starts, 0)
        self.assertEqual(self.captures[0].closes, 1)
        self.assertIsNone(self.runtime.capture)
        self.runtime.step()
        self.assertEqual(self.captures[1].starts, 1)
        self.assertEqual(self.captures[1].aborts, 0)
        self.runtime.cleanup('test_done')


class MainExitTests(unittest.TestCase):
    def test_main_reports_cleanup_mask_and_never_success_on_run_exception(self):
        for mask, failure, expected in ((0, None, 0), (1, None, 65), (31, None, 95),
                                         (0, RuntimeError('private error'), 1),
                                         (31, RuntimeError('private error'), 1)):
            with self.subTest(mask=mask, fatal=failure is not None):
                runtime = Mock(final_cleanup_mask=mask)
                runtime.run.side_effect = failure
                with patch('helper.guard.ParentGuard'), patch('helper.guard.verify_listener'), \
                     patch('helper.guard.enable_debug_privilege'), \
                     patch('helper.socket.create_connection'), patch('helper.threading.Thread'), \
                     patch('helper.Runtime', return_value=runtime):
                    result = main(['--port', '1234', '--token', 'a'*64, '--parent-pid', '1'])
                self.assertEqual(result, expected)


if __name__ == '__main__':
    unittest.main()
