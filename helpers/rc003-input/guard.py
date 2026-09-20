"""Fail-closed target selection for the optional SayAll RC003 input helper.

Host association follows ZSTDJan/windows-remote-mic-app at
1e6b1d285f9cd50f30c5bc92ac7787a693fc993d, frida_hid_tap_runtime.py:1195-1341.
This module never injects, launches, terminates, installs, or prints anything.
Call enable_debug_privilege() explicitly in the elevated helper if needed.
"""

import ctypes
from ctypes import wintypes
from dataclasses import dataclass, field
from functools import lru_cache
import hashlib
import os
import re
import socket
import struct
import threading
import uuid

try:
    import winreg
except ImportError:
    winreg = None

_ENUM = r"SYSTEM\CurrentControlSet\Enum"
_SERVICE_PREFIX = "{00001812-0000-1000-8000-00805f9b34fb}"
_PRODUCT_TOKEN = "dev_vid&012717_pid&32b8_rev&00a4"
_DIAGNOSTIC = r"Device Parameters\WUDFDiagnosticInfo"
_QUERY_AND_SYNCHRONIZE = 0x1000 | 0x100000


class GuardError(RuntimeError):
    """Only a stable reason and optional numeric Win32 error; no device identity."""

    def __init__(self, reason, winerror=None):
        self.code = reason
        self.reason = reason
        self.winerror = winerror
        super().__init__(reason if winerror is None else f"{reason}: win32={winerror}")


@dataclass(repr=False)
class Target:
    pid: int
    creation_time_100ns: int
    _handle: int = field(repr=False)
    _instance: str = field(repr=False)
    _container: uuid.UUID = field(repr=False)
    _counts: dict = field(repr=False)
    _members: tuple = field(repr=False)
    _require_exclusive: bool = field(repr=False)
    _lock: object = field(default_factory=threading.RLock, repr=False)

    def __repr__(self):
        return f"Target(pid={self.pid}, closed={not bool(self._handle)})"

    def summary(self):
        with self._lock:
            return {"pid": self.pid, "closed": not bool(self._handle), **self._counts}

    def binding_key(self):
        """Return the private upstream-compatible source-binding key; never log it."""
        with self._lock:
            if not self._handle:
                raise GuardError("target_closed")
            return hashlib.sha256(b"RemoteMic physical remote v1\0" + self._container.bytes).hexdigest()


@lru_cache(maxsize=1)
def _apis():
    if os.name != "nt" or winreg is None:
        raise GuardError("windows_required")
    if ctypes.sizeof(ctypes.c_void_p) != 8:
        raise GuardError("64bit_helper_required")
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel.WaitForSingleObject.restype = wintypes.DWORD
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    kernel.GetProcessTimes.restype = wintypes.BOOL
    kernel.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD,
                                                wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    kernel.QueryFullProcessImageNameW.restype = wintypes.BOOL
    kernel.GetSystemDirectoryW.argtypes = [wintypes.LPWSTR, wintypes.UINT]
    kernel.GetSystemDirectoryW.restype = wintypes.UINT
    kernel.GetCurrentProcess.argtypes = []
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    if not hasattr(kernel, "IsWow64Process2"):
        raise GuardError("architecture_api_unavailable")
    kernel.IsWow64Process2.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.USHORT),
                                      ctypes.POINTER(wintypes.USHORT)]
    kernel.IsWow64Process2.restype = wintypes.BOOL
    return kernel


def _os_error(reason):
    return GuardError(reason, ctypes.get_last_error())


def _names(key):
    index = 0
    while True:
        try:
            name = winreg.EnumKey(key, index)
        except OSError as error:
            if getattr(error, "winerror", None) == 259:
                return
            raise
        index += 1
        yield name


def _open(path):
    return winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, path, 0,
                          winreg.KEY_READ | winreg.KEY_WOW64_64KEY)


def _host_pid(instance, required=False):
    try:
        with _open(_ENUM + "\\" + instance + "\\" + _DIAGNOSTIC) as key:
            value, kind = winreg.QueryValueEx(key, "HostPid")
    except FileNotFoundError:
        if required:
            raise GuardError("target_host_pid_unavailable") from None
        return None
    if kind not in (winreg.REG_DWORD, winreg.REG_QWORD) or not isinstance(value, int) or not 0 <= value <= 0xFFFFFFFF:
        raise GuardError("invalid_host_pid_metadata")
    if required and value == 0:
        raise GuardError("target_host_pid_unavailable")
    return value


def _registry_snapshot(require_exclusive=True):
    _apis()
    try:
        instances = []
        services = 0
        base = _ENUM + r"\BTHLEDevice"
        with _open(base) as root:
            for service in _names(root):
                folded_service = service.casefold()
                if not (folded_service.startswith(_SERVICE_PREFIX) and _PRODUCT_TOKEN in folded_service):
                    continue
                services += 1
                with _open(base + "\\" + service) as service_key:
                    for instance in _names(service_key):
                        instances.append("BTHLEDevice\\" + service + "\\" + instance)
        if services != 1 or len(instances) != 1:
            raise GuardError("target_instance_not_unique")
        selected = instances[0]
        with _open(_ENUM + "\\" + selected) as key:
            value, kind = winreg.QueryValueEx(key, "ContainerID")
        if kind != winreg.REG_SZ or not isinstance(value, str):
            raise GuardError("invalid_container_id")
        try:
            container = uuid.UUID(value.strip("{}"))
        except (ValueError, AttributeError):
            raise GuardError("invalid_container_id") from None
        if container.int in (0, 1):
            raise GuardError("invalid_container_id")
        pid = _host_pid(selected, required=True)
        members = []
        with _open(_ENUM) as root:
            for enumerator in _names(root):
                with _open(_ENUM + "\\" + enumerator) as enum_key:
                    for device in _names(enum_key):
                        with _open(_ENUM + "\\" + enumerator + "\\" + device) as device_key:
                            for instance in _names(device_key):
                                member = "\\".join((enumerator, device, instance))
                                if _host_pid(member) == pid:
                                    members.append(member)
        normalized_members = tuple(sorted(member.casefold() for member in members))
        if normalized_members.count(selected.casefold()) != 1:
            raise GuardError("selected_target_missing_from_host")
        exclusive = len(normalized_members) == 1
        if require_exclusive and not exclusive:
            raise GuardError("host_not_exclusively_selected_target")
        counts = {"matching_service_keys": services, "matching_instances": len(instances),
                  "host_member_count": len(normalized_members), "exclusive": exclusive}
        return selected, container, pid, counts, normalized_members
    except OSError as error:
        code = getattr(error, "winerror", None)
        raise GuardError("registry_access_denied" if code == 5 else "registry_scan_failed", code) from None


def _process_identity(handle):
    kernel = _apis()
    wait = kernel.WaitForSingleObject(handle, 0)
    if wait == 0:
        raise GuardError("target_process_exited")
    if wait != 258:
        raise _os_error("process_liveness_query_failed")
    image = ctypes.create_unicode_buffer(32768)
    length = wintypes.DWORD(len(image))
    if not kernel.QueryFullProcessImageNameW(handle, 0, image, ctypes.byref(length)):
        raise _os_error("process_image_query_failed")
    system_dir = ctypes.create_unicode_buffer(32768)
    size = kernel.GetSystemDirectoryW(system_dir, len(system_dir))
    if not size or size >= len(system_dir):
        raise _os_error("system_directory_query_failed")
    allowed = {os.path.normcase(os.path.normpath(os.path.join(system_dir.value, suffix)))
               for suffix in ("WUDFHost.exe", r"drivers\UMDF\WUDFHost.exe")}
    if not os.path.isabs(image.value) or os.path.normcase(os.path.normpath(image.value)) not in allowed:
        raise GuardError("unexpected_process_image")
    process_machine, native_machine = wintypes.USHORT(), wintypes.USHORT()
    if not kernel.IsWow64Process2(handle, ctypes.byref(process_machine), ctypes.byref(native_machine)):
        raise _os_error("process_architecture_query_failed")
    machine = process_machine.value or native_machine.value
    if machine != 0x8664:
        raise GuardError("target_not_x64")
    created, exited, kernel_time, user_time = (wintypes.FILETIME() for _ in range(4))
    if not kernel.GetProcessTimes(handle, ctypes.byref(created), ctypes.byref(exited),
                                  ctypes.byref(kernel_time), ctypes.byref(user_time)):
        raise _os_error("process_time_query_failed")
    return (created.dwHighDateTime << 32) | created.dwLowDateTime


def select_target(require_exclusive=True):
    """Hold a verified RC003 host. Shared mode requires separate per-source filtering."""
    if not isinstance(require_exclusive, bool):
        raise GuardError("invalid_exclusivity_mode")
    instance, container, pid, counts, members = _registry_snapshot(require_exclusive)
    handle = _apis().OpenProcess(_QUERY_AND_SYNCHRONIZE, False, pid)
    if not handle:
        raise _os_error("target_process_open_failed")
    try:
        target = Target(pid, _process_identity(handle), handle, instance, container, counts,
                        members, require_exclusive)
        revalidate(target)
        return target
    except BaseException:
        _apis().CloseHandle(handle)
        raise


def revalidate(target):
    """Recheck process identity and complete host membership; raise on uncertainty."""
    if not isinstance(target, Target):
        raise GuardError("invalid_target_object")
    with target._lock:
        if not target._handle:
            raise GuardError("target_closed")
        if _process_identity(target._handle) != target.creation_time_100ns:
            raise GuardError("target_process_identity_changed")
        instance, container, pid, counts, members = _registry_snapshot(target._require_exclusive)
        if pid != target.pid or instance.casefold() != target._instance.casefold() or container != target._container:
            raise GuardError("target_device_identity_changed")
        if members != target._members:
            raise GuardError("target_host_membership_changed")
        if _process_identity(target._handle) != target.creation_time_100ns:
            raise GuardError("target_process_identity_changed")
        target._counts = counts
        return True


def close_target(target):
    """Idempotently release the guard handle; never terminate the process."""
    if not isinstance(target, Target):
        raise GuardError("invalid_target_object")
    with target._lock:
        if target._handle:
            if not _apis().CloseHandle(target._handle):
                raise _os_error("process_handle_close_failed")
            target._handle = 0


def enable_debug_privilege():
    """Enable SeDebugPrivilege in this process only; this does not elevate it."""
    kernel = _apis()
    advapi = ctypes.WinDLL("advapi32", use_last_error=True)

    class Luid(ctypes.Structure):
        _fields_ = [("LowPart", wintypes.DWORD), ("HighPart", wintypes.LONG)]

    class LuidAttributes(ctypes.Structure):
        _fields_ = [("Luid", Luid), ("Attributes", wintypes.DWORD)]

    class TokenPrivileges(ctypes.Structure):
        _fields_ = [("PrivilegeCount", wintypes.DWORD), ("Privileges", LuidAttributes * 1)]

    advapi.OpenProcessToken.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    advapi.OpenProcessToken.restype = wintypes.BOOL
    advapi.LookupPrivilegeValueW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, ctypes.POINTER(Luid)]
    advapi.LookupPrivilegeValueW.restype = wintypes.BOOL
    advapi.AdjustTokenPrivileges.argtypes = [wintypes.HANDLE, wintypes.BOOL,
        ctypes.POINTER(TokenPrivileges), wintypes.DWORD, ctypes.c_void_p, ctypes.c_void_p]
    advapi.AdjustTokenPrivileges.restype = wintypes.BOOL
    token = wintypes.HANDLE()
    if not advapi.OpenProcessToken(kernel.GetCurrentProcess(), 0x20 | 0x08, ctypes.byref(token)):
        raise _os_error("debug_token_open_failed")
    try:
        privileges = TokenPrivileges()
        privileges.PrivilegeCount = 1
        if not advapi.LookupPrivilegeValueW(None, "SeDebugPrivilege", ctypes.byref(privileges.Privileges[0].Luid)):
            raise _os_error("debug_privilege_lookup_failed")
        privileges.Privileges[0].Attributes = 2
        ctypes.set_last_error(0)
        if not advapi.AdjustTokenPrivileges(token, False, ctypes.byref(privileges), 0, None, None):
            raise _os_error("debug_privilege_enable_failed")
        code = ctypes.get_last_error()
        if code:
            raise GuardError("debug_privilege_not_assigned", code)
        return True
    finally:
        kernel.CloseHandle(token)


def remote_address(device_id):
    """Parse the peer address, never the adapter address, from a BLE AEP ID."""
    if not isinstance(device_id, str) or len(device_id) > 1024:
        raise GuardError("invalid_device_selection")
    match = re.fullmatch(r"BluetoothLE#[^\x00\r\n]+-((?:[0-9a-f]{2}:){5}[0-9a-f]{2})",
                         device_id, re.IGNORECASE)
    if match is None:
        raise GuardError("invalid_device_selection")
    return match.group(1).replace(":", "").casefold()


def container_for_device(device_id):
    address = remote_address(device_id)
    try:
        base = _ENUM + r"\BTHLE"
        with _open(base) as root:
            names = [name for name in _names(root) if name.casefold() == "dev_" + address]
        if len(names) != 1:
            raise GuardError("selected_ble_device_not_unique")
        with _open(base + "\\" + names[0]) as device:
            instances = list(_names(device))
        if len(instances) != 1:
            raise GuardError("selected_ble_instance_not_unique")
        with _open(base + "\\" + names[0] + "\\" + instances[0]) as instance:
            value, kind = winreg.QueryValueEx(instance, "ContainerID")
        if kind != winreg.REG_SZ or not isinstance(value, str):
            raise GuardError("invalid_selected_container")
        try:
            container = uuid.UUID(value.strip("{}"))
        except ValueError:
            raise GuardError("invalid_selected_container") from None
        if container.int in (0, 1):
            raise GuardError("invalid_selected_container")
        return container
    except OSError as error:
        raise GuardError("selected_ble_registry_unavailable", getattr(error, "winerror", None)) from None


def select_for_device(device_id):
    expected = container_for_device(device_id)
    target = select_target(require_exclusive=False)
    try:
        if target._container != expected:
            raise GuardError("selected_device_container_mismatch")
        return target
    except BaseException:
        close_target(target)
        raise


class ParentGuard:
    """Keep the original parent process object alive and detect parent exit."""

    def __init__(self, pid):
        if type(pid) is not int or not 0 < pid <= 0xffffffff or pid == os.getpid():
            raise GuardError("invalid_parent_pid")
        self._handle = _apis().OpenProcess(_QUERY_AND_SYNCHRONIZE, False, pid)
        if not self._handle:
            raise _os_error("parent_open_failed")
        self.pid = pid

    def alive(self):
        if not self._handle:
            return False
        result = _apis().WaitForSingleObject(self._handle, 0)
        if result == 258:
            return True
        if result == 0:
            return False
        raise _os_error("parent_liveness_failed")

    def close(self):
        if self._handle:
            _apis().CloseHandle(self._handle)
            self._handle = 0


def verify_listener(port, parent_pid, peer_port=None):
    """Verify listener, or the accepted loopback connection, against the parent PID."""
    api = ctypes.WinDLL("iphlpapi", use_last_error=True).GetExtendedTcpTable
    api.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.DWORD), wintypes.BOOL,
                    wintypes.ULONG, ctypes.c_int, wintypes.ULONG]
    api.restype = wintypes.DWORD
    size = wintypes.DWORD(0)
    table = 3 if peer_port is None else 5
    result = api(None, ctypes.byref(size), False, 2, table, 0)
    if result not in (0, 122) or size.value > 16 * 1024 * 1024:
        raise GuardError("listener_query_failed")
    for _ in range(3):
        data = ctypes.create_string_buffer(max(size.value, 4))
        result = api(data, ctypes.byref(size), False, 2, table, 0)
        if result != 122:
            break
        if size.value > 16 * 1024 * 1024:
            raise GuardError("listener_query_failed")
    if result != 0:
        raise GuardError("listener_query_failed", result)
    raw = data.raw
    count = struct.unpack_from("<I", raw)[0]
    if 4 + count * 24 > len(raw):
        raise GuardError("listener_query_failed")
    owners = []
    for index in range(count):
        state, local, encoded_port, remote, remote_port, pid = struct.unpack_from("<6I", raw, 4 + index * 24)
        expected_state = 2 if peer_port is None else 5
        if state == expected_state and socket.ntohs(encoded_port & 0xffff) == port:
            if socket.inet_ntoa(struct.pack("<I", local)) == "127.0.0.1":
                if peer_port is None or (socket.ntohs(remote_port & 0xffff) == peer_port and
                    socket.inet_ntoa(struct.pack("<I", remote)) == "127.0.0.1"):
                    owners.append(pid)
    if owners != [parent_pid]:
        raise GuardError("listener_owner_mismatch")
