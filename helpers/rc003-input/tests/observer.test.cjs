const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '..', 'observer.js'), 'utf8');
const number = n => ({toUInt32: () => n});
const bytes = (...usages) => [1, 0, 0, ...[0, 1, 2].flatMap(i => [
  (usages[i] || 0) & 255, (usages[i] || 0) >>> 8])];
function harness(tvEnabled = true, failures = {}, okEnabled = false, gestureMask = 0) {
  const h = {clock: 0, allowed: true, ready: true, detached: false, closed: false,
    reads: 0, writes: [], messages: [], callbacks: null, timer: null, postedStop: null};
  const context = {
    Process: {getModuleByName: () => ({getExportByName: name => name})},
    NativeFunction: function() {
      if (failures.nativeThrow) throw Error('native unavailable');
      return () => ({toNumber: () => h.clock});
    },
    Interceptor: {attach: (_address, callbacks) => {
      if (failures.attachThrow) throw Error('hook unavailable');
      h.callbacks = callbacks; return {detach() {h.detached = true;}};
    }},
    rc003Source: {initialize() {
      if (failures.initializeThrow) throw Error('source unavailable');
      return !failures.initializeFalse;
    }, matches: () => h.allowed,
      close() {if (h.failClose) throw Error('close failed'); h.closed = true;}, status: () => ({ready: h.ready})},
    send: m => h.messages.push(JSON.parse(JSON.stringify(m))),
    setInterval: callback => {h.timer = callback; return 1;}, clearInterval() {},
    recv: (type, callback) => {assert.equal(type, 'stop_capture'); h.postedStop = callback;},
    rpc: {exports: {}}
  };
  vm.createContext(context);
  try {
    vm.runInContext(source.replace('__SELECTED_SOURCE_KEY__', '"' + 'a'.repeat(64) + '"')
      .replace('__TV_CAPTURE_ENABLED__', JSON.stringify(tvEnabled))
      .replace('__OK_CAPTURE_ENABLED__', JSON.stringify(okEnabled))
      .replace('__GESTURE_CAPTURE_MASK__', JSON.stringify(gestureMask)), context);
  } catch (error) {
    if (Object.keys(failures).length === 0) throw error;
    h.loadError = error;
  }
  h.rpc = context.rpc.exports;
  h.pointer = function(memory, offset = 0) {
    return {isNull: () => false, add: n => h.pointer(memory, offset + n),
      readU8() {h.reads++; return memory.data[offset];},
      readByteArray(n) {
        h.reads++; memory.reads = (memory.reads || 0) + 1;
        if (memory.failReadAt === memory.reads) throw Error('read denied');
        return Uint8Array.from(memory.data.slice(offset, offset + n)).buffer;
      },
      writeByteArray(buffer) {
        const values = Array.from(new Uint8Array(buffer));
        memory.writes = (memory.writes || 0) + 1;
        h.writes.push({offset, values});
        if (memory.failWriteAt === memory.writes || memory.failAllWrites) throw Error('write denied');
        if (memory.ignoreWriteAt === memory.writes) return;
        memory.data.splice(offset, values.length, ...values);
        if (memory.afterWrite) memory.afterWrite(memory, memory.writes);
      }};
  };
  h.enter = function(data, options = {}) {
    const memory = Array.isArray(data) ? {data: data.slice()} : data;
    Object.assign(memory, options.memory || {});
    const input = {data: [0, 0, 0, 0, 2, 1, 0, 0]};
    if (options.input) input.data = options.input;
    const args = Array(10).fill(null);
    args[5] = number(options.ioctl ?? 0x80018483);
    args[6] = h.pointer(input); args[7] = number(options.inLength ?? 8);
    args[8] = h.pointer(memory); args[9] = number(options.outLength ?? 9);
    const call = {memory};
    h.callbacks.onEnter.call(call, args);
    return call;
  };
  h.leave = (call, status = 0) => h.callbacks.onLeave.call(call, number(status));
  h.report = (data, options = {}) => {
    const call = h.enter(data, options);
    const delivered = call.memory.data.slice();
    if (options.beforeLeave) options.beforeLeave(call.memory);
    h.leave(call, options.status ?? 0);
    return {call, delivered};
  };
  h.masks = () => h.messages.filter(m => m.type === 'mask').map(m => m.mask);
  h.faults = () => h.messages.filter(m => m.type === 'fault').map(m => m.reason);
  h.diagnostics = () => h.messages.filter(m => m.type === 'diagnostic').map(m => m.reason);
  return h;
}
let passed = 0;
async function test(name, run) {
  try {await run(); passed++;}
  catch (error) {error.message = name + ': ' + error.message; throw error;}
}
function assertStopped(result, cleanupError = null) {
  assert.equal(result.quiescent, true);
  assert.equal(result.cleanup_error, cleanupError);
}
async function main() {
  await test('load failures retain stop RPC before native/hook/source initialization', async () => {
    for (const failure of ['nativeThrow', 'attachThrow', 'initializeThrow', 'initializeFalse']) {
      const h = harness(true, {[failure]: true});
      assert(h.loadError, failure);
      assert.equal(typeof h.rpc.stop, 'function');
      assertStopped(await h.rpc.stop());
      assert(h.closed); assert.deepEqual(h.masks(), []); assert.equal(h.writes.length, 0);
      if (failure.startsWith('initialize')) assert(h.detached);
    }
  });
  await test('legacy three keys and full 16-bit usages remain read-only', () => {
    const h = harness(false);
    h.report(bytes(0xf1, 0x80, 0x81));
    h.report(bytes(0x3e, 0x52));
    h.report(bytes(0x1f1));
    h.report(bytes(0xf1), {status: 0x103});
    assert.deepEqual(h.masks(), [7, 0, 0]);
    assert.equal(h.writes.length, 0);
    assert.deepEqual(h.faults(), []);
  });
  await test('unconfigured TV is unchanged and never exported', () => {
    const h = harness(false);
    h.report(bytes()); h.report(bytes(0x35)); h.report(bytes(0x35)); h.report(bytes());
    assert.deepEqual(h.masks(), [0, 0, 0, 0]);
    assert.equal(h.writes.length, 0);
    assert.deepEqual(h.diagnostics(), []);
  });
  await test('foreign keyboard is not read or written, even during owned TV', () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35));
    const before = {reads: h.reads, writes: h.writes.length, messages: h.messages.length};
    h.allowed = false;
    assert.deepEqual(h.report(bytes(0x35)).delivered, bytes(0x35));
    assert.deepEqual({reads: h.reads, writes: h.writes.length, messages: h.messages.length}, before);
  });
  await test('preheld TV remains nonneutral until its real successful release', () => {
    const h = harness();
    h.report(bytes(0x35)); h.report(bytes(0x35));
    assert.deepEqual(h.masks(), [8, 8]);
    assert.equal(h.writes.length, 0);
    h.report(bytes(), {status: 0x103}); h.report(bytes(0x35));
    assert.equal(h.writes.length, 0);
    assert(!h.diagnostics().includes('tv_capture_ready'));
    h.report(bytes());
    const down = h.report(bytes(0x35));
    assert.deepEqual(down.delivered, bytes());
    assert.deepEqual(down.call.memory.data, bytes(0x35));
    h.report(bytes());
    assert.deepEqual(h.masks(), [8, 8, 8, 0, 8, 0]);
    assert.equal(h.diagnostics().filter(x => x === 'tv_preheld_passthrough').length, 1);
  });
  await test('preheld legacy key cannot arm TV or forge neutral', () => {
    const h = harness(); h.report(bytes(0xf1)); h.report(bytes(0x35));
    assert.deepEqual(h.masks(), [1, 8]); assert.equal(h.writes.length, 0);
    h.report(bytes());
    assert.deepEqual(h.report(bytes(0x35)).delivered, bytes());
  });
  await test('clear only TV slots, retain other keys, restore exact original', () => {
    const h = harness(); h.report(bytes());
    const original = bytes(0xf1, 0x35, 0x81);
    const down = h.report(original);
    assert.deepEqual(down.delivered, bytes(0xf1, 0, 0x81));
    assert.deepEqual(down.call.memory.data, original);
    assert.deepEqual(h.masks(), [0, 13]);
    assert.deepEqual(h.writes, [{offset: 5, values: [0, 0]}, {offset: 5, values: [0x35, 0]}]);
  });
  await test('duplicate TV slots are all removed and restored', () => {
    const h = harness(); h.report(bytes());
    const down = h.report(bytes(0x35, 0x35, 0x35));
    assert.deepEqual(down.delivered, bytes());
    assert.deepEqual(down.call.memory.data, bytes(0x35, 0x35, 0x35));
    assert.deepEqual(h.masks(), [0, 8]);
  });
  await test('repeated owned reports retain down without diagnostic flooding', () => {
    const h = harness(); h.report(bytes());
    for (let i = 0; i < 5; i++) assert.deepEqual(h.report(bytes(0x35)).delivered, bytes());
    h.report(bytes());
    assert.deepEqual(h.masks(), [0, 8, 8, 8, 8, 8, 0]);
    assert.equal(h.diagnostics().filter(x => x === 'tv_report_cleared').length, 1);
  });
  await test('non-TV reports retain every byte and never write', () => {
    const h = harness(); h.report(bytes());
    for (const report of [bytes(0x135), bytes(0x3e, 0x52), bytes(0xf1, 0x80, 0x81)]) {
      assert.deepEqual(h.report(report).delivered, report);
    }
    assert.equal(h.writes.length, 0);
  });
  await test('invalid shape never arms capture', () => {
    const h = harness();
    h.report([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    h.report([1, 1, 0, 0, 0, 0, 0, 0, 0]);
    h.report(bytes(), {ioctl: 0}); h.report(bytes(), {inLength: 7});
    h.report(bytes(), {outLength: 10}); h.report(bytes(), {input: [0,0,0,0,1,1,0,0]});
    h.report(bytes(0x35));
    assert.deepEqual(h.masks(), [8]); assert.equal(h.writes.length, 0);
  });
  await test('clear failure cancels without a down or normal release', () => {
    const h = harness(); h.report(bytes());
    const down = h.report(bytes(0x35), {memory: {failWriteAt: 1}});
    assert.deepEqual(down.call.memory.data, bytes(0x35));
    assert.deepEqual(h.masks(), [0]);
    assert.deepEqual(h.faults(), ['tv_report_write_failed']); assert(h.detached);
  });
  await test('partial multi-slot write failure restores only owned slots', () => {
    const h = harness(); h.report(bytes());
    const down = h.report(bytes(0x35, 0x35, 0x80), {memory: {failWriteAt: 2}});
    assert.deepEqual(down.call.memory.data, bytes(0x35, 0x35, 0x80));
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['tv_report_write_failed']);
    assert(h.writes.every(write => write.offset === 3 || write.offset === 5));
  });
  await test('silent write no-op fails readback and never maps', () => {
    const h = harness(); h.report(bytes());
    h.report(bytes(0x35), {memory: {ignoreWriteAt: 1}});
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['tv_report_verify_failed']);
  });
  await test('unrelated buffer mutation is not overwritten during failed readback', async () => {
    const h = harness(); h.report(bytes());
    const down = h.report(bytes(0x35), {memory: {afterWrite: (memory, count) => {
      if (count === 1) memory.data[5] = 0x52;
    }}});
    assert.equal(down.call.memory.data[5], 0x52);
    assert.equal(h.writes.length, 1);
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['tv_report_restore_failed']);
    assertStopped(await h.rpc.stop(), 'tv_report_restore_failed');
  });
  await test('read failure before any modification cancels', () => {
    const h = harness(); h.report(bytes());
    h.report(bytes(0x35), {memory: {failReadAt: 1}});
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['report_read_failed']);
    assert.equal(h.writes.length, 0);
  });
  await test('readback failure restores before native submission but never maps', () => {
    const h = harness(); h.report(bytes());
    const down = h.report(bytes(0x35), {memory: {failReadAt: 2}});
    assert.deepEqual(down.delivered, bytes(0x35));
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['report_read_failed']);
  });
  await test('NTSTATUS failure and pending never prove TV submission', () => {
    for (const status of [0x103, 0xc0000001]) {
      const h = harness(); h.report(bytes());
      const down = h.report(bytes(0x35), {status});
      assert.deepEqual(down.call.memory.data, bytes(0x35));
      assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['tv_report_submit_failed']);
    }
  });
  await test('failed owned release is cancellation, not a click', () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35));
    h.report(bytes(), {status: 0xc0000001});
    assert.deepEqual(h.masks(), [0, 8]); assert.deepEqual(h.faults(), ['tv_report_submit_failed']);
  });
  await test('kernel-changed buffer is retained, never replaced with stale snapshot', () => {
    const h = harness(); h.report(bytes());
    const down = h.report(bytes(0x35), {beforeLeave: memory => {memory.data[5] = 0x52;}});
    assert.deepEqual(down.call.memory.data, bytes(0, 0x52));
    assert.equal(h.writes.length, 1); assert(h.diagnostics().includes('tv_buffer_changed'));
    assert.deepEqual(h.masks(), [0, 8]);
  });
  await test('restore failure never reports successful cleanup', async () => {
    const h = harness(); h.report(bytes());
    h.report(bytes(0x35), {memory: {failWriteAt: 2}});
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['tv_report_restore_failed']);
    assertStopped(await h.rpc.stop(), 'tv_report_restore_failed');
  });
  await test('stop waits for restoration and does not publish a release', async () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35));
    const call = h.enter(bytes(0x35));
    let settled = false;
    const pending = h.rpc.stop().then(value => {settled = true; return value;});
    await Promise.resolve(); assert.equal(settled, false); assert.equal(h.detached, false);
    const foreignAfterStop = h.report(bytes(0x35));
    assert.deepEqual(foreignAfterStop.delivered, bytes(0x35));
    h.leave(call);
    assertStopped(await pending); assert(h.detached && h.closed);
    assert.deepEqual(call.memory.data, bytes(0x35)); assert.deepEqual(h.masks(), [0, 8]);
  });
  await test('posted cancellation revokes immediately and drains in-flight restoration', async () => {
    const h = harness(); h.report(bytes()); const call = h.enter(bytes(0x35));
    h.postedStop(); assert.equal(h.detached, false);
    const pending = h.rpc.stop(); h.leave(call);
    assertStopped(await pending); assert.deepEqual(h.masks(), [0]);
    assert.deepEqual(call.memory.data, bytes(0x35));
  });
  await test('stop RPC retries share one pending promise until quiescent', async () => {
    const h = harness(); h.report(bytes()); const call = h.enter(bytes(0x35));
    const first = h.rpc.stop(); const second = h.rpc.stop();
    assert.equal(first, second);
    let settled = false; first.then(() => {settled = true;});
    await Promise.resolve(); await Promise.resolve();
    assert.equal(settled, false); assert.equal(h.detached, false);
    h.leave(call); assertStopped(await first); assert.equal(h.rpc.stop(), first);
  });
  await test('quiescent hook cleanup error is explicit and permits caller disposition', async () => {
    const h = harness(); h.failClose = true;
    assertStopped(await h.rpc.stop(), 'capture_cleanup_failed');
    assert(h.detached); assert.equal(h.closed, false);
  });
  await test('lease expiry during a call restores and cancels without normal release', async () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35));
    const call = h.enter(bytes(0x35)); h.clock = 10001; h.timer();
    assert.equal(h.detached, false); h.leave(call);
    assert.deepEqual(call.memory.data, bytes(0x35)); assert.deepEqual(h.masks(), [0, 8]);
    assert.equal(h.rpc.renew(), false); assertStopped(await h.rpc.stop());
  });
  await test('lost verified source cancels an owned press', () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35)); h.ready = false;
    assert.equal(h.rpc.renew(), false);
    assert.deepEqual(h.faults(), ['tv_source_lost']); assert.deepEqual(h.masks(), [0, 8]);
  });
  await test('nested same buffer cannot fabricate neutral or restore outer buffer', async () => {
    const h = harness(); h.report(bytes());
    const memory = {data: bytes(0x35)};
    const outer = h.enter(memory); const beforeReads = h.reads;
    const nested = h.enter(memory);
    // Only the operation/selector guard was read; no nested output read.
    assert.equal(h.reads - beforeReads, 2);
    assert.deepEqual(h.faults(), ['tv_report_overlap']);
    h.leave(nested); assert.deepEqual(memory.data, bytes());
    assert.equal(h.writes.length, 1); assert.equal(h.detached, false);
    h.leave(outer); assert.deepEqual(memory.data, bytes(0x35));
    assert.equal(h.writes.length, 2); assert.deepEqual(h.masks(), [0]);
    assertStopped(await h.rpc.stop());
  });
  await test('overlapping distinct buffers also cancel rather than reorder edges', () => {
    const h = harness(); h.report(bytes());
    const outer = h.enter(bytes(0x35)); const concurrent = h.enter(bytes());
    h.leave(concurrent); h.leave(outer);
    assert.deepEqual(h.faults(), ['tv_report_overlap']); assert.deepEqual(h.masks(), [0]);
  });
  await test('foreign source during in-flight TV does not trigger overlap fault', () => {
    const h = harness(); h.report(bytes()); const outer = h.enter(bytes(0x35));
    h.allowed = false; h.report(bytes(0x35)); h.allowed = true; h.leave(outer);
    assert.deepEqual(h.faults(), []); assert.deepEqual(h.masks(), [0, 8]);
  });
  await test('invalid report while owning TV cancels pending gesture', () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35));
    h.report([1, 1, 0, 0, 0, 0, 0, 0, 0]);
    assert.deepEqual(h.faults(), ['tv_report_invalid']); assert.deepEqual(h.masks(), [0, 8]);
  });
  await test('other device report ID is neither TV release nor a capture failure', () => {
    const h = harness(); h.report(bytes()); h.report(bytes(0x35));
    const writes = h.writes.length;
    const other = [2, 0, 0, 0x35, 0, 0, 0, 0, 0];
    assert.deepEqual(h.report(other).delivered, other);
    assert.deepEqual(h.masks(), [0, 8]); assert.deepEqual(h.faults(), []);
    assert.equal(h.writes.length, writes);
    h.report(bytes()); assert.deepEqual(h.masks(), [0, 8, 0]);
  });
  await test('TV and OK permission matrix clears only enabled slots and leaves voice untouched', () => {
    for (const tv of [false, true]) for (const ok of [false, true]) {
      const h = harness(tv, {}, ok); h.report(bytes());
      const original = bytes(0x35, 0x28, 0x3e);
      const down = h.report(original);
      assert.deepEqual(down.delivered, bytes(tv ? 0 : 0x35, ok ? 0 : 0x28, 0x3e));
      assert.deepEqual(down.call.memory.data, original);
      assert.deepEqual(h.masks(), [0, (tv ? 8 : 0) | (ok ? 16 : 0)]);
      h.report(bytes());
      assert.deepEqual(h.faults(), []);
    }
  });
  await test('preheld OK passes through until a real successful neutral', () => {
    const h = harness(false, {}, true);
    for (let i = 0; i < 3; i++)
      assert.deepEqual(h.report(bytes(0x28)).delivered, bytes(0x28));
    h.report(bytes(), {status: 0x103});
    assert.deepEqual(h.report(bytes(0x28)).delivered, bytes(0x28));
    assert.equal(h.writes.length, 0);
    assert.equal(h.diagnostics().filter(x => x === 'ok_preheld_passthrough').length, 1);
    h.report(bytes());
    const down = h.enter(bytes(0x28));
    assert.deepEqual(down.memory.data, bytes());
    assert.deepEqual(h.masks(), [16, 16, 16, 16, 0]); // No DOWN until success.
    h.leave(down);
    assert.deepEqual(h.masks(), [16, 16, 16, 16, 0, 16]);
    h.report(bytes()); assert.equal(h.masks().at(-1), 0);
  });
  await test('either configured preheld key prevents shared capture readiness', () => {
    for (const preheld of [0x28, 0x35]) {
      const h = harness(true, {}, true);
      h.report(bytes(preheld));
      const both = h.report(bytes(0x28, 0x35));
      assert.deepEqual(both.delivered, bytes(0x28, 0x35));
      h.report(bytes(preheld)); assert.equal(h.writes.length, 0);
      h.report(bytes());
      assert.deepEqual(h.report(bytes(0x28, 0x35)).delivered, bytes());
      assert.equal(h.masks().at(-1), 24);
      assert(h.diagnostics().includes('ok_capture_ready'));
      assert(h.diagnostics().includes('tv_capture_ready'));
    }
  });
  await test('foreign keyboard during owned OK is never read or cleared', () => {
    const h = harness(false, {}, true); h.report(bytes()); h.report(bytes(0x28));
    h.allowed = false; const reads = h.reads, writes = h.writes.length;
    assert.deepEqual(h.report(bytes(0x28)).delivered, bytes(0x28));
    assert.equal(h.reads, reads); assert.equal(h.writes.length, writes);
    assert.deepEqual(h.masks(), [0, 16]); assert.deepEqual(h.faults(), []);
  });
  await test('duplicate OK and mixed TV slots are restored exactly without extra edges', () => {
    for (const original of [bytes(0x28, 0x28, 0x28), bytes(0x28, 0x35, 0x28)]) {
      const h = harness(true, {}, true); h.report(bytes());
      const expectedMask = original[5] === 0x35 ? 24 : 16;
      for (let i = 0; i < 3; i++) {
        const down = h.report(original);
        assert.deepEqual(down.delivered, bytes()); assert.deepEqual(down.call.memory.data, original);
      }
      assert.deepEqual(h.masks(), [0, expectedMask, expectedMask, expectedMask]);
      assert.equal(h.diagnostics().filter(x => x === 'ok_report_cleared').length, 1);
    }
  });
  await test('mixed partial write failure restores both configured usages without publishing either', () => {
    const h = harness(true, {}, true); h.report(bytes());
    const original = bytes(0x35, 0x28, 0x52);
    const down = h.report(original, {memory: {failWriteAt: 2}});
    assert.deepEqual(down.call.memory.data, original);
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['ok_report_write_failed']);
    assert(h.writes.every(write => write.offset === 3 || write.offset === 5));
  });
  await test('OK-only write, readback and restore failures cancel without semantic DOWN', async () => {
    for (const [memory, reason] of [
      [{failWriteAt: 1}, 'ok_report_write_failed'],
      [{ignoreWriteAt: 1}, 'ok_report_verify_failed'],
      [{failReadAt: 2}, 'report_read_failed'],
      [{failWriteAt: 2}, 'ok_report_restore_failed'],
    ]) {
      const h = harness(false, {}, true); h.report(bytes());
      h.report(bytes(0x28), {memory});
      assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), [reason]);
      assertStopped(await h.rpc.stop(), reason === 'ok_report_restore_failed' ? reason : null);
    }
  });
  await test('OK cannot overwrite unrelated memory during failed clear verification', async () => {
    const h = harness(false, {}, true); h.report(bytes());
    const down = h.report(bytes(0x28), {memory: {afterWrite(memory, count) {
      if (count === 1) memory.data[5] = 0x52;
    }}});
    assert.equal(down.call.memory.data[5], 0x52);
    assert.equal(h.writes.length, 1); assert.deepEqual(h.masks(), [0]);
    assertStopped(await h.rpc.stop(), 'ok_report_restore_failed');
  });
  await test('unsuccessful OK DOWN and UP never become mapped edges or a synthetic click', () => {
    for (const status of [0x103, 0xc0000001]) for (const release of [false, true]) {
      const h = harness(false, {}, true); h.report(bytes());
      if (release) h.report(bytes(0x28));
      h.report(release ? bytes() : bytes(0x28), {status});
      assert.deepEqual(h.masks(), release ? [0, 16] : [0]);
      assert.deepEqual(h.faults(), ['ok_report_submit_failed']);
    }
  });
  await test('OK stop, posted stop and lease expiry drain restoration without a normal release', async () => {
    for (const cancel of ['rpc', 'post', 'expiry']) {
      const h = harness(false, {}, true); h.report(bytes()); h.report(bytes(0x28));
      const call = h.enter(bytes(0x28));
      if (cancel === 'post') h.postedStop();
      if (cancel === 'expiry') { h.clock = 10001; h.timer(); }
      let settled = false;
      const pending = h.rpc.stop().then(result => {settled = true; return result;});
      await Promise.resolve(); assert.equal(settled, false); assert.equal(h.detached, false);
      h.leave(call);
      assertStopped(await pending); assert.deepEqual(call.memory.data, bytes(0x28));
      assert.deepEqual(h.masks(), [0, 16]);
    }
  });
  await test('OK-only overlap is rejected before artificial neutral can be read', async () => {
    const h = harness(false, {}, true); h.report(bytes());
    const memory = {data: bytes(0x28)};
    const outer = h.enter(memory), reads = h.reads;
    const inner = h.enter(memory); assert.equal(h.reads - reads, 2);
    h.leave(inner); assert.equal(h.detached, false);
    h.leave(outer); assert.deepEqual(memory.data, bytes(0x28));
    assert.deepEqual(h.faults(), ['ok_report_overlap']); assert.deepEqual(h.masks(), [0]);
    assertStopped(await h.rpc.stop());
  });
  await test('lost source cancels OK-only and mixed owned reports', () => {
    for (const tv of [false, true]) {
      const h = harness(tv, {}, true); h.report(bytes()); h.report(bytes(0x28, 0x35));
      h.ready = false; assert.equal(h.rpc.renew(), false);
      assert.deepEqual(h.faults(), ['ok_source_lost']);
      assert.equal(h.masks().at(-1), tv ? 24 : 16);
    }
  });
  await test('other report IDs preserve OK hold while invalid same-ID frames cancel', () => {
    const h = harness(false, {}, true); h.report(bytes()); h.report(bytes(0x28));
    const other = [2, 0, 0, 0x28, 0, 0, 0, 0, 0];
    assert.deepEqual(h.report(other).delivered, other);
    assert.deepEqual(h.masks(), [0, 16]);
    h.report([1, 1, 0, 0, 0, 0, 0, 0, 0]);
    assert.deepEqual(h.faults(), ['ok_report_invalid']); assert.deepEqual(h.masks(), [0, 16]);
  });
  const ordinary = [[0xf1, 1], [0x80, 2], [0x81, 4], [0x52, 32], [0x51, 64],
    [0x50, 128], [0x4f, 256], [0x4a, 512], [0x65, 1024], [0x66, 2048], [0x7f, 4096]];
  await test('each of all 13 key permissions clears only exact configured usage, including idle first use', () => {
    for (const [usage, bit] of [...ordinary, [0x35, 8], [0x28, 16]]) {
      const h = harness(bit === 8, {}, bit === 16, bit & 0x1fe7);
      h.report(bytes());
      // Established attribution must survive idle; renew the bounded source lease.
      for (let i = 0; i < 12; i++) {h.clock += 4000; assert.equal(h.rpc.renew(), true);}
      const original = bytes(usage, 0x3e, 0x152); // Voice and 16-bit alias remain native.
      const down = h.report(original);
      assert.deepEqual(down.delivered, bytes(0, 0x3e, 0x152));
      assert.deepEqual(down.call.memory.data, original);
      assert.deepEqual(h.masks(), [0, bit]);
      h.report(bytes());
      assert.deepEqual(h.masks(), [0, bit, 0]);
      assert.deepEqual(h.faults(), []);
    }
  });
  await test('unconfigured ordinary usages remain native and only original three remain observable', () => {
    const h = harness(false); h.report(bytes());
    for (const [usage, bit] of ordinary) {
      const original = bytes(usage, 0x3e, usage + 0x100);
      assert.deepEqual(h.report(original).delivered, original);
      assert.equal(h.masks().at(-1), bit & 7);
    }
    assert.equal(h.writes.length, 0);
  });
  await test('ordinary capture preserves foreign keyboards and unrelated report fields byte for byte', () => {
    for (const [usage, bit] of ordinary) {
      const h = harness(false, {}, false, bit); h.report(bytes());
      h.report(bytes(usage)); h.allowed = false;
      const reads = h.reads, writes = h.writes.length, masks = h.masks();
      const original = bytes(usage, 0x3e, 0x152);
      assert.deepEqual(h.report(original).delivered, original);
      assert.equal(h.reads, reads); assert.equal(h.writes.length, writes);
      assert.deepEqual(h.masks(), masks); assert.deepEqual(h.faults(), []);
    }
  });
  await test('every ordinary preheld key waits for real neutral and clears repeated reports once armed', () => {
    for (const [usage, bit] of ordinary) {
      const h = harness(false, {}, false, bit);
      assert.deepEqual(h.report(bytes(usage)).delivered, bytes(usage));
      h.report(bytes(), {status: 0x103});
      assert.deepEqual(h.report(bytes(usage)).delivered, bytes(usage));
      assert.equal(h.writes.length, 0);
      h.report(bytes());
      for (let i = 0; i < 3; i++) {
        const original = bytes(usage, usage, usage);
        const down = h.report(original);
        assert.deepEqual(down.delivered, bytes());
        assert.deepEqual(down.call.memory.data, original);
      }
      h.report(bytes());
      assert.deepEqual(h.masks(), [bit, bit, 0, bit, bit, bit, 0]);
      assert.equal(h.diagnostics().filter(x => x === 'gesture_preheld_passthrough').length, 1);
      assert.equal(h.diagnostics().filter(x => x === 'gesture_report_cleared').length, 1);
    }
  });
  await test('mixed TV OK and ordinary slots form one transaction, without partial semantic success', () => {
    const h = harness(true, {}, true, 32); h.report(bytes());
    const original = bytes(0x35, 0x28, 0x52);
    const down = h.report(original, {memory: {failWriteAt: 3}});
    assert.deepEqual(down.call.memory.data, original);
    assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), ['gesture_report_write_failed']);
  });
  await test('ordinary write verification and restoration failures never emit mapped DOWN', async () => {
    for (const [usage, bit] of ordinary) for (const [memory, reason] of [
      [{failWriteAt: 1}, 'gesture_report_write_failed'],
      [{ignoreWriteAt: 1}, 'gesture_report_verify_failed'],
      [{failReadAt: 2}, 'report_read_failed'],
      [{failWriteAt: 2}, 'gesture_report_restore_failed'],
    ]) {
      const h = harness(false, {}, false, bit); h.report(bytes());
      h.report(bytes(usage), {memory});
      assert.deepEqual(h.masks(), [0]); assert.deepEqual(h.faults(), [reason]);
      assertStopped(await h.rpc.stop(), reason === 'gesture_report_restore_failed' ? reason : null);
    }
  });
  await test('ordinary buffer mutation cannot be overwritten during failed clear verification', async () => {
    const h = harness(false, {}, false, 32); h.report(bytes());
    const down = h.report(bytes(0x52), {memory: {afterWrite(memory, count) {
      if (count === 1) memory.data[5] = 0x28;
    }}});
    assert.equal(down.call.memory.data[5], 0x28); assert.equal(h.writes.length, 1);
    assert.deepEqual(h.masks(), [0]); assertStopped(await h.rpc.stop(), 'gesture_report_restore_failed');
  });
  await test('every ordinary stop and lease expiry drains in-flight restoration without synthetic release', async () => {
    for (const [usage, bit] of ordinary) for (const cancel of ['rpc', 'post', 'expiry']) {
      const h = harness(false, {}, false, bit); h.report(bytes()); h.report(bytes(usage));
      const call = h.enter(bytes(usage));
      if (cancel === 'post') h.postedStop();
      if (cancel === 'expiry') {h.clock = 10001; h.timer();}
      let settled = false;
      const pending = h.rpc.stop().then(result => {settled = true; return result;});
      await Promise.resolve(); assert.equal(settled, false); assert.equal(h.detached, false);
      h.leave(call);
      assertStopped(await pending); assert.deepEqual(call.memory.data, bytes(usage));
      assert.deepEqual(h.masks(), [0, bit]);
    }
  });
  await test('ordinary overlap cannot manufacture neutral and source loss cancels held gestures', () => {
    for (const [usage, bit] of ordinary) {
      const h = harness(false, {}, false, bit); h.report(bytes());
      const memory = {data: bytes(usage)}, outer = h.enter(memory), inner = h.enter(memory);
      h.leave(inner); h.leave(outer);
      assert.deepEqual(memory.data, bytes(usage));
      assert.deepEqual(h.faults(), ['gesture_report_overlap']); assert.deepEqual(h.masks(), [0]);
      const lost = harness(false, {}, false, bit); lost.report(bytes()); lost.report(bytes(usage));
      lost.ready = false; assert.equal(lost.rpc.renew(), false);
      assert.deepEqual(lost.faults(), ['gesture_source_lost']); assert.deepEqual(lost.masks(), [0, bit]);
    }
  });
  console.log(`observer checks passed: ${passed}`);
}
main().catch(error => {console.error(error); process.exitCode = 1;});
