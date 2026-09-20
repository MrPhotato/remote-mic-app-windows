const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
let clock = 0, callbacks, reads = 0, allowed = true, detached = false;
const messages = [];
const moduleStub = {getExportByName: name => name};
const context = {
  Process: {getModuleByName: () => moduleStub},
  NativeFunction: function() { return () => ({toNumber: () => clock}); },
  Interceptor: {attach: (_address, handlers) => { callbacks = handlers; return {detach() {detached = true;}}; }},
  rc003Source: {initialize: () => true, matches: () => allowed, close() {}, status: () => ({ready: true})},
  send: m => messages.push(m), setInterval: () => 1, clearInterval() {}, rpc: {exports: {}}
};
vm.createContext(context);
vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'observer.js'), 'utf8').replace('__SELECTED_SOURCE_KEY__', '"' + 'a'.repeat(64) + '"'), context);
function ptr(bytes, offset=0) { return {isNull: () => false, add: n => ptr(bytes, offset+n),
  readU8: () => bytes[offset], readByteArray(n) {reads++; return Uint8Array.from(bytes.slice(offset,offset+n)).buffer;}}; }
const number = n => ({toUInt32: () => n});
function report(bytes, status=0) {
  const args = Array(10).fill(null);
  args[5] = number(0x80018483); args[6] = ptr([0,0,0,0,2,1,0,0]); args[7] = number(8);
  args[8] = ptr(bytes); args[9] = number(9);
  const invocation = {};
  callbacks.onEnter.call(invocation,args); callbacks.onLeave.call(invocation,number(status));
}
report([1,0,0,0xf1,0,0x80,0,0x81,0]);
assert.equal(messages.at(-1).mask,7);
report([1,0,0,0x3e,0,0x52,0,0,0]); // F5 and direction never enter the three-bit mask.
assert.equal(messages.at(-1).mask,0);
const before = messages.length;
report([1,0,0,0xf1,1,0,0,0,0]);
assert.equal(messages.at(-1).mask,0); // both bytes of each slot count.
report([1,0,0,0xf1,0,0,0,0,0],0x103);
assert.equal(messages.length,before+1); // pending is never accepted.
allowed = false; const oldReads = reads;
report([1,0,0,0xf1,0,0,0,0,0]);
assert.equal(reads,oldReads); // foreign source buffer never read.
allowed = true; clock = 10001;
assert.equal(context.rpc.exports.renew(),false);
context.rpc.exports.stop(); assert.equal(detached,true);
console.log('observer checks passed');
