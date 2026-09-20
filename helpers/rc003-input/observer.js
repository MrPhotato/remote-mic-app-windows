'use strict';
// GPL-3.0-only. Report entry observation adapted from ZSTDJan commit
// 1e6b1d285f9cd50f30c5bc92ac7787a693fc993d. No writes to reports or key injection.
// Bit 0 = Back (F1), bit 1 = Volume Up (80), bit 2 = Volume Down (81).
let stopped = false;
let listener = null;
let timer = null;
const tick = new NativeFunction(Process.getModuleByName('kernel32.dll')
  .getExportByName('GetTickCount64'), 'uint64', []);
function now() { return tick().toNumber(); }
let deadline = now() + 10000;
function active() { return !stopped && now() < deadline; }
function snapshot(pointer) {
  if (pointer.isNull()) return null;
  const b = new Uint8Array(pointer.readByteArray(9));
  if (b[0] !== 1 || b[1] !== 0 || b[2] !== 0) return null;
  let mask = 0;
  for (let i = 3; i < 9; i += 2) {
    const usage = b[i] | (b[i + 1] << 8);
    if (usage === 0xf1) mask |= 1;
    else if (usage === 0x80) mask |= 2;
    else if (usage === 0x81) mask |= 4;
  }
  return mask;
}
function stop(reason) {
  if (stopped) return;
  stopped = true;
  if (timer !== null) clearInterval(timer);
  if (listener !== null) listener.detach();
  rc003Source.close();
  send({type: 'stopped', reason});
}
listener = Interceptor.attach(Process.getModuleByName('ntdll.dll')
  .getExportByName('NtDeviceIoControlFile'), {
  onEnter(args) {
    this.mask = null;
    if (!active() || args[5].toUInt32() !== 0x80018483) return;
    try {
      if (args[7].toUInt32() !== 8 || args[9].toUInt32() !== 9 ||
          args[6].isNull() || args[8].isNull() || !rc003Source.matches(args)) return;
      if (args[6].add(4).readU8() !== 2 || args[6].add(5).readU8() !== 1) return;
      this.mask = snapshot(args[8]);
    } catch (_) { send({type: 'fault', reason: 'report_read_failed'}); }
  },
  onLeave(retval) {
    if (this.mask !== null && active() && retval.toUInt32() === 0)
      send({type: 'mask', mask: this.mask});
  }
});
timer = setInterval(() => { if (!active()) stop('lease_expired'); }, 500);
if (!rc003Source.initialize(__SELECTED_SOURCE_KEY__, active)) {
  stop('source_initialization_failed');
  throw new Error('source_initialization_failed');
}
rpc.exports = {
  renew() {
    if (!active() || !rc003Source.status().ready) return false;
    deadline = now() + 10000;
    return true;
  },
  stop() { stop('client_cleanup'); return true; }
};
send({type: 'loaded'});
