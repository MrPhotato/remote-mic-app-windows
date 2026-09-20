# SayAll optional RC003 key helper

This Windows x64 helper is an explicit, elevated, optional source for Back and
Volume Up/Down. The main SayAll application remains unelevated. The helper does
not install drivers, change boot settings, modify device reports, suppress keys,
inject keystrokes, or handle the microphone/F5 path. It does temporarily inject
Frida into the selected WUDFHost and inspect version-dependent UMDF internals.

## Build and deployment

The minimum supported OS is **Windows 10 1809 (x64)**. The package excludes only
`ucrtbase.dll`: [Microsoft's UCRT deployment documentation](https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment?view=msvc-170)
states that Windows 10 and later provide UCRT as an OS component and always use
the system copy, even if an application bundles a newer copy. The package keeps
`VCRUNTIME140.dll` and any API-set forwarders selected by PyInstaller. Manifest
generation rejects a bundled UCRT or missing Python, Frida, or VC runtime.

Run `scripts/build-rc003-helper.ps1` with Windows x64 Python **3.11 or newer** and Node.js.
The reference build uses Python 3.11.9; the manifest records the actual runtime.
It creates an isolated build venv, verifies the hash-pinned dependency lock,
runs simulated tests, and packages an **asInvoker, windowed, onedir** executable.
Users of the result do not need Python. Output is
`target/rc003-helper/SayAllKeyHelper/`; its complete file manifest is the sibling
`manifest.json` (also copied inside the bundle, excluded from its own list).
This is a fixed-input build workflow, not a claim of bitwise
identical PE output across machines. Existing bundles are sent to the recycle bin.

The app's trusted bootstrap must verify the manifest and stage the entire bundle
in an administrator-controlled directory before elevating it. Do not elevate a
bundle whose Python runtime, DLLs or JS are writable by ordinary users. There is
no arbitrary script, target PID, log path, or command-execution option.

## Protocol 1

CLI is only `--port <1..65535> --token <64 hex> --parent-pid <uint32>`.
The helper verifies the loopback listener's PID and retains a parent process
handle. It connects to `127.0.0.1`, sends a hello containing protocol 1, the token
and its own PID. The parent verifies that hello before accepting any state.
All messages are UTF-8 newline JSON, at most 4096 bytes including newline.

The parent sends a lease every second:
`{"type":"lease","generation":1,"enabled":true,"device_id":"<BLE AEP ID>"}`.
It may send `{"type":"stop"}`. Disabled leases may use an empty/null device ID.
The device ID is used only in memory: its final peer address selects exactly one
`Enum/BTHLE/Dev_<address>` instance, whose ContainerID must match the independently
selected unique RC003 HID1812/2717:32b8/REV00a4 target. No match means no capture.

Helper messages are `hello`, `heartbeat`, `status`, `state`, and `diagnostic`.
State contains generation, increasing sequence, and an absolute `pressed_mask`:
**1 = Back, 2 = Volume Up, 4 = Volume Down**. No other usage is exported.
Status phases are waiting/ready/failed, with fixed English reason codes.
Diagnostics contain fixed codes only, never exception text, paths, IDs, addresses,
ContainerID hashes, tokens or reports. The hello token is authentication data and
must never be included in parent logs.

Each attachment first reports `waiting/awaiting_neutral`; it exports no nonzero
state until a successful, attributed entry report has a real three-key mask 0.
That real zero emits state 0 then `ready/source_armed`. A direction press can
provide this neutral three-key report after initial enable. Idle time does not
disarm an established capture. Reattachment always requires fresh neutrality.
This deliberately avoids claiming that a synthetic zero proves physical release.

EOF, parent exit, or five seconds without a valid lease aborts calls and ends the
helper. The script also detaches its hooks if its ten-second lease expires.
Changed selection/generation/enable state, target revalidation failure, script
failure, or host restart first stops/unloads/detaches the old capture. Transient
target failures retry after two seconds while the parent lease remains valid.
The helper never restarts or terminates WUDFHost. Cleanup status precedes state
release: the parent must cancel pending gestures and clear only this input source
on waiting/failure/EOF/deadline, even if the helper cannot send a final message.

## Source and license

GPL-3.0-only; substantial source binding and target-association adaptations from
[ZSTDJan/windows-remote-mic-app](https://github.com/ZSTDJan/windows-remote-mic-app/tree/1e6b1d285f9cd50f30c5bc92ac7787a693fc993d),
commit `1e6b1d285f9cd50f30c5bc92ac7787a693fc993d`, modules
`apps/windows/rc003/src/ovb_rc003/frida_hid_tap_runtime.py` and
`frida_hid_tap_injector.py`. Changes on 2026-09-20 remove report mutation,
Gadget persistence, broad event export, and upstream socket control; retain
strict source binding and add bounded local IPC, narrow masks, and lifecycle guards.
The upstream GPL license is copied in `licenses/GPL-3.0.txt`.

Frida 17.18.0: wxWindows Library Licence 3.1, with its referenced LGPL text;
Python 3.11.9 and PyInstaller license/bootloader exception are also included.
Distribute matching helper/JS source, dependency lock and build recipe alongside
the binary under applicable licenses. The copied top-level licenses do not claim
to complete an audit of every third-party component embedded in Frida's binary.
Physical product integration, reconnect/sleep behavior and cold first use require
separate real-device validation; simulated tests do not establish those outcomes.
