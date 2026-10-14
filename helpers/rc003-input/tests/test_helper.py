import json
from pathlib import Path
import socket
import sys
import threading
import time
import unittest
import uuid
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from controller import InputState
from bundle_manifest import without_system_ucrt, validate_runtime_files
from helper import Runtime, receive, arguments, main
from capture import FridaCapture, CaptureError, FRIDA_VERSION, CAPTURE_DIAGNOSTICS, CAPTURE_FAULTS
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

    def test_tv_held_at_start_is_not_a_neutral_three_key_report(self):
        self.state.begin(2, True)
        for mask in (8, 8, 9, 8):
            self.state.observe(mask)
        self.assertFalse(self.state.armed)
        self.assertFalse(any(event['type'] == 'state' for event in self.events))
        for mask in (0, 8, 8, 15, 0):
            self.state.observe(mask)
        self.assertEqual([event['pressed_mask'] for event in self.events if event['type'] == 'state'],
                         [0, 8, 15, 0])

    def test_stopping_held_tv_cancels_without_a_click_release(self):
        self.state.begin(2, True)
        self.state.observe(0)
        self.state.observe(8)
        before = len(self.events)
        self.state.stop('tv_source_lost')
        self.assertEqual(self.events[before:], [dict(type='status', generation=2,
                                                    phase='waiting', reason='tv_source_lost')])
        self.state.begin(3, True)
        self.state.observe(8)
        self.assertFalse(self.state.armed)
        self.state.observe(0)
        self.assertTrue(self.state.armed)

    def test_tv_configuration_and_mask_domain_fail_closed(self):
        for value in (1, 0, None, 'true'):
            with self.assertRaises(ValueError): self.state.begin(2, value)
        self.state.begin(2, True)
        for mask in (-1, 16, True, '8', None):
            with self.assertRaises(ValueError): self.state.observe(mask)
        self.state.begin(3, False)
        with self.assertRaises(ValueError): self.state.observe(8)


class ProtocolTests(unittest.TestCase):
    def test_token_and_cli_are_narrow(self):
        self.assertEqual(token_value('a' * 64), 'a' * 64)
        for token in ('a' * 63, 'g' * 64, None, '../script'):
            with self.assertRaises(ProtocolError): token_value(token)
        with self.assertRaises(ProtocolError): arguments(['--port', '0', '--token', 'a'*64, '--parent-pid', '1'])

    def test_stream_splits_multiple_and_limits(self):
        parser = LineDecoder()
        wire = encode(dict(type='lease', generation=3, enabled=True, device_id='test', tv_enabled=False))
        self.assertEqual(parser.feed(wire[:8]), [])
        self.assertEqual(parser.feed(wire[8:] + b'{"type":"stop"}\n'), [Lease(3, True, 'test', False), None])
        with self.assertRaises(ProtocolError): LineDecoder().feed(b'x' * 4096)
        with self.assertRaises(ProtocolError): LineDecoder().feed(b'x' * 4096 + b'\n')

    def test_schema_rejects_arbitrary_operations(self):
        for payload in ({'type': 'exec', 'script': 'x'}, {'type': 'stop', 'pid': 1},
                        {'type': 'lease', 'generation': True, 'enabled': True, 'device_id': 'x', 'tv_enabled': False},
                        {'type': 'lease', 'generation': -1, 'enabled': False, 'device_id': None, 'tv_enabled': False}):
            with self.assertRaises(ProtocolError): decode_line(json.dumps(payload).encode())
        self.assertEqual(decode_line(b'{"type":"lease","generation":0,"enabled":false,"device_id":null,"tv_enabled":false}'), Lease(0, False, '', False))

    def test_lease_expiration_and_generation_replay(self):
        now = [0.0]
        box = Mailbox(lambda: now[0])
        box.update(Lease(2, True, 'test', False))
        with self.assertRaises(ProtocolError): box.update(Lease(1, True, 'test', False))
        now[0] = 4.9
        box.snapshot()
        self.assertFalse(box.stopped.is_set())
        now[0] = 5.0
        box.snapshot()
        self.assertTrue(box.stopped.is_set())

    def test_tv_option_is_required_and_must_be_a_boolean(self):
        message = dict(type='lease', generation=3, enabled=True, device_id='test')
        with self.assertRaises(ProtocolError): decode_line(json.dumps(message).encode())
        for invalid in (None, 0, 1, 'true', []):
            with self.assertRaises(ProtocolError):
                decode_line(json.dumps({**message, 'tv_enabled': invalid}).encode())
        self.assertEqual(decode_line(json.dumps({**message, 'tv_enabled': True}).encode()),
                         Lease(3, True, 'test', True))

    def test_same_generation_cannot_change_device_or_tv_permission(self):
        box = Mailbox()
        original = Lease(3, True, 'test', False)
        box.update(original)
        box.update(original)
        for changed in (Lease(3, True, 'test', True), Lease(3, True, 'other', False),
                        Lease(3, False, '', False)):
            with self.assertRaisesRegex(ProtocolError, 'selection_changed_without_generation'):
                box.update(changed)
            self.assertEqual(box.snapshot(), original)
        box.update(Lease(4, True, 'test', True))
        self.assertTrue(box.snapshot().tv_enabled)

    def test_remote_address_is_last_peer_not_adapter(self):
        self.assertEqual(guard.remote_address('BluetoothLE#BluetoothLE00:11:22:33:44:55-66:77:88:99:AA:BB'), '66778899aabb')
        for value in ('prefix-aa:bb:cc:dd:ee:ff', 'BluetoothLE#00:11:22:33:44:55', 'BluetoothLE#x-66:77:88:99:aa:bb-suffix'):
            with self.assertRaises(guard.GuardError): guard.remote_address(value)


class FakeCapture:
    def __init__(self, selection, tv_enabled=False):
        self.selection = selection
        self.tv_enabled = tv_enabled
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
            self.capture = FridaCapture('test', False)

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

    def test_tv_messages_are_narrow_and_respect_configuration(self):
        self.capture._on_message({'type': 'send', 'payload': {'type': 'mask', 'mask': 8}}, None)
        self.assertEqual(list(self.capture.drain()), [('fault', 'invalid_script_message')])
        self.capture.tv_enabled = True
        for mask in (0, 8, 15):
            self.capture._on_message({'type': 'send', 'payload': {'type': 'mask', 'mask': mask}}, None)
        self.assertEqual(list(self.capture.drain()), [('mask', 0), ('mask', 8), ('mask', 15)])
        for code in sorted(CAPTURE_FAULTS):
            self.capture._on_message({'type': 'send', 'payload': {'type': 'fault', 'reason': code}}, None)
        self.assertEqual(list(self.capture.drain()), [('fault', code) for code in sorted(CAPTURE_FAULTS)])
        for code in sorted(CAPTURE_DIAGNOSTICS):
            self.capture._on_message({'type': 'send', 'payload': {'type': 'diagnostic', 'reason': code}}, None)
        self.assertEqual(list(self.capture.drain()),
                         [('diagnostic', code) for code in sorted(CAPTURE_DIAGNOSTICS)])
        self.capture._on_message({'type': 'send', 'payload': {'type': 'fault',
                                                            'reason': 'private device details'}}, None)
        self.capture._on_message({'type': 'send', 'payload': {'type': 'diagnostic',
                                                            'reason': 'private device details'}}, None)
        self.assertEqual(list(self.capture.drain()), [('fault', 'script_stopped'),
                                                     ('fault', 'invalid_script_message')])

    def test_abort_signals_script_once_and_never_waits_on_stop_rpc(self):
        script = Mock()
        self.capture.script = script
        self.capture.abort()
        self.capture.abort()
        script.post.assert_called_once_with({'type': 'stop_capture'})
        script.exports_sync.stop.assert_not_called()
        self.assertTrue(self.capture.aborted.is_set())

    def test_failed_stop_signal_does_not_expose_exception_details(self):
        self.capture.script = Mock()
        self.capture.script.post.side_effect = RuntimeError('private device details')
        self.capture.abort()
        self.assertEqual(list(self.capture.drain()), [('fault', 'capture_stop_signal_failed')])

    def test_stop_timeout_retains_observer_until_quiescence_is_confirmed(self):
        script, session = Mock(), Mock()
        self.capture.script, self.capture.session = script, session
        self.capture.script_load_attempted = True
        script.exports_sync.stop.side_effect = [RuntimeError('private timeout'),
                                                dict(quiescent=True, cleanup_error=None)]
        self.assertEqual(self.capture.close(), ['script_stop_failed'])
        self.assertTrue(self.capture.cleanup_pending)
        self.assertFalse(self.capture.closed)
        self.assertIs(self.capture.script, script)
        self.assertIs(self.capture.session, session)
        script.unload.assert_not_called()
        session.detach.assert_not_called()
        self.assertEqual(self.capture.close(), [])
        script.unload.assert_called_once()
        session.detach.assert_called_once()
        self.assertFalse(self.capture.cleanup_pending)
        self.assertTrue(self.capture.closed)

    def test_quiescent_restore_failure_is_reported_but_can_unload(self):
        for code in ('tv_report_restore_failed', 'capture_cleanup_failed'):
            with self.subTest(code=code):
                self.setUp()
                script = Mock()
                script.exports_sync.stop.return_value = dict(quiescent=True, cleanup_error=code)
                self.capture.script = script
                self.capture.script_load_attempted = True
                self.assertEqual(self.capture.close(), [code])
                script.unload.assert_called_once()
                self.assertTrue(self.capture.closed)

    def test_invalid_quiescence_result_cannot_authorize_unloading(self):
        for result in (True, None, {}, dict(quiescent=False, cleanup_error=None),
                       dict(quiescent=1, cleanup_error=None),
                       dict(quiescent=True, cleanup_error='unknown')):
            with self.subTest(result=result):
                self.setUp()
                self.capture.script = Mock()
                self.capture.script_load_attempted = True
                self.capture.script.exports_sync.stop.return_value = result
                self.assertEqual(self.capture.close(), ['script_stop_failed'])
                self.capture.script.unload.assert_not_called()
                self.assertTrue(self.capture.cleanup_pending)

    def test_created_but_never_loaded_script_can_cleanup_without_stop_rpc(self):
        script = Mock()
        self.capture.script = script
        self.assertEqual(self.capture.close(), [])
        script.exports_sync.stop.assert_not_called()
        script.unload.assert_called_once()
        self.assertTrue(self.capture.closed)

    def test_create_and_load_failures_remain_cleanable(self):
        for stage in ('create', 'load', 'cancel_before_load'):
            with self.subTest(stage=stage):
                self.setUp()
                script, session, target = Mock(), Mock(), Mock()
                target.binding_key.return_value = 'a' * 64
                script.exports_sync.stop.return_value = dict(quiescent=True, cleanup_error=None)
                session.create_script.return_value = script
                self.capture.frida.get_local_device = Mock(return_value=Mock(attach=Mock(return_value=session)))
                if stage == 'create':
                    session.create_script.side_effect = RuntimeError('private error')
                elif stage == 'load':
                    script.load.side_effect = RuntimeError('private error')
                else:
                    script.on.side_effect = lambda *args: self.capture.abort()
                with patch('capture.guard.select_for_device', return_value=target), \
                     patch('capture.guard.revalidate'), patch('capture.guard.close_target'):
                    with self.assertRaises(CaptureError): self.capture.start()
                    self.assertEqual(self.capture.close(), [])
                self.assertTrue(self.capture.closed)
                self.assertFalse(self.capture.cleanup_pending)
                session.detach.assert_called_once()
                if stage == 'load':
                    script.exports_sync.stop.assert_called_once()
                else:
                    script.exports_sync.stop.assert_not_called()

    def test_failed_source_evaluation_without_stop_export_can_be_unloaded(self):
        script = Mock()
        script.exports_sync.stop.side_effect = RuntimeError('no stop export')
        script.list_exports_sync.return_value = []
        self.capture.script = script
        self.capture.script_load_attempted = True
        self.assertEqual(self.capture.close(), ['capture_observer_not_installed'])
        script.unload.assert_called_once()
        self.assertTrue(self.capture.closed)

    def test_missing_stop_is_not_assumed_when_export_enumeration_is_unavailable(self):
        script = Mock()
        script.exports_sync.stop.side_effect = RuntimeError('rpc unavailable')
        script.list_exports_sync.side_effect = RuntimeError('rpc timeout')
        self.capture.script = script
        self.capture.script_load_attempted = True
        self.assertEqual(self.capture.close(), ['script_stop_failed'])
        script.unload.assert_not_called()
        self.assertTrue(self.capture.cleanup_pending)

    def test_exited_original_host_allows_pending_capture_cleanup(self):
        script, session, target = Mock(), Mock(), Mock()
        script.exports_sync.stop.side_effect = RuntimeError('session detached')
        self.capture.script, self.capture.session, self.capture.target = script, session, target
        self.capture.script_load_attempted = True
        with patch('capture.guard.target_exited', return_value=True) as exited, \
             patch('capture.guard.close_target') as close_target:
            self.assertEqual(self.capture.close(), ['capture_host_exited'])
        exited.assert_called_once_with(target)
        close_target.assert_called_once_with(target)
        script.list_exports_sync.assert_not_called()
        script.unload.assert_called_once()
        session.detach.assert_called_once()
        self.assertFalse(self.capture.cleanup_pending)

    def test_detached_session_with_alive_or_unknown_host_must_keep_capture(self):
        for alive in (False, RuntimeError('query failed')):
            with self.subTest(alive=alive):
                self.setUp()
                script = Mock()
                script.exports_sync.stop.side_effect = RuntimeError('session detached')
                script.list_exports_sync.side_effect = RuntimeError('session detached')
                self.capture.script, self.capture.target = script, Mock()
                self.capture.script_load_attempted = True
                with patch('capture.guard.target_exited', return_value=alive,
                           side_effect=alive if isinstance(alive, Exception) else None):
                    self.assertEqual(self.capture.close(), ['script_stop_failed'])
                script.unload.assert_not_called()
                self.assertTrue(self.capture.cleanup_pending)


class CaptureOwnerTests(unittest.TestCase):
    def test_host_exit_proof_uses_only_the_retained_handle(self):
        target = guard.Target(999, 100, 123, 'not-read', uuid.UUID(int=2), {}, (), False)
        api = Mock()
        with patch('guard._apis', return_value=api):
            api.WaitForSingleObject.return_value = 258
            self.assertFalse(guard.target_exited(target))
            api.WaitForSingleObject.return_value = 0
            self.assertTrue(guard.target_exited(target))
        self.assertEqual(api.WaitForSingleObject.call_args_list,
                         [((123, 0),), ((123, 0),)])
        api.OpenProcess.assert_not_called()
        api.CloseHandle.assert_not_called()

    def test_fixed_noninheritable_mutex_is_held_until_close(self):
        api = Mock()
        api.CreateMutexW.return_value = 100
        api.WaitForSingleObject.return_value = 0
        with patch('guard._apis', return_value=api):
            owner = guard.CaptureOwner()
            api.CreateMutexW.assert_called_once_with(None, False, r'Local\SayAll.RC003.ReportCapture')
            api.ReleaseMutex.assert_not_called()
            owner.close()
            owner.close()
        api.ReleaseMutex.assert_called_once_with(100)
        api.CloseHandle.assert_called_once_with(100)

    def test_live_owner_is_never_replaced_or_released(self):
        api = Mock()
        api.CreateMutexW.return_value = 100
        api.WaitForSingleObject.return_value = 258
        with patch('guard._apis', return_value=api):
            with self.assertRaisesRegex(guard.GuardError, 'capture_owner_busy'):
                guard.CaptureOwner()
        api.ReleaseMutex.assert_not_called()
        api.CloseHandle.assert_called_once_with(100)

    def test_abandoned_owner_does_not_silently_start_capture(self):
        api = Mock()
        api.CreateMutexW.return_value = 100
        api.WaitForSingleObject.return_value = 0x80
        with patch('guard._apis', return_value=api):
            with self.assertRaisesRegex(guard.GuardError, 'capture_owner_unavailable'):
                guard.CaptureOwner()
        api.ReleaseMutex.assert_called_once_with(100)
        api.CloseHandle.assert_called_once_with(100)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.now = [0.0]
        self.box = Mailbox(lambda: self.now[0])
        self.events, self.captures = [], []
        def factory(selection, tv_enabled):
            capture = FakeCapture(selection, tv_enabled)
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
        self.box.update(Lease(1, True, 'test', False))
        self.runtime.step()
        def close():
            self.box.stop('parent_stop')
            return ['script_unload_failed']
        self.captures[0].close = close
        self.runtime.cleanup('selection_changed')
        self.runtime.run()
        self.assertEqual(self.runtime.final_cleanup_mask, 2)

    def test_earlier_retry_cleanup_error_does_not_pollute_clean_exit(self):
        self.box.update(Lease(1, True, 'test', False))
        self.runtime.step()
        self.captures[0].close = Mock(return_value=['session_detach_failed'])
        self.runtime.cleanup('capture_failed')
        self.assertEqual(self.events[-1]['code'], 'session_detach_failed')
        self.runtime.step()
        self.box.stop('parent_stop')
        self.runtime.run()
        self.assertEqual(self.runtime.final_cleanup_mask, 0)

    def test_capture_diagnostics_keep_generation_without_arming(self):
        self.box.update(Lease(4, True, 'test', False))
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
        def factory(device, tv_enabled):
            capture = original_factory(device, tv_enabled)
            def start():
                capture.events = [('diagnostic', 'capture_attach_started'), ('mask', 0)]
                raise CaptureError('capture_attach_failed')
            capture.start = start
            return capture
        self.runtime.factory = factory
        self.box.update(Lease(5, True, 'test', False))
        self.runtime.step()
        self.assertEqual(self.events[-2],
                         dict(type='diagnostic', generation=5, code='capture_attach_started'))
        self.assertEqual(self.events[-1]['reason'], 'capture_attach_failed')
        self.assertFalse(any(event['type'] == 'state' for event in self.events))
        self.assertEqual(self.captures[0].closes, 1)
        self.assertIsNone(self.runtime.capture)

    def test_selection_change_cleans_before_rebind(self):
        self.box.update(Lease(1, True, 'first', False))
        self.runtime.step()
        first = self.captures[0]
        first.events = [('mask', 0), ('mask', 1)]
        self.runtime.step()
        self.box.update(Lease(2, True, 'second', False))
        self.runtime.step()
        self.assertEqual(first.closes, 1)
        self.assertEqual(self.captures[1].selection, 'second')
        self.assertFalse(self.runtime.state.armed)
        self.assertEqual(self.runtime.state.generation, 2)
        self.assertEqual(self.events[-1]['reason'], 'awaiting_neutral')

    def test_failed_revalidation_cleans_and_backs_off(self):
        self.box.update(Lease(1, True, 'first', False))
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
        self.box.update(Lease(1, False, '', False))
        self.runtime.step()
        self.assertEqual(self.captures, [])
        self.box.update(Lease(2, True, 'first', False))
        self.runtime.step()
        self.box.update(Lease(3, False, '', False))
        self.runtime.step()
        self.assertEqual(self.captures[0].closes, 1)
        self.assertEqual(self.events[-1]['reason'], 'disabled')

    def test_parent_eof_aborts_and_cleans_without_injection(self):
        self.box.update(Lease(1, True, 'first', False))
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
        self.runtime.publish_lease(Lease(1, True, 'first', False))
        self.runtime.step()
        self.runtime.publish_lease(Lease(1, True, 'first', False))
        self.assertEqual(self.captures[0].aborts, 0)

    def test_new_capture_is_not_cancelled_after_lease_publication(self):
        self.runtime.publish_lease(Lease(1, True, 'first', False))
        self.runtime.step()
        previous = self.captures[0]
        cancelling, finish_cancel = threading.Event(), threading.Event()
        original_abort = previous.abort
        def delayed_abort():
            original_abort()
            cancelling.set()
            self.assertTrue(finish_cancel.wait(1))
        previous.abort = delayed_abort
        publisher = threading.Thread(target=self.runtime.publish_lease, args=(Lease(2, True, 'second', False),))
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
        def delayed_factory(device, tv_enabled):
            capture = original_factory(device, tv_enabled)
            if device == 'first':
                factory_entered.set()
                self.assertTrue(finish_factory.wait(1))
            return capture
        self.runtime.factory = delayed_factory
        self.runtime.publish_lease(Lease(1, True, 'first', False))
        worker = threading.Thread(target=self.runtime.step)
        worker.start()
        try:
            self.assertTrue(factory_entered.wait(1))
            self.runtime.publish_lease(Lease(2, True, 'second', False))
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

    def test_tv_option_change_cancels_before_close_and_rebinds_generation(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        first = self.captures[0]
        self.assertTrue(first.tv_enabled)
        first.events = [('mask', 0), ('mask', 8)]
        self.runtime.step()
        boundary = len(self.events)
        first.abort = lambda: self.assertEqual(self.events[-1]['reason'], 'selection_changed')
        first.close = lambda: (self.assertEqual(self.events[-1]['reason'], 'selection_changed') or [])
        self.runtime.publish_lease(Lease(2, True, 'first', False))
        first.events = [('mask', 0)]
        self.runtime.step()
        self.assertFalse(any(event['type'] == 'state' for event in self.events[boundary:]))
        self.assertEqual(len(self.captures), 2)
        self.assertFalse(self.captures[1].tv_enabled)
        self.assertFalse(self.runtime.state.armed)
        self.assertEqual(self.runtime.state.generation, 2)

    def test_tv_fault_cancels_pending_press_before_close_without_release(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        capture = self.captures[0]
        capture.events = [('mask', 0), ('mask', 8)]
        self.runtime.step()
        boundary = len(self.events)
        def close():
            self.assertEqual(self.events[-1]['type'], 'status')
            self.assertEqual(self.events[-1]['reason'], 'tv_report_submit_failed')
            return []
        capture.close = close
        capture.events = [('fault', 'tv_report_submit_failed'), ('mask', 0)]
        self.runtime.step()
        self.assertFalse(any(event['type'] == 'state' for event in self.events[boundary:]))
        self.assertIsNone(self.runtime.capture)
        self.assertFalse(self.runtime.state.armed)

    def test_changed_lease_during_drain_never_forwards_old_tv_release(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        capture = self.captures[0]
        capture.events = [('mask', 0), ('mask', 8)]
        self.runtime.step()
        boundary = len(self.events)
        def drain():
            self.runtime.publish_lease(Lease(2, True, 'second', True))
            yield 'mask', 0
            yield 'mask', 8
        capture.drain = drain
        self.runtime.step()
        self.assertFalse(any(event['type'] == 'state' for event in self.events[boundary:]))
        self.assertEqual(capture.aborts, 1)

    def test_parent_stop_while_tv_is_held_emits_cancel_before_abort(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        capture = self.captures[0]
        capture.events = [('mask', 0), ('mask', 8)]
        self.runtime.step()
        boundary = len(self.events)
        capture.abort = lambda: self.assertEqual(self.events[-1]['reason'], 'parent_stop')
        self.runtime.publish_lease(None)
        self.assertFalse(any(event['type'] == 'state' for event in self.events[boundary:]))
        self.assertFalse(self.runtime.state.armed)

    def test_abort_rejects_already_queued_state_even_with_unchanged_lease(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        capture = self.captures[0]
        capture.events = [('mask', 0), ('mask', 8)]
        self.runtime.step()
        boundary = len(self.events)
        capture.events = [('mask', 0), ('mask', 8)]
        self.runtime.abort()
        self.runtime.step()
        self.assertFalse(any(event['type'] == 'state' for event in self.events[boundary:]))
        self.assertFalse(self.runtime.state.armed)

    def test_pending_cleanup_prevents_rebind_and_waits_between_attempts(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        capture = self.captures[0]
        calls = []
        def close():
            calls.append(self.now[0])
            capture.cleanup_pending = len(calls) == 1
            return ['script_stop_failed'] if capture.cleanup_pending else []
        capture.close = close
        self.runtime.publish_lease(Lease(2, True, 'second', True))
        self.runtime.step()
        self.assertIs(self.runtime.capture, capture)
        self.assertEqual(len(self.captures), 1)
        self.now[0] = 1.0
        self.runtime.step()
        self.assertEqual(calls, [0.0])
        self.now[0] = 2.0
        self.runtime.step()
        self.assertEqual(calls, [0.0, 2.0])
        self.assertEqual(len(self.captures), 2)
        self.assertEqual(self.runtime.state.generation, 2)

    def test_parent_exit_waits_for_cleanup_and_retains_failure_exit_bit(self):
        self.runtime.publish_lease(Lease(1, True, 'first', True))
        self.runtime.step()
        capture = self.captures[0]
        calls = []
        def close():
            calls.append(True)
            capture.cleanup_pending = len(calls) == 1
            return ['script_stop_failed'] if capture.cleanup_pending else []
        capture.close = close
        self.box.stop('parent_stop')
        with patch('helper.time.sleep') as sleep:
            self.runtime.run()
        sleep.assert_called_once_with(2.0)
        self.assertEqual(len(calls), 2)
        self.assertIsNone(self.runtime.capture)
        self.assertEqual(self.runtime.final_cleanup_mask, 1)


class MainExitTests(unittest.TestCase):
    def test_capture_owner_is_held_through_runtime_cleanup(self):
        owner = Mock()
        runtime = Mock(final_cleanup_mask=0)
        runtime.run.side_effect = lambda: owner.close.assert_not_called()
        sock = Mock()
        sock.getsockname.return_value = ('127.0.0.1', 12345)
        with patch('helper.guard.ParentGuard'), patch('helper.guard.CaptureOwner', return_value=owner), \
             patch('helper.guard.verify_listener'), patch('helper.guard.enable_debug_privilege'), \
             patch('helper.socket.create_connection', return_value=sock), \
             patch('helper.threading.Thread'), patch('helper.Runtime', return_value=runtime):
            self.assertEqual(main(['--port', '1234', '--token', 'a'*64, '--parent-pid', '1']), 0)
        owner.close.assert_called_once()

    def test_busy_capture_owner_fails_before_privilege_or_attachment(self):
        sock = Mock()
        sock.getsockname.return_value = ('127.0.0.1', 12345)
        with patch('helper.guard.ParentGuard'), \
             patch('helper.guard.CaptureOwner', side_effect=guard.GuardError('capture_owner_busy')), \
             patch('helper.guard.verify_listener'), patch('helper.guard.enable_debug_privilege') as privilege, \
             patch('helper.socket.create_connection', return_value=sock), \
             patch('helper.threading.Thread'), patch('helper.Runtime') as runtime:
            self.assertEqual(main(['--port', '1234', '--token', 'a'*64, '--parent-pid', '1']), 1)
        privilege.assert_not_called()
        runtime.assert_not_called()
        self.assertEqual(json.loads(sock.sendall.call_args_list[-1].args[0])['code'], 'capture_owner_busy')

    def test_hello_uses_protocol_two(self):
        runtime = Mock(final_cleanup_mask=0)
        sock = Mock()
        sock.getsockname.return_value = ('127.0.0.1', 12345)
        with patch('helper.guard.ParentGuard'), patch('helper.guard.CaptureOwner'), patch('helper.guard.verify_listener'), \
             patch('helper.guard.enable_debug_privilege'), \
             patch('helper.socket.create_connection', return_value=sock), \
             patch('helper.threading.Thread'), patch('helper.Runtime', return_value=runtime):
            self.assertEqual(main(['--port', '1234', '--token', 'a'*64, '--parent-pid', '1']), 0)
        hello = json.loads(sock.sendall.call_args_list[0].args[0])
        self.assertEqual(hello['type'], 'hello')
        self.assertEqual(hello['protocol'], 2)

    def test_main_reports_cleanup_mask_and_never_success_on_run_exception(self):
        for mask, failure, expected in ((0, None, 0), (1, None, 65), (31, None, 95),
                                         (0, RuntimeError('private error'), 1),
                                         (31, RuntimeError('private error'), 1)):
            with self.subTest(mask=mask, fatal=failure is not None):
                runtime = Mock(final_cleanup_mask=mask)
                runtime.run.side_effect = failure
                with patch('helper.guard.ParentGuard'), patch('helper.guard.CaptureOwner'), patch('helper.guard.verify_listener'), \
                     patch('helper.guard.enable_debug_privilege'), \
                     patch('helper.socket.create_connection'), patch('helper.threading.Thread'), \
                     patch('helper.Runtime', return_value=runtime):
                    result = main(['--port', '1234', '--token', 'a'*64, '--parent-pid', '1'])
                self.assertEqual(result, expected)


if __name__ == '__main__':
    unittest.main()
