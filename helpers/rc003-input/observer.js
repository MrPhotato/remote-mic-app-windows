'use strict';
// GPL-3.0-only. Report entry observation adapted from ZSTDJan commit
// 1e6b1d285f9cd50f30c5bc92ac7787a693fc993d. TV slot clearing follows the
// report-copy boundary reviewed in GetSayAll commit 6a4aaf88ef740534a29152ea0ef021cffee82256.
// Bits: Back=1, Volume Up=2, Volume Down=4, captured TV=8. No key injection.
const tvEnabled = __TV_CAPTURE_ENABLED__ === true;
let stopped = false;
let listener = null;
let timer = null;
let inFlight = 0;
let cleanupComplete = false;
let cleanupError = null;
let restorationFailed = false;
let stopPromise = null;
let resolveStop = null;
let tvReady = false;
let tvOwned = false;
let preheldNoted = false;
let tick = null;
function now() { return tick === null ? 0 : tick().toNumber(); }
let deadline = 0;
function active() { return !stopped && now() < deadline; }
function diagnostic(reason) { send({type: 'diagnostic', reason}); }
function sameBytes(a, b) { return a.every((value, index) => value === b[index]); }
function snapshot(pointer) {
  const bytes = new Uint8Array(pointer.readByteArray(9));
  if (bytes[0] !== 1 || bytes[1] !== 0 || bytes[2] !== 0) return null;
  let mask = 0;
  const tvSlots = [];
  for (let i = 3; i < 9; i += 2) {
    const usage = bytes[i] | (bytes[i + 1] << 8);
    if (usage === 0xf1) mask |= 1;
    else if (usage === 0x80) mask |= 2;
    else if (usage === 0x81) mask |= 4;
    else if (tvEnabled && usage === 0x35) { mask |= 8; tvSlots.push(i); }
  }
  return {pointer, bytes, mask, tvSlots, patched: null, needsRestore: false};
}
function finishStop() {
  if (!stopped || inFlight !== 0 || cleanupComplete) return;
  try {
    if (listener !== null) listener.detach();
    rc003Source.close();
    if (restorationFailed) cleanupError = 'tv_report_restore_failed';
  } catch (_) { cleanupError = 'capture_cleanup_failed'; }
  cleanupComplete = true;
  if (resolveStop !== null) resolveStop({quiescent: true, cleanup_error: cleanupError});
}
function stop(reason) {
  if (!stopped) {
    stopped = true;
    tvReady = false;
    tvOwned = false;
    if (timer !== null) clearInterval(timer);
    // A fault/stop cancels a pending gesture. A synthetic mask=0 would click it.
    send({type: 'stopped', reason});
  }
  finishStop();
}
function fault(reason) {
  if (reason === 'tv_report_restore_failed') restorationFailed = true;
  if (stopped) return;
  try { send({type: 'fault', reason}); }
  finally { stop(reason); }
}
function stopAndWait() {
  if (stopPromise === null) stopPromise = new Promise(resolve => {resolveStop = resolve;});
  stop('client_cleanup');
  if (cleanupComplete) resolveStop({quiescent: true, cleanup_error: cleanupError});
  return stopPromise;
}
// Restore only bytes owned by this invocation. Never change page protection,
// overwrite unrelated slots, or overwrite a buffer changed by the kernel.
function restore(frame, partialWrite = false) {
  if (!frame.needsRestore) return true;
  try {
    const current = new Uint8Array(frame.pointer.readByteArray(9));
    if (sameBytes(current, frame.bytes)) { frame.needsRestore = false; return true; }
    let matches = sameBytes(current, frame.patched);
    if (partialWrite) {
      matches = current.every((value, index) => {
        const slot = frame.tvSlots.find(offset => index === offset || index === offset + 1);
        return slot === undefined ? value === frame.bytes[index]
          : ((current[slot] === 0 && current[slot + 1] === 0)
            || (current[slot] === frame.bytes[slot] && current[slot + 1] === frame.bytes[slot + 1]));
      });
    }
    if (!matches) {
      frame.needsRestore = false;
      if (!partialWrite) diagnostic('tv_buffer_changed');
      return !partialWrite;
    }
    for (const offset of frame.tvSlots) {
      frame.pointer.add(offset).writeByteArray(
        new Uint8Array([frame.bytes[offset], frame.bytes[offset + 1]]).buffer);
    }
    if (!sameBytes(new Uint8Array(frame.pointer.readByteArray(9)), frame.bytes)) return false;
    frame.needsRestore = false;
    return true;
  } catch (_) { return false; }
}
// Keep cleanup reachable even if hook installation or source initialization
// fails during script.load(). No installed observer may lack a stop RPC.
recv('stop_capture', () => stop('client_cleanup'));
rpc.exports = {
  renew() {
    if (!active()) { if (!stopped) stop('lease_expired'); return false; }
    if (!rc003Source.status().ready) {
      if (tvEnabled) fault('tv_source_lost');
      return false;
    }
    deadline = now() + 10000;
    return true;
  },
  stop: stopAndWait
};
try {
  tick = new NativeFunction(Process.getModuleByName('kernel32.dll')
    .getExportByName('GetTickCount64'), 'uint64', []);
  deadline = now() + 10000;
} catch (_) {
  stop('source_initialization_failed');
  throw new Error('source_initialization_failed');
}
listener = Interceptor.attach(Process.getModuleByName('ntdll.dll')
  .getExportByName('NtDeviceIoControlFile'), {
  onEnter(args) {
    this.frame = null;
    this.counted = false;
    if (!active()) { if (!stopped) stop('lease_expired'); return; }
    if (args[5].toUInt32() !== 0x80018483) return;
    try {
      if (args[7].toUInt32() !== 8 || args[9].toUInt32() !== 9 ||
          args[6].isNull() || args[8].isNull() || !rc003Source.matches(args)) return;
      if (args[6].add(4).readU8() !== 2 || args[6].add(5).readU8() !== 1) return;
      // Check before reading: a nested call can see the first call's cleared
      // buffer. It must never turn that artificial zero into neutral or UP.
      if (tvEnabled && inFlight !== 0) { fault('tv_report_overlap'); return; }
      const frame = snapshot(args[8]);
      if (frame === null) {
        // Other report IDs may share the device. They are not a TV release.
        if (tvEnabled && tvOwned && args[8].readU8() === 1) fault('tv_report_invalid');
        return;
      }
      this.frame = frame;
      if (tvEnabled) { inFlight++; this.counted = true; }
      if (!tvEnabled || !tvReady || frame.tvSlots.length === 0) return;
      frame.patched = frame.bytes.slice();
      for (const offset of frame.tvSlots) {
        frame.patched[offset] = 0;
        frame.patched[offset + 1] = 0;
      }
      frame.needsRestore = true;
      try {
        for (const offset of frame.tvSlots)
          frame.pointer.add(offset).writeByteArray(new Uint8Array([0, 0]).buffer);
      } catch (_) {
        fault(restore(frame, true) ? 'tv_report_write_failed' : 'tv_report_restore_failed');
        return;
      }
      if (!sameBytes(new Uint8Array(frame.pointer.readByteArray(9)), frame.patched)) {
        fault(restore(frame, true) ? 'tv_report_verify_failed' : 'tv_report_restore_failed');
        return;
      }
      frame.cleared = true;
    } catch (_) {
      const frame = this.frame;
      fault(frame !== null && !restore(frame, true) ? 'tv_report_restore_failed' : 'report_read_failed');
    }
  },
  onLeave(retval) {
    try {
      const frame = this.frame;
      if (frame === null) return;
      // Even after stop/expiry, restore an already modified in-flight buffer
      // before allowing stop()'s Promise to resolve and the script to unload.
      if (!restore(frame)) { fault('tv_report_restore_failed'); return; }
      if (!active()) { if (!stopped) stop('lease_expired'); return; }
      if (retval.toUInt32() !== 0) {
        if (tvEnabled && (frame.cleared || tvOwned)) fault('tv_report_submit_failed');
        return;
      }
      if (tvEnabled) {
        if (!tvReady) {
          // Preserve preheld TV in the mask: Python's neutral gate must not
          // mistake it for a released remote. It cannot emit a gesture yet.
          if (frame.mask === 0) { tvReady = true; diagnostic('tv_capture_ready'); }
          else if ((frame.mask & 8) !== 0 && !preheldNoted) {
            preheldNoted = true;
            diagnostic('tv_preheld_passthrough');
          }
        } else if ((frame.mask & 8) !== 0) {
          if (!frame.cleared) { fault('tv_report_verify_failed'); return; }
          if (!tvOwned) diagnostic('tv_report_cleared');
          tvOwned = true;
        } else { tvOwned = false; }
      }
      send({type: 'mask', mask: frame.mask});
    } catch (_) { fault('report_read_failed'); }
    finally {
      if (this.counted) { inFlight--; this.counted = false; }
      finishStop();
    }
  }
});
timer = setInterval(() => { if (!active()) stop('lease_expired'); }, 500);
let initialized = false;
try { initialized = rc003Source.initialize(__SELECTED_SOURCE_KEY__, active); }
catch (_) { /* The fixed failure below preserves a callable cleanup RPC. */ }
if (!initialized) {
  stop('source_initialization_failed');
  throw new Error('source_initialization_failed');
}
send({type: 'loaded'});
