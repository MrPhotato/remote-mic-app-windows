'use strict';
// Strict optional-helper source attribution. Substantially extracted from:
// ZSTDJan/windows-remote-mic-app, commit 1e6b1d285f9cd50f30c5bc92ac7787a693fc993d,
// frida_hid_tap_runtime.py, GPL-3.0. No report reads/writes, injection, or fallback
// attribution by host PID, report shape, handle value, or other device identity.
// Uses version-dependent UMDF implementation inspection; every proof must pass.
const rc003Source = (() => {
  let selectedSourceKey = null;
  let sourceApi = null;
  let sourceEntryStatus = 'not_checked';
  const sourceFrames = new Map();
  const sourceHandles = new Map();
  const sourceCallsites = new Map();
  const sourceVtables = new Map();
  const reasonCounts = Object.create(null);
  let activePredicate = null;
  let initialized = false;
  let ready = false;
  let stopped = true;
  const listeners = [];
  function record(reason) {
    const key = typeof reason === 'string' && /^[a-z_]{1,64}$/.test(reason)
      ? reason : 'source_validation_failed';
    reasonCounts[key] = (reasonCounts[key] || 0) + 1;
  }
  function active() {
    if (!ready || stopped || activePredicate === null) return false;
    try { return activePredicate() === true; }
    catch (_) { record('activity_check_failed'); return false; }
  }
  // Upstream diagnostic hooks intentionally have no output or identity export.
  function recordSourceEvidence(_key, _diagnostic) {}
function insideModule(module, address, size = 1) {
  return address.compare(module.base) >= 0 && address.add(size).compare(module.base.add(module.size)) <= 0;
}

function initializeSourceApi() {
  if (Process.arch !== "x64") return;
  try {
    const kernel = Process.getModuleByName("kernel32.dll");
    const getSystemDirectory = new NativeFunction(kernel.getExportByName("GetSystemDirectoryW"), "uint", ["pointer", "uint"]);
    const directory = Memory.alloc(65536);
    const count = getSystemDirectory(directory, 32768);
    if (!count || count >= 32768) return;
    const prefix = directory.readUtf16String(count) + "\\";
    const registry = Module.load(prefix + "advapi32.dll");
    const crypto = Module.load(prefix + "bcrypt.dll");
    const api = {
      query: new NativeFunction(registry.getExportByName("RegQueryValueExW"), "long", ["pointer", "pointer", "pointer", "pointer", "pointer", "pointer"]),
      lookup: new NativeFunction(Process.getModuleByName("ntdll.dll").getExportByName("RtlLookupFunctionEntry"), "pointer", ["pointer", "pointer", "pointer"]),
      hash: new NativeFunction(crypto.getExportByName("BCryptHash"), "int", ["pointer", "pointer", "uint", "pointer", "uint", "pointer", "uint"]),
      close: new NativeFunction(crypto.getExportByName("BCryptCloseAlgorithmProvider"), "int", ["pointer", "uint"]),
      containerName: Memory.allocUtf16String("ContainerID")
    };
    const open = new NativeFunction(crypto.getExportByName("BCryptOpenAlgorithmProvider"), "int", ["pointer", "pointer", "pointer", "uint"]);
    const algorithm = Memory.alloc(Process.pointerSize);
    if (open(algorithm, Memory.allocUtf16String("SHA256"), ptr(0), 0) !== 0) return;
    api.algorithm = algorithm.readPointer();
    sourceApi = api;
  } catch (_error) { sourceApi = null; }
}

function containerSourceKey(key, diagnostic = {}) {
  diagnostic.reason = "device_registry_unavailable";
  if (sourceApi === null || key.isNull()) return null;
  const size = Memory.alloc(4), type = Memory.alloc(4), value = Memory.alloc(80);
  type.writeU32(0);
  size.writeU32(80);
  const status = sourceApi.query(key, sourceApi.containerName, ptr(0), type, value, size);
  diagnostic.value_type = type.readU32();
  diagnostic.value_bytes = size.readU32();
  if (status !== 0) {
    diagnostic.reason = "device_registry_read_failed"; diagnostic.winerror = status; return null;
  }
  diagnostic.reason = "device_identity_invalid";
  if (type.readU32() !== 1 || (size.readU32() !== 74 && size.readU32() !== 78)) return null;
  const count = size.readU32() / 2;
  if (value.add((count - 1) * 2).readU16() !== 0) return null;
  const text = value.readUtf16String(count - 1);
  if (!/^(?:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\{[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\})$/i.test(text)) return null;
  const canonical = text.replace(/[{}-]/g, "");
  if (/^0{31}[01]$/.test(canonical)) return null;
  const bytes = asciiBytes("RemoteMic physical remote v1\0").concat(canonical.match(/../g).map(x => parseInt(x, 16)));
  const data = Memory.alloc(bytes.length), digest = Memory.alloc(32);
  data.writeByteArray(bytes);
  diagnostic.reason = "device_identity_hash_failed";
  return sourceApi.hash(sourceApi.algorithm, ptr(0), 0, data, bytes.length, digest, 32) === 0 ? hex(digest, 32) : null;
}

function instanceSourceKey(key, diagnostic = {}, evidence = {}) {
  // The verified stack getter can return Device Parameters, not the Enum instance.
  // Derive only this live device's exact instance; never search other device keys.
  diagnostic.reason = "device_registry_unavailable";
  if (sourceApi === null || key.isNull()) return null;
  try {
    const query = new NativeFunction(Process.getModuleByName("ntdll.dll").getExportByName("NtQueryKey"),
      "int", ["pointer", "int", "pointer", "uint", "pointer"]);
    const buffer = Memory.alloc(8192), needed = Memory.alloc(4);
    const status = query(key, 3, buffer, 8192, needed);
    evidence.key_name_status = status >>> 0;
    if (status !== 0) return null;
    const length = buffer.readU32();
    if (length === 0 || length > 8188 || length % 2 !== 0) return null;
    const name = buffer.add(4).readUtf16String(length / 2);
    if (name.length * 2 !== length) return null;
    const parts = name.split("\\"), lower = parts.map(value => value.toLowerCase());
    const index = 5;
    evidence.key_scope = "other";
    if (lower.slice(0, 4).join("\\") !== "\\registry\\machine\\system" ||
        !/^(currentcontrolset|controlset[0-9]{3})$/.test(lower[4]) || lower[index] !== "enum" ||
        parts.length < index + 4 || parts.slice(4).some(part => !part || part.includes("\0"))) return null;
    const tail = lower.slice(index + 4);
    evidence.key_scope = tail.length === 0 ? "device_instance" :
      tail[0] === "device parameters" ? "device_parameters" : "enum_descendant";
    evidence.key_depth = tail.length;
    evidence.key_bus = ["bthledevice", "bthenum", "bthle", "hid"].includes(lower[index + 1]) ? lower[index + 1] : "other";
    if (tail.length > 1 || (tail.length === 1 && tail[0] !== "device parameters")) return null;
    if (tail.length === 0) return containerSourceKey(key, diagnostic);
    const registry = Process.getModuleByName("advapi32.dll");
    const open = new NativeFunction(registry.getExportByName("RegOpenKeyExW"), "long",
      ["pointer", "pointer", "uint", "uint", "pointer"]);
    const close = new NativeFunction(registry.getExportByName("RegCloseKey"), "long", ["pointer"]);
    const root = Memory.alloc(Process.pointerSize);
    evidence.instance_open_status = open(ptr("0xffffffff80000002"),
      Memory.allocUtf16String(parts.slice(3, index + 4).join("\\")), 0, 1, root);
    if (evidence.instance_open_status !== 0) {
      diagnostic.reason = "device_registry_read_failed";
      diagnostic.winerror = evidence.instance_open_status;
      return null;
    }
    try {
      const identity = containerSourceKey(root.readPointer(), diagnostic);
      evidence.instance_value_status = diagnostic.winerror === undefined ? 0 : diagnostic.winerror;
      evidence.instance_identity_valid = identity !== null;
      if (identity !== null) {
        evidence.instance_ref = identity.slice(0, 12);
        evidence.instance_matches_selected = identity === selectedSourceKey;
      }
      return identity;
    } finally { close(root.readPointer()); }
  } catch (_error) { evidence.query_incomplete = true; }
  return null;
}

function pointerLoad(instruction, register) {
  const ops = instruction.operands;
  if (instruction.mnemonic !== "mov" || ops.length !== 2 || ops[0].type !== "reg" ||
      ops[0].value !== register || ops[1].type !== "mem" || ops[1].size !== 8) return null;
  const mem = ops[1].value;
  if (mem.index || mem.segment || !Number.isInteger(mem.disp) || mem.disp <= 0 || mem.disp > 4096) return null;
  return mem;
}

function deviceStackLoad(module, returnAddress) {
  if (sourceApi === null || !insideModule(module, returnAddress)) return null;
  const cacheKey = returnAddress.toString();
  if (sourceCallsites.has(cacheKey)) return sourceCallsites.get(cacheKey);
  let result = null;
  try {
    const imageBase = Memory.alloc(8);
    const entry = sourceApi.lookup(returnAddress.sub(1), imageBase, ptr(0));
    if (entry.isNull() || !imageBase.readPointer().equals(module.base)) return null;
    let cursor = module.base.add(entry.readU32());
    const end = module.base.add(entry.add(4).readU32());
    if (!insideModule(module, cursor) || returnAddress.compare(end) > 0 ||
        returnAddress.sub(cursor).toUInt32() > 8192) return null;
    let candidate = null;
    while (cursor.compare(returnAddress) < 0) {
      const instruction = Instruction.parse(cursor);
      const next = instruction.next;
      if (next.compare(returnAddress) > 0) break;
      const load = pointerLoad(instruction, "rcx");
      if (load && /^(rbx|rbp|rsi|rdi|r12|r13|r14|r15)$/.test(load.base)) {
        candidate = {base: load.base, disp: load.disp};
      } else if (instruction.mnemonic === "call" && next.equals(returnAddress)) {
        result = candidate;
      } else if (!/^(mov|lea|nop)$/.test(instruction.mnemonic) ||
                 (instruction.operands[0] && instruction.operands[0].type === "reg" &&
                  !/^(rax|eax|r8|r8d|r9|r9d|rdx|edx)$/.test(instruction.operands[0].value))) {
        candidate = null;
      }
      cursor = next;
    }
  } catch (_error) { result = null; }
  if (sourceCallsites.size < 64) sourceCallsites.set(cacheKey, result);
  return result;
}

function deviceStackInterface(module, queryInterface) {
  if (!insideModule(module, queryInterface) || sourceApi === null) return false;
  const imageBase = Memory.alloc(8);
  const entry = sourceApi.lookup(queryInterface, imageBase, ptr(0));
  if (entry.isNull() || !imageBase.readPointer().equals(module.base) ||
      !module.base.add(entry.readU32()).equals(queryInterface)) return false;
  const end = module.base.add(entry.add(4).readU32());
  if (!insideModule(module, end.sub(1)) || end.sub(queryInterface).toUInt32() > 512) return false;
  // Windows builds omit C++ RTTI. Identify the interface by the two halves of
  // its IID checked by QueryInterface (verified with Microsoft's matching PDB).
  // Do not call QueryInterface/AddRef or execute any undocumented host method.
  const iid = "7a2dfa5b66f7d34a959ed8fe0f0e83e1";
  let cursor = queryInterface, previous = null, low = false, high = false;
  while (cursor.compare(end) < 0) {
    const instruction = Instruction.parse(cursor), ops = instruction.operands;
    if (instruction.next.compare(end) > 0) return false;
    if (previous && /^(sub|cmp)$/.test(instruction.mnemonic) && ops.length === 2 &&
        ops[0].type === "reg" && ops[0].value === "rax" && ops[1].type === "mem" &&
        ops[1].size === 8 && ops[1].value.base === "rip") {
      const before = previous.operands;
      if (previous.mnemonic === "mov" && before.length === 2 && before[0].type === "reg" &&
          before[0].value === "rax" && before[1].type === "mem" && before[1].size === 8 &&
          before[1].value.base === "rdx" && !before[1].value.index) {
        const half = before[1].value.disp;
        const address = instruction.next.add(ops[1].value.disp).sub(half);
        if ((half === 0 || half === 8) && insideModule(module, address, 16) && hex(address, 16) === iid) {
          if (half === 0) low = true; else high = true;
        }
      }
    }
    previous = instruction; cursor = instruction.next;
  }
  return low && high;
}

function deviceRegistryKey(module, object) {
  const vtable = object.readPointer();
  if (!insideModule(module, vtable, 32 * 8)) return null;
  let offset = sourceVtables.get(vtable.toString());
  if (offset !== undefined) return object.add(offset).readPointer();
  if (!deviceStackInterface(module, vtable.readPointer())) return null;
  // This is a pure field getter in the verified UMDF ABI. Decode it rather
  // than invoking an undocumented C++ method or hard-coding its field offset.
  const getter = vtable.add(31 * 8).readPointer();
  if (!insideModule(module, getter, 16)) return null;
  const instruction = Instruction.parse(getter), field = pointerLoad(instruction, "rax");
  if (field === null || field.base !== "rcx" || Instruction.parse(instruction.next).mnemonic !== "ret") return null;
  if (sourceVtables.size < 64) sourceVtables.set(vtable.toString(), field.disp);
  return object.add(field.disp).readPointer();
}

function resolveCopyDevice(context, returnAddress, handle, diagnostic = {}) {
  try {
    diagnostic.reason = "source_host_unverified";
    const module = Process.findModuleByAddress(returnAddress);
    diagnostic.caller_module = module && ["wudfhost.exe", "kernel32.dll", "kernelbase.dll", "ntdll.dll", "apphelp.dll"].includes(module.name.toLowerCase()) ? module.name.toLowerCase() : "other";
    if (module) diagnostic.caller_rva = returnAddress.sub(module.base).toUInt32();
    if (module === null || module.name.toLowerCase() !== "wudfhost.exe") return null;
    diagnostic.reason = "copy_callsite_unverified";
    const load = deviceStackLoad(module, returnAddress);
    if (load === null) return null;
    diagnostic.reason = "copy_object_unverified";
    const object = context[load.base];
    if (!object || object.isNull() || !object.add(load.disp).readPointer().equals(handle)) return null;
    diagnostic.reason = "device_interface_unverified";
    const registryKey = deviceRegistryKey(module, object);
    if (registryKey === null || registryKey.isNull()) return null;
    const cached = sourceHandles.get(handle.toString());
    if (cached && cached.object.equals(object) && cached.registryKey.equals(registryKey)) return cached.key;
    const key = instanceSourceKey(registryKey, diagnostic);
    if (key === null) recordSourceEvidence(registryKey, diagnostic);
    if (key !== null && sourceHandles.size < 64) sourceHandles.set(handle.toString(), {object, registryKey, key});
    return key;
  } catch (_error) { return null; }
}

function asciiBytes(text) {
  const result = [];
  for (let index = 0; index < text.length; index++) {
    result.push(text.charCodeAt(index) & 0xff);
  }
  return result;
}

function hex(pointer, length) {
  if (pointer.isNull() || length <= 0) return "";
  const bytes = new Uint8Array(pointer.readByteArray(length));
  let result = "";
  for (let index = 0; index < bytes.length; index++) {
    result += bytes[index].toString(16).padStart(2, "0");
  }
  return result;
}

function deviceIoControlImportSlot(module) {
  // Read the loaded PE's name/index pairing ourselves. Frida has reported
  // the preceding import's slot for DeviceIoControl in a live WUDFHost.
  function span(rva, size) {
    if (!Number.isInteger(rva) || rva < 0 || !Number.isInteger(size) || size < 1 ||
        rva > module.size - size) throw Error("invalid import range");
    return module.base.add(rva);
  }
  function nameAt(rva) {
    let name = "";
    for (let i = 0; i < 256; i++) {
      const ch = span(rva + i, 1).readU8();
      if (ch === 0) return name;
      name += String.fromCharCode(ch);
    }
    throw Error("unterminated import name");
  }
  sourceEntryStatus = "invalid_import_table";
  if (span(0, 64).readU16() !== 0x5a4d) return null;
  const ntRva = module.base.add(0x3c).readU32(), nt = span(ntRva, 24);
  if (nt.readU32() !== 0x4550 || nt.add(4).readU16() !== 0x8664 ||
      nt.add(20).readU16() < 128) return null;
  const optional = span(ntRva + 24, nt.add(20).readU16());
  if (optional.readU16() !== 0x20b || optional.add(56).readU32() !== module.size ||
      optional.add(108).readU32() < 2) return null;
  const directoryRva = optional.add(120).readU32(), directorySize = optional.add(124).readU32();
  if (directoryRva === 0 || directorySize < 20) return null;
  span(directoryRva, directorySize);
  let found = null;
  for (let index = 0; index < Math.min(1024, Math.floor(directorySize / 20)); index++) {
    const descriptor = span(directoryRva + index * 20, 20);
    const lookup = descriptor.readU32(), table = descriptor.add(16).readU32();
    const dllName = descriptor.add(12).readU32();
    if ((lookup | table | dllName | descriptor.add(4).readU32() | descriptor.add(8).readU32()) === 0) {
      sourceEntryStatus = found === null ? "import_missing" : "target_unverified";
      return found;
    }
    if (lookup === 0 || table === 0 || dllName === 0 || nameAt(dllName).length === 0) return null;
    let terminated = false;
    for (let entry = 0; entry < 4096; entry++) {
      const name = span(lookup + entry * 8, 8).readU64();
      if (name.compare(uint64(0)) === 0) { terminated = true; break; }
      const slot = span(table + entry * 8, 8);
      if (name.and(uint64("0x8000000000000000")).compare(uint64(0)) !== 0) continue;
      if (name.compare(uint64(0xffffffff)) > 0) return null;
      const nameRva = name.toNumber();
      span(nameRva, 3);
      if (nameAt(nameRva + 2) !== "DeviceIoControl") continue;
      if (found !== null) { sourceEntryStatus = "import_ambiguous"; return null; }
      found = slot;
    }
    if (!terminated) return null;
  }
  return null;
}

function deviceIoControlEntry(module) {
  try {
    const slot = deviceIoControlImportSlot(module);
    if (slot === null) return null;
    // API-set resolution can report a different address from the live IAT.
    // Read the slot so the source frame is captured before any Win32 wrapper
    // changes its caller's nonvolatile registers (notably the device in rbp).
    const target = slot.readPointer();
    if (target.isNull()) return null;
    for (const name of ["kernel32.dll", "kernelbase.dll"]) {
      const owner = Process.findModuleByName(name);
      if (owner === null) continue;
      // Windows application compatibility may redirect GetProcAddress while
      // an existing import still points at the original PE export.
      const resolved = owner.findExportByName("DeviceIoControl");
      if (resolved !== null && target.equals(resolved)) { sourceEntryStatus = "verified"; return target; }
      const exported = owner.enumerateExports().find(item => item.name === "DeviceIoControl" && item.type === "function");
      if (exported && target.equals(exported.address)) { sourceEntryStatus = "verified"; return target; }
    }
    return null;
  } catch (_error) { return null; }
}

  function status() {
    return {ready, stopped, reason_counts: Object.assign({}, reasonCounts)};
  }
  function close() {
    stopped = true;
    ready = false;
    activePredicate = null;
    while (listeners.length !== 0) {
      const listener = listeners.pop();
      try { listener.detach(); }
      catch (_) { record('listener_detach_failed'); }
    }
    sourceFrames.clear();
    sourceHandles.clear();
    sourceCallsites.clear();
    sourceVtables.clear();
    selectedSourceKey = null;
    if (sourceApi !== null) {
      const api = sourceApi;
      sourceApi = null;
      try {
        if (api.close(api.algorithm, 0) !== 0) record('crypto_close_failed');
      } catch (_) { record('crypto_close_failed'); }
    }
    return status();
  }
  function initialize(selectedKey, isActive) {
    if (initialized) { record('already_initialized'); return false; }
    initialized = true;
    if (Process.arch !== 'x64' || Process.pointerSize !== 8) {
      record('unsupported_architecture'); return false;
    }
    if (typeof selectedKey !== 'string' || !/^[a-f0-9]{64}$/.test(selectedKey) ||
        typeof isActive !== 'function') {
      record('invalid_source_selection'); return false;
    }
    selectedSourceKey = selectedKey;
    activePredicate = isActive;
    try {
      initializeSourceApi();
      if (sourceApi === null) throw new Error('source_api_unavailable');
      const host = Process.findModuleByName('WUDFHost.exe');
      if (host === null) throw new Error('source_host_unverified');
      const deviceEntry = deviceIoControlEntry(host);
      if (deviceEntry === null) {
        record(sourceEntryStatus);
        throw new Error('device_entry_unverified');
      }
      const closeEntry = Process.getModuleByName('ntdll.dll').getExportByName('NtClose');
      listeners.push(Interceptor.attach(deviceEntry, {
        onEnter(args) {
          this.sourceFrame = null;
          if (!active()) return;
          const thread = Process.getCurrentThreadId();
          const frames = sourceFrames.get(thread) || [];
          if (frames.length >= 32 || (!sourceFrames.has(thread) && sourceFrames.size >= 64)) {
            ready = false;
            record('source_frame_limit');
            return;
          }
          // Every active wrapper call gets a frame, including non-candidates.
          // A nested unrelated call must not borrow an outer candidate's proof.
          const frame = {handle: args[0], metadata: args[2], inputLength: args[3].toUInt32(),
            buffer: args[4], outputLength: args[5].toUInt32(), key: null, valid: false};
          frames.push(frame);
          sourceFrames.set(thread, frames);
          this.sourceFrame = frame;
          this.sourceThread = thread;
          if (args[1].toUInt32() !== 0x80018483 || frame.inputLength !== 8 ||
              frame.outputLength !== 9 || frame.metadata.isNull() || frame.buffer.isNull()) return;
          const diagnostic = {};
          frame.key = resolveCopyDevice(this.context, this.returnAddress, frame.handle, diagnostic);
          if (frame.key === null) record(diagnostic.reason || 'source_validation_failed');
          else if (frame.key !== selectedSourceKey) record('source_not_selected');
          else frame.valid = true;
        },
        onLeave(_retval) {
          if (this.sourceFrame === null || stopped) return;
          const frames = sourceFrames.get(this.sourceThread);
          if (!frames) return;
          if (frames.pop() !== this.sourceFrame) {
            record('source_frame_mismatch');
            sourceFrames.delete(this.sourceThread);
          } else if (frames.length === 0) sourceFrames.delete(this.sourceThread);
        }
      }));
      listeners.push(Interceptor.attach(closeEntry, {
        onEnter(args) {
          if (stopped) return;
          // No report or device content is read. Handle reuse loses its cache.
          const handle = args[0];
          sourceHandles.delete(handle.toString());
          for (const [sourceHandle, source] of sourceHandles) {
            if (source.registryKey.equals(handle)) sourceHandles.delete(sourceHandle);
          }
          for (const frames of sourceFrames.values()) {
            for (const frame of frames) if (frame.handle.equals(handle)) frame.valid = false;
          }
        }
      }));
      stopped = false;
      ready = true;
      return true;
    } catch (error) {
      record(error && error.message);
      record('source_initialization_failed');
      close();
      return false;
    }
  }
  function matches(args) {
    if (!active()) return false;
    try {
      if (args[5].toUInt32() !== 0x80018483) return false;
      const frames = sourceFrames.get(Process.getCurrentThreadId());
      const frame = frames && frames[frames.length - 1];
      if (!frame || !frame.valid || frame.key !== selectedSourceKey ||
          !frame.handle.equals(args[0]) || !frame.metadata.equals(args[6]) ||
          !frame.buffer.equals(args[8]) || frame.inputLength !== args[7].toUInt32() ||
          frame.outputLength !== args[9].toUInt32()) {
        record('copy_frame_unverified');
        return false;
      }
      return true;
    } catch (_) { record('copy_frame_exception'); return false; }
  }
  return {initialize, matches, close, status};
})();
