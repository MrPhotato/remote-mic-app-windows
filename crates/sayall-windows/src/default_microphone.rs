//! Optional, user-authorized default Capture ownership. Render defaults are never written.
//! PolicyConfig is confined to the single SetDefaultEndpoint operation below.
use crate::PlatformError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneSelectionSnapshot {
    pub target_microphone_id: Option<String>,
    pub target_microphone_name: Option<String>,
    pub current_microphone_id: Option<String>,
    pub current_microphone_name: Option<String>,
    pub phase: String,
    pub last_error: Option<String>,
    pub restore_pending: bool,
    /// Ownership evidence survives a simultaneous error/failed UI phase.
    #[serde(default)]
    pub external_change_observed: bool,
}

impl Default for MicrophoneSelectionSnapshot {
    fn default() -> Self {
        Self {
            target_microphone_id: None,
            target_microphone_name: None,
            current_microphone_id: None,
            current_microphone_name: None,
            phase: "unconfigured".into(),
            last_error: None,
            restore_pending: false,
            external_change_observed: false,
        }
    }
}

#[cfg(windows)]
pub use native::DefaultMicrophoneRuntime;

#[cfg(not(windows))]
pub struct DefaultMicrophoneRuntime;
#[cfg(not(windows))]
impl DefaultMicrophoneRuntime {
    pub fn new(_: PathBuf) -> Result<Self, PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
    pub fn select(&self, _: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
    pub fn use_now(&self, _: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
    pub fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
    pub fn restore(&self) -> Result<(), PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
    pub fn release_session(&self) -> Result<(), PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Write;
    use std::sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    };
    use std::time::{Duration, Instant};
    use windows::core::{IUnknown, IUnknown_Vtbl, Interface, GUID, HRESULT, PCWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eCapture, ERole, IMMDeviceEnumerator, IMMEndpoint, IMMNotificationClient,
        MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToStringAlloc,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

    const WAIT: Duration = Duration::from_secs(10);
    fn note(stage: &str, result: &str) {
        crate::gatt_note(format!("default_microphone stage={stage} result={result}"));
    }
    fn role_note(stage: &str, role: usize, result: &str) {
        let role = ["console", "multimedia", "communications"]
            .get(role)
            .copied()
            .unwrap_or("invalid");
        crate::gatt_note(format!(
            "default_microphone stage={stage} role={role} result={result}"
        ));
    }
    fn elapsed_note(stage: &str, passed: bool, started: Instant) {
        let result = if passed { "passed" } else { "failed" };
        crate::gatt_note(format!(
            "default_microphone stage={stage} result={result} elapsed_ms={}",
            started.elapsed().as_millis()
        ));
    }
    fn failure(stage: &'static str, error: windows::core::Error) -> PlatformError {
        crate::gatt_note(format!(
            "default_microphone stage={stage} result=failed hresult=0x{:08X}",
            error.code().0 as u32
        ));
        PlatformError::Audio(format!(
            "默认麦克风操作失败（{stage}，0x{:08X}）",
            error.code().0 as u32
        ))
    }
    fn local_failure(message: &str) -> PlatformError {
        PlatformError::Audio(message.to_owned())
    }

    // ABI reference: SoundSwitch 6ce0c4d80849fe4654ec36a0db0556eee8555f26,
    // Interop/Interface/Policy/IPolicyConfig.cs. This ABI-only declaration and
    // ownership implementation were independently written. Ten preceding methods are
    // deliberately unavailable here. No registry, format, visibility or per-app API.
    #[repr(transparent)]
    #[derive(Clone)]
    struct PolicyConfig(IUnknown);
    #[repr(C)]
    struct PolicyVtable {
        base: IUnknown_Vtbl,
        unused: [usize; 10],
        set_default: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, ERole) -> HRESULT,
    }
    unsafe impl Interface for PolicyConfig {
        type Vtable = PolicyVtable;
        const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
    }

    trait AudioAccess {
        fn current(&self, role: usize) -> Result<Option<String>, PlatformError>;
        fn capture_name(&self, id: &str) -> Result<String, PlatformError>;
        fn set_capture(&mut self, id: &str, role: usize) -> Result<(), PlatformError>;
    }
    struct WindowsAudio {
        enumerator: IMMDeviceEnumerator,
        policy: PolicyConfig,
    }
    impl WindowsAudio {
        fn new() -> Result<Self, PlatformError> {
            let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|e| failure("enumerator", e))?;
            let policy = unsafe {
                CoCreateInstance(
                    &GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9),
                    None,
                    CLSCTX_ALL,
                )
            }
            .map_err(|e| failure("policy_interface", e))?;
            Ok(Self { enumerator, policy })
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    impl AudioAccess for WindowsAudio {
        fn current(&self, role: usize) -> Result<Option<String>, PlatformError> {
            match unsafe {
                self.enumerator
                    .GetDefaultAudioEndpoint(eCapture, ERole(role as i32))
            } {
                Ok(device) => {
                    let pointer =
                        unsafe { device.GetId() }.map_err(|e| failure("read_default_id", e))?;
                    let result = unsafe { pointer.to_string() }
                        .map_err(|e| failure("decode_default_id", e.into()));
                    unsafe {
                        CoTaskMemFree(Some(pointer.0.cast()));
                    }
                    result.map(Some)
                }
                Err(error) if error.code() == HRESULT::from_win32(1168) => Ok(None),
                Err(error) => Err(failure("read_default", error)),
            }
        }
        fn capture_name(&self, id: &str) -> Result<String, PlatformError> {
            if id.is_empty() || id.contains('\0') {
                return Err(local_failure("麦克风标识无效"));
            }
            let id = wide(id);
            let device = unsafe { self.enumerator.GetDevice(PCWSTR(id.as_ptr())) }
                .map_err(|e| failure("get_capture", e))?;
            let endpoint: IMMEndpoint =
                device.cast().map_err(|e| failure("capture_interface", e))?;
            let flow =
                unsafe { endpoint.GetDataFlow() }.map_err(|e| failure("capture_direction", e))?;
            let state = unsafe { device.GetState() }.map_err(|e| failure("capture_state", e))?;
            if flow != eCapture || state != DEVICE_STATE_ACTIVE {
                return Err(local_failure("所选设备不是当前可用的麦克风"));
            }
            let store = unsafe { device.OpenPropertyStore(STGM_READ) }
                .map_err(|e| failure("capture_properties", e))?;
            let mut value = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }
                .map_err(|e| failure("capture_name", e))?;
            let result = unsafe { PropVariantToStringAlloc(&value) }
                .map_err(|e| failure("capture_name", e))
                .and_then(|pointer| {
                    let result = unsafe { pointer.to_string() }
                        .map_err(|e| failure("capture_name", e.into()));
                    unsafe {
                        CoTaskMemFree(Some(pointer.0.cast()));
                    }
                    result
                });
            let _ = unsafe { PropVariantClear(&mut value) };
            result
        }
        fn set_capture(&mut self, id: &str, role: usize) -> Result<(), PlatformError> {
            // Validate direction/state immediately before every write, including restore.
            self.capture_name(id)?;
            if role > 2 {
                return Err(local_failure("麦克风角色无效"));
            }
            let id = wide(id);
            unsafe {
                (self.policy.vtable().set_default)(
                    self.policy.as_raw(),
                    PCWSTR(id.as_ptr()),
                    ERole(role as i32),
                )
                .ok()
            }
            .map_err(|e| failure("set_capture_default", e))
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct RoleLease {
        original: String,
        last_applied: Option<String>,
        pending: Option<String>,
        owned: bool,
        #[serde(default)]
        relinquished: bool,
    }
    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Journal {
        schema_version: u32,
        roles: Vec<RoleLease>,
    }
    impl Journal {
        fn empty() -> Self {
            Self {
                schema_version: 1,
                roles: Vec::new(),
            }
        }
        fn pending(&self) -> bool {
            self.roles.iter().any(|role| role.owned)
        }
        fn validate(&self) -> Result<(), PlatformError> {
            if self.schema_version != 1
                || (!self.roles.is_empty() && self.roles.len() != 3)
                || self.roles.iter().any(|role| {
                    role.original.is_empty()
                        || role.original.contains('\0')
                        || role
                            .last_applied
                            .iter()
                            .chain(role.pending.iter())
                            .any(|id| id.is_empty() || id.contains('\0'))
                        || (role.owned && role.last_applied.is_none() && role.pending.is_none())
                })
            {
                return Err(local_failure("麦克风恢复记录无效，尚未切换系统设备"));
            }
            Ok(())
        }
    }
    trait JournalStore {
        fn save(&mut self, journal: &Journal) -> Result<(), PlatformError>;
    }
    struct DiskJournal(PathBuf);
    impl DiskJournal {
        fn load(&self) -> Result<Journal, PlatformError> {
            match std::fs::read(&self.0) {
                Ok(bytes) => {
                    let journal: Journal = serde_json::from_slice(&bytes)
                        .map_err(|_| local_failure("无法读取麦克风恢复记录，尚未切换系统设备"))?;
                    journal.validate()?;
                    Ok(journal)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Journal::empty()),
                Err(_) => Err(local_failure("无法读取麦克风恢复记录，尚未切换系统设备")),
            }
        }
    }
    impl JournalStore for DiskJournal {
        fn save(&mut self, journal: &Journal) -> Result<(), PlatformError> {
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let parent = self.0.parent().ok_or("journal parent missing")?;
                std::fs::create_dir_all(parent)?;
                let temporary = self.0.with_extension("pending");
                let mut file = std::fs::File::create(&temporary)?;
                file.write_all(&serde_json::to_vec(journal)?)?;
                file.sync_all()?;
                drop(file);
                use windows::Win32::Storage::FileSystem::{
                    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
                };
                let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
                let to: Vec<u16> = self.0.as_os_str().encode_wide().chain(Some(0)).collect();
                unsafe {
                    MoveFileExW(
                        PCWSTR(from.as_ptr()),
                        PCWSTR(to.as_ptr()),
                        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                    )?;
                }
                Ok(())
            })();
            if result.is_err() {
                note("journal_write", "failed");
            }
            result.map_err(|_| local_failure("无法保存麦克风恢复记录，操作已停止"))
        }
    }
    use std::os::windows::ffi::OsStrExt;

    struct Owner<B: AudioAccess, S: JournalStore> {
        backend: B,
        store: S,
        journal: Journal,
        expected: [VecDeque<Option<String>>; 3],
        state: MicrophoneSelectionSnapshot,
    }
    impl<B: AudioAccess, S: JournalStore> Owner<B, S> {
        fn new(backend: B, store: S, journal: Journal) -> Self {
            Self {
                backend,
                store,
                journal,
                expected: std::array::from_fn(|_| VecDeque::new()),
                state: MicrophoneSelectionSnapshot::default(),
            }
        }
        fn record_error(&mut self, error: &PlatformError) {
            self.state.phase = "failed".into();
            self.state.last_error = Some(error.to_string());
        }
        fn defaults(&self) -> Result<[Option<String>; 3], PlatformError> {
            Ok([
                self.backend.current(0)?,
                self.backend.current(1)?,
                self.backend.current(2)?,
            ])
        }
        fn relinquish(&mut self, role: usize, reason: &str) {
            self.state.external_change_observed = true;
            // Windows can change Console and Multimedia together. Never restore
            // the other half of that pair over a user's external selection.
            let affected = if role < 2 { 0..2 } else { 2..3 };
            for affected in affected {
                self.journal.roles[affected].owned = false;
                self.journal.roles[affected].relinquished = true;
                role_note("ownership", affected, reason);
            }
            if self.state.phase != "failed" {
                self.state.phase = "external".into();
            }
        }
        fn reconcile(&mut self, current: &[Option<String>; 3]) -> Result<(), PlatformError> {
            if self.journal.roles.len() != 3 {
                return Ok(());
            }
            let mut changed = false;
            for (role, current) in current.iter().enumerate() {
                let lease = &self.journal.roles[role];
                if lease.owned
                    && lease.last_applied.is_none()
                    && lease.pending.is_some()
                    && current.as_deref() == Some(lease.original.as_str())
                {
                    // A write-ahead intent may survive a crash before that role
                    // changed. It is not evidence of an external paired change.
                    self.journal.roles[role].owned = false;
                    self.journal.roles[role].pending = None;
                    changed = true;
                    continue;
                }
                if lease.owned
                    && current != &lease.last_applied
                    && !(lease.pending.is_some() && current == &lease.pending)
                {
                    self.relinquish(role, "external_readback");
                    changed = true;
                }
            }
            if changed {
                self.store.save(&self.journal)?;
            }
            Ok(())
        }
        fn write_capture(
            &mut self,
            id: &str,
            role: usize,
            stage: &str,
        ) -> Result<(), PlatformError> {
            let before_values = self.defaults()?;
            self.reconcile(&before_values)?;
            if before_values[role].as_deref() == Some(id) {
                return Ok(());
            }
            let before = self.journal.roles.clone();
            let affected: Vec<_> = (0..3)
                .filter(|candidate| {
                    (*candidate == role || (role < 2 && *candidate < 2))
                        && before_values[*candidate].as_deref() != Some(id)
                })
                .collect();
            // Save all three baselines and intent for both potentially linked roles
            // before the system call. A crash in the call must not lose its side effect.
            for &affected in &affected {
                let previous = before_values[affected]
                    .clone()
                    .ok_or_else(|| local_failure("无法确认切换前的默认麦克风"))?;
                let lease = &mut self.journal.roles[affected];
                if !lease.owned {
                    lease.original = previous;
                }
                lease.pending = Some(id.to_owned());
                lease.owned = true;
                lease.relinquished = false;
            }
            if let Err(error) = self.store.save(&self.journal) {
                self.journal.roles = before;
                return Err(error);
            }
            let write = self.backend.set_capture(id, role);
            role_note(
                &format!("{stage}_write"),
                role,
                if write.is_ok() { "passed" } else { "failed" },
            );
            let after = match self.defaults() {
                Ok(after) => after,
                Err(error) => {
                    role_note(&format!("{stage}_readback"), role, "failed");
                    return Err(error);
                }
            };
            for &affected in &affected {
                if after[affected] == before_values[affected] {
                    self.journal.roles[affected] = before[affected].clone();
                } else if after[affected].as_deref() == Some(id) {
                    let lease = &mut self.journal.roles[affected];
                    lease.last_applied = Some(id.to_owned());
                    lease.pending = None;
                    self.expected[affected].push_back(Some(id.to_owned()));
                    if affected != role {
                        role_note(&format!("{stage}_side_effect"), affected, "linked_change");
                    }
                } else {
                    self.relinquish(affected, "external_during_write");
                }
            }
            if affected.iter().any(|&affected| {
                after[affected] != before_values[affected] && after[affected].as_deref() != Some(id)
            }) {
                self.relinquish(role, "external_during_write");
            }
            self.reconcile(&after)?;
            self.store.save(&self.journal)?;
            if after[role].as_deref() == Some(id) {
                role_note(&format!("{stage}_readback"), role, "passed");
                Ok(())
            } else {
                role_note(&format!("{stage}_readback"), role, "failed");
                Err(write
                    .err()
                    .unwrap_or_else(|| local_failure("默认麦克风切换未生效")))
            }
        }
        fn protect_paired_role(
            &self,
            role: usize,
            target: &str,
            current: &[Option<String>; 3],
        ) -> Result<(), PlatformError> {
            if role < 2 {
                let peer = 1 - role;
                if !self.journal.roles[peer].owned && current[peer].as_deref() != Some(target) {
                    role_note("restore_decision", role, "linked_peer_not_owned");
                    return Err(local_failure(
                        "Windows 联动麦克风角色，已保留恢复记录以避免覆盖其它选择",
                    ));
                }
            }
            Ok(())
        }
        fn select(
            &mut self,
            id: String,
            cancelled: &impl Fn() -> bool,
        ) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            let started = Instant::now();
            let result = self.select_inner(id, cancelled);
            if let Err(error) = &result {
                self.record_error(error);
            }
            elapsed_note("select", result.is_ok(), started);
            result
        }
        fn select_inner(
            &mut self,
            id: String,
            cancelled: &impl Fn() -> bool,
        ) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            if cancelled() {
                return Err(local_failure("麦克风切换已取消"));
            }
            let name = self.backend.capture_name(&id)?;
            // A recovered journal must finish before a new baseline can be taken.
            if self.state.target_microphone_id.is_none() && self.journal.pending() {
                self.restore()?;
                if self.journal.pending() {
                    return Err(local_failure("上次麦克风恢复尚未完成"));
                }
            }
            if self.journal.roles.is_empty() {
                let mut roles = Vec::new();
                for role in 0..3 {
                    let original = self
                        .backend
                        .current(role)?
                        .ok_or_else(|| local_failure("无法确认原默认麦克风，尚未进行切换"))?;
                    roles.push(RoleLease {
                        original,
                        last_applied: None,
                        pending: None,
                        owned: false,
                        relinquished: false,
                    });
                }
                self.journal.roles = roles;
            }
            self.state.target_microphone_id = Some(id.clone());
            self.state.target_microphone_name = Some(name);
            let before_values = self.defaults()?;
            self.reconcile(&before_values)?;
            let before = self.journal.roles.clone();
            let mut result = Ok(());
            for role in 0..3 {
                if cancelled() {
                    result = Err(local_failure("麦克风切换已取消"));
                    break;
                }
                result = self.write_capture(&id, role, "select");
                if result.is_err() {
                    break;
                }
            }
            if cancelled() && result.is_ok() {
                result = Err(local_failure("麦克风切换已取消"));
            }
            if let Err(error) = result {
                self.rollback_selection(&before, &before_values);
                return Err(error);
            }
            self.state.phase = "ready".into();
            self.state.external_change_observed = false;
            self.state.last_error = None;
            self.snapshot()
        }
        fn use_now(
            &mut self,
            id: String,
            cancelled: &impl Fn() -> bool,
        ) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            let started = Instant::now();
            let previous_target = (
                self.state.target_microphone_id.clone(),
                self.state.target_microphone_name.clone(),
            );
            let result = (|| {
                // Reuse the checked three-role write, write-ahead journal and
                // rollback. A failed manual choice must retain prior recovery.
                let selected = self.select_inner(id.clone(), cancelled)?;
                if selected.phase != "ready"
                    || selected.current_microphone_id.as_deref() != Some(id.as_str())
                {
                    return Err(local_failure("默认麦克风未确认，已保留恢复记录"));
                }
                if cancelled() {
                    return Err(local_failure("麦克风切换已取消，已保留恢复记录"));
                }
                // This explicit user choice becomes the system preference.
                // Persist the relinquishment before reporting success so exit
                // or the next process cannot undo a completed manual choice.
                let committed = Journal::empty();
                self.store.save(&committed)?;
                self.journal = committed;
                self.state.target_microphone_id = None;
                self.state.target_microphone_name = None;
                self.state.phase = "external".into();
                self.state.external_change_observed = true;
                self.state.restore_pending = false;
                self.state.last_error = None;
                note("manual_choice", "committed_without_restore");
                self.snapshot()
            })();
            if let Err(error) = &result {
                self.state.target_microphone_id = previous_target.0;
                self.state.target_microphone_name = previous_target.1;
                self.record_error(error);
            }
            elapsed_note("use_now", result.is_ok(), started);
            result
        }
        fn rollback_selection(&mut self, before: &[RoleLease], previous: &[Option<String>; 3]) {
            for (role, previous) in previous.iter().enumerate() {
                let result = (|| -> Result<(), PlatformError> {
                    let current = self.defaults()?;
                    self.reconcile(&current)?;
                    if current[role] == *previous || !self.journal.roles[role].owned {
                        return Ok(());
                    }
                    let target = previous
                        .as_deref()
                        .ok_or_else(|| local_failure("回滚前麦克风不可确认"))?;
                    self.protect_paired_role(role, target, &current)?;
                    self.write_capture(target, role, "rollback")
                })();
                if result.is_err() {
                    role_note("rollback", role, "pending");
                }
            }
            // A later role's write can change an earlier one. Restore the old lease
            // only after the whole rollback has independently reached its old value.
            if let Ok(current) = self.defaults() {
                let _ = self.reconcile(&current);
                for role in 0..3 {
                    if current[role] == previous[role] && !self.journal.roles[role].relinquished {
                        self.journal.roles[role] = before[role].clone();
                    }
                }
                let _ = self.store.save(&self.journal);
            }
        }
        fn observe(&mut self, role: usize, event: Option<String>) -> Result<(), PlatformError> {
            if role > 2 {
                return Ok(());
            }
            let expected = self.expected[role].iter().position(|value| *value == event);
            if let Some(index) = expected {
                self.expected[role].remove(index);
            }
            if self.journal.roles.len() != 3
                || (!self.journal.roles[role].owned
                    && (role == 2 || !self.journal.roles[1 - role].owned))
            {
                return Ok(());
            }
            let current = self.backend.current(role)?;
            let lease = &self.journal.roles[role];
            let matches_owned = current == lease.last_applied
                || (lease.pending.is_some() && current == lease.pending);
            // A delayed own A notification while current/default is already our B
            // is benign. An unexpected A followed by B still relinquishes ownership.
            if expected.is_none() || !matches_owned {
                self.relinquish(role, "external_change");
                self.store.save(&self.journal)?;
            }
            Ok(())
        }
        fn uncertainty(&mut self) -> Result<(), PlatformError> {
            self.state.external_change_observed = true;
            for lease in &mut self.journal.roles {
                lease.owned = false;
                lease.relinquished = true;
            }
            self.state.phase = "external".into();
            self.store.save(&self.journal)?;
            note("ownership", "notification_overflow");
            Ok(())
        }
        fn snapshot(&mut self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            let defaults = self.defaults()?;
            let current = defaults[0].clone();
            self.state.current_microphone_name = current
                .as_deref()
                .map(|id| self.backend.capture_name(id))
                .transpose()?;
            self.state.current_microphone_id = current;
            // Poll every role even if no role is owned. Ready always means all three
            // Capture defaults match; a notification never triggers re-selection.
            self.reconcile(&defaults)?;
            let all_match = self.state.target_microphone_id.is_some()
                && defaults
                    .iter()
                    .all(|current| current == &self.state.target_microphone_id);
            if self.state.phase != "failed"
                && self.state.target_microphone_id.is_some()
                && !all_match
            {
                self.state.phase = "external".into();
            }
            self.state.restore_pending = self.journal.pending();
            Ok(self.state.clone())
        }
        fn restore_role(&mut self, role: usize) -> Result<(), PlatformError> {
            let current = self.defaults()?;
            self.reconcile(&current)?;
            let lease = self.journal.roles[role].clone();
            if !lease.owned {
                role_note("restore_decision", role, "not_owned");
                return Ok(());
            }
            if current[role].as_deref() == Some(lease.original.as_str()) {
                return Ok(());
            }
            self.protect_paired_role(role, &lease.original, &current)?;
            self.write_capture(&lease.original, role, "restore")
        }
        fn restore(&mut self) -> Result<(), PlatformError> {
            let started = Instant::now();
            let result = self.restore_inner();
            if let Err(error) = &result {
                self.record_error(error);
            }
            elapsed_note("restore", result.is_ok(), started);
            result
        }
        fn release_session(&mut self) -> Result<(), PlatformError> {
            let result = self.restore();
            // A new connection takes a fresh baseline. Even after failure, the
            // next selection must finish the retained journal before switching.
            self.state.target_microphone_id = None;
            self.state.target_microphone_name = None;
            if result.is_ok() {
                self.state.phase = if self.state.external_change_observed {
                    "external"
                } else {
                    "unconfigured"
                }
                .into();
                self.state.last_error = None;
            }
            note(
                "release_session",
                if result.is_ok() { "passed" } else { "failed" },
            );
            result
        }
        fn restore_inner(&mut self) -> Result<(), PlatformError> {
            let mut failure = None;
            for role in 0..self.journal.roles.len() {
                if let Err(error) = self.restore_role(role) {
                    failure.get_or_insert(error);
                }
            }
            if !self.journal.roles.is_empty() {
                let current = self.defaults()?;
                self.reconcile(&current)?;
                for (role, current) in current.iter().enumerate() {
                    let lease = &mut self.journal.roles[role];
                    if !lease.owned {
                        continue;
                    }
                    if current.as_deref() == Some(lease.original.as_str()) {
                        lease.owned = false;
                        lease.pending = None;
                        role_note("restore_final_readback", role, "passed");
                    } else {
                        failure.get_or_insert_with(|| {
                            local_failure("Windows 麦克风角色联动，部分原设置尚未恢复")
                        });
                        role_note("restore_final_readback", role, "pending");
                    }
                }
                self.store.save(&self.journal)?;
            }
            self.state.restore_pending = self.journal.pending();
            if let Some(error) = failure {
                return Err(error);
            }
            if !self.journal.pending() {
                self.journal = Journal::empty();
                self.store.save(&self.journal)?;
            }
            Ok(())
        }
    }

    enum Request {
        Select(
            String,
            u64,
            Arc<AtomicBool>,
            SyncSender<Result<MicrophoneSelectionSnapshot, PlatformError>>,
        ),
        UseNow(
            String,
            u64,
            Arc<AtomicBool>,
            SyncSender<Result<MicrophoneSelectionSnapshot, PlatformError>>,
        ),
        Snapshot(SyncSender<Result<MicrophoneSelectionSnapshot, PlatformError>>),
        Restore(SyncSender<Result<(), PlatformError>>),
        ReleaseSession(SyncSender<Result<(), PlatformError>>),
    }
    pub struct DefaultMicrophoneRuntime {
        sender: SyncSender<Request>,
        closing: Arc<AtomicBool>,
        session_epoch: Arc<AtomicU64>,
    }
    impl DefaultMicrophoneRuntime {
        pub fn new(journal_path: PathBuf) -> Result<Self, PlatformError> {
            let (sender, receiver) = mpsc::sync_channel(32);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let closing = Arc::new(AtomicBool::new(false));
            let worker_closing = closing.clone();
            let session_epoch = Arc::new(AtomicU64::new(0));
            let worker_epoch = session_epoch.clone();
            std::thread::Builder::new()
                .name("sayall-default-microphone".into())
                .spawn(move || {
                    if let Err(error) = wasapi::initialize_mta().ok() {
                        let _ = ready_tx.send(Err(failure("com_initialize", error)));
                        return;
                    }
                    struct Apartment;
                    impl Drop for Apartment {
                        fn drop(&mut self) {
                            wasapi::deinitialize();
                        }
                    }
                    let _apartment = Apartment;
                    let setup = (|| {
                        let backend = WindowsAudio::new()?;
                        let store = DiskJournal(journal_path);
                        let journal = store.load()?;
                        let (events_tx, events_rx) = mpsc::sync_channel(128);
                        let overflow = Arc::new(AtomicBool::new(false));
                        let callback: IMMNotificationClient = Notifications {
                            sender: events_tx,
                            overflow: overflow.clone(),
                        }
                        .into();
                        unsafe {
                            backend
                                .enumerator
                                .RegisterEndpointNotificationCallback(&callback)
                        }
                        .map_err(|e| failure("notification_register", e))?;
                        let mut owner = Owner::new(backend, store, journal);
                        owner.state.restore_pending = owner.journal.pending();
                        Ok::<_, PlatformError>((owner, callback, events_rx, overflow))
                    })();
                    let (mut owner, callback, events, overflow) = match setup {
                        Ok(value) => value,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    };
                    let _ = ready_tx.send(Ok(()));
                    note("worker", "ready");
                    loop {
                        if overflow.swap(false, Ordering::AcqRel) {
                            if let Err(error) = owner.uncertainty() {
                                owner.record_error(&error);
                            }
                        }
                        while let Ok((role, id)) = events.try_recv() {
                            if let Err(error) = owner.observe(role, id) {
                                owner.record_error(&error);
                            }
                        }
                        let request = match receiver.recv_timeout(Duration::from_millis(100)) {
                            Ok(value) => value,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(_) => break,
                        };
                        // Drain callbacks that arrived while waiting for a request.
                        while let Ok((role, id)) = events.try_recv() {
                            if let Err(error) = owner.observe(role, id) {
                                owner.record_error(&error);
                            }
                        }
                        match request {
                            Request::Select(id, epoch, cancelled, reply) => {
                                let result = owner.select(id, &|| {
                                    worker_closing.load(Ordering::Acquire)
                                        || worker_epoch.load(Ordering::Acquire) != epoch
                                        || cancelled.load(Ordering::Acquire)
                                });
                                let _ = reply.send(result);
                            }
                            Request::UseNow(id, epoch, cancelled, reply) => {
                                let result = owner.use_now(id, &|| {
                                    worker_closing.load(Ordering::Acquire)
                                        || worker_epoch.load(Ordering::Acquire) != epoch
                                        || cancelled.load(Ordering::Acquire)
                                });
                                let _ = reply.send(result);
                            }
                            Request::Snapshot(reply) => {
                                let result = owner.snapshot();
                                if let Err(error) = &result {
                                    owner.record_error(error);
                                }
                                let _ = reply.send(result);
                            }
                            Request::Restore(reply) => {
                                let _ = reply.send(owner.restore());
                            }
                            Request::ReleaseSession(reply) => {
                                let _ = reply.send(owner.release_session());
                            }
                        }
                    }
                    let _ = unsafe {
                        owner
                            .backend
                            .enumerator
                            .UnregisterEndpointNotificationCallback(&callback)
                    };
                    note("worker", "stopped");
                })
                .map_err(|_| local_failure("无法启动麦克风切换线程"))?;
            ready_rx
                .recv_timeout(WAIT)
                .map_err(|_| local_failure("麦克风切换初始化超时"))??;
            Ok(Self {
                sender,
                closing,
                session_epoch,
            })
        }
        pub fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            self.request_selection(id, false)
        }
        pub fn use_now(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            self.request_selection(id, true)
        }
        fn request_selection(
            &self,
            id: String,
            manual: bool,
        ) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            if self.closing.load(Ordering::Acquire) {
                return Err(local_failure("应用正在退出，麦克风切换已取消"));
            }
            let (reply, receive) = mpsc::sync_channel(1);
            let cancelled = Arc::new(AtomicBool::new(false));
            let request = if manual {
                Request::UseNow(
                    id,
                    self.session_epoch.load(Ordering::Acquire),
                    cancelled.clone(),
                    reply,
                )
            } else {
                Request::Select(
                    id,
                    self.session_epoch.load(Ordering::Acquire),
                    cancelled.clone(),
                    reply,
                )
            };
            self.sender
                .try_send(request)
                .map_err(|_| local_failure("麦克风操作正忙，请稍后重试"))?;
            match receive.recv_timeout(WAIT) {
                Ok(result) => result,
                Err(_) => {
                    cancelled.store(true, Ordering::Release);
                    note("select", "timed_out_cancel_requested");
                    Err(local_failure("麦克风切换超时，正在核对恢复状态"))
                }
            }
        }
        pub fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            let (reply, receive) = mpsc::sync_channel(1);
            self.sender
                .try_send(Request::Snapshot(reply))
                .map_err(|_| local_failure("麦克风操作正忙"))?;
            receive
                .recv_timeout(WAIT)
                .map_err(|_| local_failure("读取默认麦克风超时"))?
        }
        pub fn restore(&self) -> Result<(), PlatformError> {
            self.closing.store(true, Ordering::Release);
            let (reply, receive) = mpsc::sync_channel(1);
            self.sender
                .try_send(Request::Restore(reply))
                .map_err(|_| local_failure("麦克风恢复请求尚未进入队列"))?;
            receive
                .recv_timeout(WAIT)
                .map_err(|_| local_failure("麦克风恢复超时，已保留恢复记录"))?
        }
        /// End the current remote-connection lease without closing the worker.
        /// Invalidate queued/in-flight selections before requesting restoration.
        pub fn release_session(&self) -> Result<(), PlatformError> {
            self.session_epoch.fetch_add(1, Ordering::AcqRel);
            let (reply, receive) = mpsc::sync_channel(1);
            self.sender
                .try_send(Request::ReleaseSession(reply))
                .map_err(|_| local_failure("麦克风恢复请求尚未进入队列"))?;
            receive
                .recv_timeout(WAIT)
                .map_err(|_| local_failure("麦克风恢复超时，已保留恢复记录"))?
        }
    }
    #[windows::core::implement(IMMNotificationClient)]
    struct Notifications {
        sender: SyncSender<(usize, Option<String>)>,
        overflow: Arc<AtomicBool>,
    }
    #[allow(non_snake_case)]
    impl windows::Win32::Media::Audio::IMMNotificationClient_Impl for Notifications_Impl {
        fn OnDeviceStateChanged(
            &self,
            _: &PCWSTR,
            _: windows::Win32::Media::Audio::DEVICE_STATE,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnPropertyValueChanged(
            &self,
            _: &PCWSTR,
            _: &windows::Win32::Foundation::PROPERTYKEY,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnDefaultDeviceChanged(
            &self,
            flow: windows::Win32::Media::Audio::EDataFlow,
            role: ERole,
            id: &PCWSTR,
        ) -> windows::core::Result<()> {
            if flow != eCapture || !(0..3).contains(&role.0) {
                return Ok(());
            }
            let value = if id.is_null() {
                None
            } else {
                match unsafe { id.to_string() } {
                    Ok(id) => Some(id),
                    Err(_) => {
                        self.overflow.store(true, Ordering::Release);
                        return Ok(());
                    }
                }
            };
            if self.sender.try_send((role.0 as usize, value)).is_err() {
                self.overflow.store(true, Ordering::Release);
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::RefCell;
        use std::collections::BTreeSet;
        struct Fake {
            current: RefCell<[Option<String>; 3]>,
            active: BTreeSet<String>,
            writes: Vec<(usize, String)>,
            fail: Option<(usize, String)>,
            linked_console_multimedia: bool,
        }
        impl AudioAccess for Fake {
            fn current(&self, role: usize) -> Result<Option<String>, PlatformError> {
                Ok(self.current.borrow()[role].clone())
            }
            fn capture_name(&self, id: &str) -> Result<String, PlatformError> {
                if self.active.contains(id) {
                    Ok(id.to_owned())
                } else {
                    Err(local_failure("capture unavailable"))
                }
            }
            fn set_capture(&mut self, id: &str, role: usize) -> Result<(), PlatformError> {
                self.capture_name(id)?;
                self.writes.push((role, id.to_owned()));
                if self
                    .fail
                    .as_ref()
                    .is_some_and(|(r, value)| *r == role && value == id)
                {
                    return Err(local_failure("set failed"));
                }
                self.current.borrow_mut()[role] = Some(id.to_owned());
                if self.linked_console_multimedia && role < 2 {
                    self.current.borrow_mut()[1 - role] = Some(id.to_owned());
                }
                Ok(())
            }
        }
        #[derive(Default)]
        struct Store {
            saved: Vec<Journal>,
            fail_at: Option<usize>,
        }
        impl JournalStore for Store {
            fn save(&mut self, journal: &Journal) -> Result<(), PlatformError> {
                if self.fail_at == Some(self.saved.len()) {
                    return Err(local_failure("journal failed"));
                }
                self.saved.push(journal.clone());
                Ok(())
            }
        }
        fn owner() -> Owner<Fake, Store> {
            Owner::new(
                Fake {
                    current: RefCell::new([Some("A".into()), Some("A2".into()), Some("A3".into())]),
                    active: ["A", "A2", "A3", "B", "C", "D"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    writes: Vec::new(),
                    fail: None,
                    linked_console_multimedia: false,
                },
                Store::default(),
                Journal::empty(),
            )
        }
        fn values(owner: &Owner<Fake, Store>) -> [Option<String>; 3] {
            owner.backend.current.borrow().clone()
        }
        fn all(value: &str) -> [Option<String>; 3] {
            std::array::from_fn(|_| Some(value.to_owned()))
        }
        fn ack(owner: &mut Owner<Fake, Store>, id: &str) {
            for role in 0..3 {
                owner.observe(role, Some(id.into())).unwrap();
            }
        }

        #[test]
        fn distinct_roles_round_trip_and_restore_is_idempotent() {
            let mut owner = owner();
            let original = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            assert_eq!(values(&owner), all("B"));
            assert!(owner.snapshot().unwrap().restore_pending);
            assert!(owner.store.saved[0].roles[0].pending.as_deref() == Some("B"));
            owner.restore().unwrap();
            assert_eq!(values(&owner), original);
            let writes = owner.backend.writes.len();
            owner.restore().unwrap();
            assert_eq!(writes, owner.backend.writes.len());
            assert!(!owner.journal.pending());
        }
        #[test]
        fn repeated_own_selection_preserves_first_baseline() {
            let mut owner = owner();
            let original = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            owner.select("C".into(), &|| false).unwrap();
            // Own B notifications may arrive after our newer C writes.
            ack(&mut owner, "B");
            ack(&mut owner, "C");
            assert!(owner.journal.roles.iter().all(|role| role.owned));
            owner.restore().unwrap();
            assert_eq!(values(&owner), original);
        }
        #[test]
        fn manual_choice_survives_disconnect_exit_and_the_next_remote_lease() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            let selected = owner.use_now("C".into(), &|| false).unwrap();
            ack(&mut owner, "C");
            assert_eq!(selected.current_microphone_id.as_deref(), Some("C"));
            assert!(selected.target_microphone_id.is_none());
            assert!(selected.external_change_observed);
            assert!(!selected.restore_pending);
            assert!(!owner.store.saved.last().unwrap().pending());
            owner.release_session().unwrap();
            owner.restore().unwrap();
            assert_eq!(values(&owner), all("C"));
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.release_session().unwrap();
            assert_eq!(values(&owner), all("C"));
        }
        #[test]
        fn manual_choice_failure_rolls_back_and_retains_remote_recovery() {
            let mut owner = owner();
            let baseline = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.fail = Some((2, "C".into()));
            assert!(owner.use_now("C".into(), &|| false).is_err());
            assert_eq!(values(&owner), all("B"));
            assert_eq!(owner.state.target_microphone_id.as_deref(), Some("B"));
            owner.restore().unwrap();
            assert_eq!(values(&owner), baseline);
        }
        #[test]
        fn manual_commit_persistence_failure_keeps_journal_and_does_not_claim_success() {
            let mut owner = owner();
            let baseline = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.store.fail_at = Some(owner.store.saved.len());
            assert!(owner.use_now("B".into(), &|| false).is_err());
            assert_eq!(owner.state.phase, "failed");
            assert!(owner.journal.pending());
            owner.store.fail_at = None;
            owner.restore().unwrap();
            assert_eq!(values(&owner), baseline);
        }
        #[test]
        fn cancelled_manual_choice_never_writes_or_discards_existing_lease() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            let writes = owner.backend.writes.len();
            assert!(owner.use_now("C".into(), &|| true).is_err());
            assert_eq!(owner.backend.writes.len(), writes);
            assert_eq!(values(&owner), all("B"));
            assert!(owner.journal.pending());
        }
        #[test]
        fn connection_release_can_repeat_and_next_connection_uses_fresh_baseline() {
            let mut owner = owner();
            let original = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.release_session().unwrap();
            assert_eq!(values(&owner), original);
            assert!(owner.state.target_microphone_id.is_none());
            let writes = owner.backend.writes.len();
            owner.release_session().unwrap();
            assert_eq!(owner.backend.writes.len(), writes);
            *owner.backend.current.borrow_mut() = all("D");
            owner.select("C".into(), &|| false).unwrap();
            ack(&mut owner, "C");
            owner.release_session().unwrap();
            assert_eq!(values(&owner), all("D"));
        }
        #[test]
        fn failed_connection_release_retains_recovery_before_new_selection() {
            let mut owner = linked_owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.active.remove("A");
            assert!(owner.release_session().is_err());
            assert!(owner.state.target_microphone_id.is_none());
            assert!(owner.journal.pending());
            assert!(owner.select("C".into(), &|| false).is_err());
            assert_eq!(values(&owner)[0].as_deref(), Some("B"));
            owner.backend.active.insert("A".into());
            owner.release_session().unwrap();
            owner.select("C".into(), &|| false).unwrap();
            ack(&mut owner, "C");
            owner.release_session().unwrap();
            assert_eq!(values(&owner)[0].as_deref(), Some("A"));
        }
        #[test]
        fn connection_release_preserves_external_aba_and_signals_no_auto_reclaim() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.observe(0, Some("C".into())).unwrap();
            owner.observe(0, Some("B".into())).unwrap();
            owner.release_session().unwrap();
            assert_eq!(values(&owner)[0].as_deref(), Some("B"));
            assert_eq!(owner.state.phase, "external");
        }
        #[test]
        fn external_choice_survives_failed_phase_and_connection_release() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.record_error(&local_failure("transient read failure"));
            *owner.backend.current.borrow_mut() = all("C");
            ack(&mut owner, "C");
            assert_eq!(owner.state.phase, "failed");
            assert!(owner.snapshot().unwrap().external_change_observed);
            owner.release_session().unwrap();
            assert_eq!(owner.state.phase, "external");
            assert!(owner.state.external_change_observed);
            assert_eq!(values(&owner), all("C"));
        }
        #[test]
        fn connection_epoch_change_cancels_inflight_selection_and_restores() {
            let mut owner = linked_owner();
            let original = values(&owner);
            let checks = std::cell::Cell::new(0);
            assert!(owner
                .select("B".into(), &|| {
                    checks.set(checks.get() + 1);
                    checks.get() >= 3
                })
                .is_err());
            owner.release_session().unwrap();
            assert_eq!(values(&owner), original);
            assert!(!owner.journal.pending());
        }
        #[test]
        fn partial_second_selection_rolls_back_to_previous_selection_not_baseline() {
            let mut owner = owner();
            let original = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.fail = Some((1, "C".into()));
            assert!(owner.select("C".into(), &|| false).is_err());
            assert_eq!(values(&owner), all("B"));
            assert!(owner
                .journal
                .roles
                .iter()
                .all(|role| role.last_applied.as_deref() == Some("B")));
            owner.restore().unwrap();
            assert_eq!(values(&owner), original);
        }
        #[test]
        fn external_aba_relinquishes_even_when_current_matches_our_target() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            // The two external notifications arrive after the user returned to B.
            owner.observe(0, Some("C".into())).unwrap();
            owner.observe(0, Some("B".into())).unwrap();
            assert!(!owner.journal.roles[0].owned);
            owner.restore().unwrap();
            assert_eq!(values(&owner)[0].as_deref(), Some("B"));
        }
        #[test]
        fn explicit_reselection_after_external_change_uses_new_segment_baseline() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.current.borrow_mut()[0] = Some("C".into());
            owner.observe(0, Some("C".into())).unwrap();
            owner.select("D".into(), &|| false).unwrap();
            ack(&mut owner, "D");
            assert_eq!(owner.journal.roles[0].original, "C");
            assert_eq!(owner.journal.roles[1].original, "B");
            owner.restore().unwrap();
            assert_eq!(
                values(&owner),
                [Some("C".into()), Some("B".into()), Some("A3".into())]
            );
        }
        #[test]
        fn delayed_external_notification_is_detected_before_reselection() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.current.borrow_mut()[1] = Some("C".into());
            // No observe call: the external notification has not arrived yet.
            owner.select("D".into(), &|| false).unwrap();
            ack(&mut owner, "D");
            assert_eq!(owner.journal.roles[1].original, "C");
            owner.restore().unwrap();
            assert_eq!(
                values(&owner),
                [Some("B".into()), Some("C".into()), Some("A3".into())]
            );
        }
        #[test]
        fn ready_requires_all_three_roles_even_when_console_is_unchanged() {
            let mut owner = owner();
            *owner.backend.current.borrow_mut() = all("B");
            owner.select("B".into(), &|| false).unwrap();
            owner.backend.current.borrow_mut()[2] = Some("C".into());
            let state = owner.snapshot().unwrap();
            assert_eq!(state.current_microphone_id.as_deref(), Some("B"));
            assert_eq!(state.phase, "external");
            assert!(!state.restore_pending);
        }
        #[test]
        fn no_op_never_acquires_restore_ownership_and_external_phase_still_changes() {
            let mut owner = owner();
            *owner.backend.current.borrow_mut() = all("B");
            owner.select("B".into(), &|| false).unwrap();
            assert!(owner.backend.writes.is_empty());
            assert!(!owner.journal.pending());
            owner.backend.current.borrow_mut()[0] = Some("C".into());
            owner.observe(0, Some("C".into())).unwrap();
            assert_eq!(owner.snapshot().unwrap().phase, "external");
            owner.restore().unwrap();
            assert_eq!(values(&owner)[0].as_deref(), Some("C"));
        }
        #[test]
        fn same_value_after_external_change_does_not_reclaim_ownership() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.observe(0, Some("C".into())).unwrap();
            owner.observe(0, Some("B".into())).unwrap();
            let writes = owner.backend.writes.len();
            owner.select("B".into(), &|| false).unwrap();
            assert_eq!(writes, owner.backend.writes.len());
            assert!(!owner.journal.roles[0].owned);
        }
        #[test]
        fn write_ahead_failure_prevents_all_system_writes() {
            let mut owner = owner();
            owner.store.fail_at = Some(0);
            assert!(owner.select("B".into(), &|| false).is_err());
            assert!(owner.backend.writes.is_empty());
        }
        #[test]
        fn missing_original_keeps_pending_without_guessing_another_device() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            owner.backend.active.remove("A");
            assert!(owner.restore().is_err());
            assert_eq!(values(&owner)[0].as_deref(), Some("B"));
            assert!(owner.journal.roles[0].owned);
        }
        #[test]
        fn startup_pending_write_is_recovered_before_new_baseline() {
            let mut old = owner();
            old.select("B".into(), &|| false).unwrap();
            let mut journal = old.journal.clone();
            journal.roles[0].last_applied = None;
            journal.roles[0].pending = Some("B".into());
            let mut fresh = Owner::new(old.backend, Store::default(), journal);
            assert_eq!(values(&fresh), all("B")); // Constructing a worker must not write.
            fresh.select("C".into(), &|| false).unwrap();
            fresh.restore().unwrap();
            assert_eq!(
                values(&fresh),
                [Some("A".into()), Some("A2".into()), Some("A3".into())]
            );
            let intent = old.store.saved[0].clone();
            let mut untouched = Owner::new(owner().backend, Store::default(), intent.clone());
            untouched.restore().unwrap();
            assert!(untouched.backend.writes.is_empty());
            assert!(!untouched.journal.pending());
            let mut partial = Owner::new(owner().backend, Store::default(), intent);
            partial.backend.current.borrow_mut()[0] = Some("B".into());
            assert!(partial.select("C".into(), &|| false).is_err());
            assert!(partial.journal.roles[0].owned);
            assert!(partial.backend.writes.is_empty());
        }
        #[test]
        fn cancelled_request_performs_no_writes_and_notification_loss_relinquishes() {
            let mut owner = owner();
            assert!(owner.select("B".into(), &|| true).is_err());
            assert!(owner.backend.writes.is_empty());
            owner.select("B".into(), &|| false).unwrap();
            owner.uncertainty().unwrap();
            owner.restore().unwrap();
            assert_eq!(values(&owner), all("B"));
        }
        #[test]
        fn invalid_journals_fail_closed() {
            let mut owner = owner();
            owner.select("B".into(), &|| false).unwrap();
            let mut journal = owner.journal.clone();
            journal.validate().unwrap();
            journal.roles.pop();
            assert!(journal.validate().is_err());
            assert!(serde_json::from_str::<Journal>("{}").is_err());
        }
        fn linked_owner() -> Owner<Fake, Store> {
            let mut owner = owner();
            owner.backend.linked_console_multimedia = true;
            owner.backend.current.borrow_mut()[1] = Some("A".into());
            owner
        }
        #[test]
        fn linked_side_effect_has_write_ahead_ownership_and_round_trips() {
            let mut owner = linked_owner();
            let before = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            assert_eq!(owner.store.saved[0].roles[1].pending.as_deref(), Some("B"));
            assert!(owner.journal.roles[1].owned);
            assert_eq!(owner.backend.writes, vec![(0, "B".into()), (2, "B".into())]);
            owner.restore().unwrap();
            assert_eq!(values(&owner), before);
            assert!(!owner.journal.pending());
        }
        #[test]
        fn linked_external_change_relinquishes_both_roles_before_restoration() {
            let mut owner = linked_owner();
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.current.borrow_mut()[1] = Some("C".into());
            let writes = owner.backend.writes.len();
            owner.restore().unwrap();
            assert_eq!(
                values(&owner),
                [Some("B".into()), Some("C".into()), Some("A3".into())]
            );
            assert!(owner.backend.writes[writes..]
                .iter()
                .all(|(role, _)| *role == 2));
        }
        #[test]
        fn linked_partial_failure_rolls_back_the_whole_selection() {
            let mut owner = linked_owner();
            let before = values(&owner);
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            owner.backend.fail = Some((2, "C".into()));
            assert!(owner.select("C".into(), &|| false).is_err());
            assert_eq!(values(&owner), all("B"));
            owner.restore().unwrap();
            assert_eq!(values(&owner), before);
        }
        #[test]
        fn unrepresentable_linked_baselines_remain_pending_instead_of_false_success() {
            let mut owner = owner();
            owner.backend.linked_console_multimedia = true;
            owner.select("B".into(), &|| false).unwrap();
            ack(&mut owner, "B");
            assert!(owner.restore().is_err());
            assert!(owner.journal.roles[0].owned);
            assert!(owner.snapshot().unwrap().restore_pending);
            let writes = owner.backend.writes.len();
            assert!(owner.restore().is_err());
            assert_eq!(writes, owner.backend.writes.len());
        }

        /// Run `probe` first, then explicitly authorize the bounded default-Capture
        /// round trip with SAYALL_DEFAULT_MIC_TEST=allow-default-capture-switch.
        /// Prefer the current default physical Capture. If it is virtual or
        /// ambiguous, set SAYALL_DEFAULT_MIC_PHYSICAL_INDEX from the probe output.
        /// No app launch, audio client, volume, Render write or Bluetooth operation.
        #[test]
        #[ignore = "Changes Capture defaults only with explicit local test authorization"]
        fn default_microphone_live_round_trip() {
            use windows::Win32::Media::Audio::{
                eRender, EDataFlow, Headset, Microphone, PKEY_AudioEndpoint_FormFactor,
            };
            use windows::Win32::System::Com::StructuredStorage::PropVariantToUInt32;

            let mode = std::env::var("SAYALL_DEFAULT_MIC_TEST").unwrap_or_default();
            assert!(
                mode == "probe"
                    || mode == "allow-default-capture-switch"
                    || mode == "allow-manual-capture-switch",
                "explicit probe/switch mode required"
            );
            wasapi::initialize_mta().ok().expect("test MTA");
            struct TestApartment;
            impl Drop for TestApartment {
                fn drop(&mut self) {
                    wasapi::deinitialize();
                }
            }
            let _apartment = TestApartment;
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                    .expect("independent enumerator");
            fn defaults(
                enumerator: &IMMDeviceEnumerator,
                flow: EDataFlow,
            ) -> Result<[String; 3], PlatformError> {
                let mut ids = Vec::new();
                for role in 0..3 {
                    let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, ERole(role)) }
                        .map_err(|e| failure("live_read_default", e))?;
                    let pointer =
                        unsafe { device.GetId() }.map_err(|e| failure("live_read_id", e))?;
                    let value = unsafe { pointer.to_string() }
                        .map_err(|e| failure("live_decode_id", e.into()));
                    unsafe {
                        CoTaskMemFree(Some(pointer.0.cast()));
                    }
                    ids.push(value?);
                }
                ids.try_into().map_err(|_| local_failure("live role count"))
            }
            let capture_before = defaults(&enumerator, eCapture).expect("three Capture baselines");
            let render_before = defaults(&enumerator, eRender).expect("three Render baselines");
            let microphones =
                crate::audio_devices::list_microphones().expect("read-only microphone inventory");
            let virtuals: Vec<_> = microphones
                .iter()
                .filter(|mic| mic.is_virtual_cable_candidate && !mic.render_endpoint_ids.is_empty())
                .collect();
            let mut physical: Vec<_> = microphones
                .iter()
                .filter(|mic| {
                    if mic.is_virtual_cable_candidate {
                        return false;
                    }
                    let id = wide(&mic.id);
                    let Ok(device) = (unsafe { enumerator.GetDevice(PCWSTR(id.as_ptr())) }) else {
                        return false;
                    };
                    let Ok(store) = (unsafe { device.OpenPropertyStore(STGM_READ) }) else {
                        return false;
                    };
                    let Ok(mut value) = (unsafe { store.GetValue(&PKEY_AudioEndpoint_FormFactor) })
                    else {
                        return false;
                    };
                    let factor = unsafe { PropVariantToUInt32(&value) }.ok();
                    let _ = unsafe { PropVariantClear(&mut value) };
                    factor == Some(Microphone.0 as u32) || factor == Some(Headset.0 as u32)
                })
                .collect();
            physical.sort_by(|left, right| left.id.cmp(&right.id));
            println!("default_microphone_live stage=probe capture_roles=3 render_roles=3 physical_candidates={} verified_cable_candidates={} read_only=true", physical.len(), virtuals.len());
            for (index, candidate) in physical.iter().enumerate() {
                println!("default_microphone_live stage=physical_candidate index={index} is_default={} read_only=true", candidate.id == capture_before[0]);
            }
            if mode == "probe" {
                return;
            }
            assert_eq!(
                virtuals.len(),
                1,
                "requires one verified virtual microphone"
            );
            let physical_index = match std::env::var("SAYALL_DEFAULT_MIC_PHYSICAL_INDEX") {
                Ok(value) => value.parse::<usize>().expect("physical test index must be an integer from the probe"),
                Err(_) => physical.iter().position(|candidate| candidate.id == capture_before[0])
                    .or_else(|| (physical.len() == 1).then_some(0))
                    .expect("probe then explicitly choose SAYALL_DEFAULT_MIC_PHYSICAL_INDEX; current default is not a unique physical candidate"),
            };
            let physical_target = *physical.get(physical_index).expect(
                "physical test index must refer to an active candidate from the current probe",
            );
            println!("default_microphone_live stage=test_target physical_index={physical_index} is_default={}", physical_target.id == capture_before[0]);
            if mode == "allow-manual-capture-switch" {
                assert!(
                    capture_before.iter().all(|id| id == &physical_target.id),
                    "manual test requires a uniform physical baseline; probe only otherwise"
                );
            }

            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test timestamp")
                .as_nanos();
            let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .canonicalize()
                .expect("repository target directory")
                .join(format!(
                    "default-microphone-live-{}-{stamp}",
                    std::process::id()
                ));
            std::fs::create_dir(&directory).expect("fresh private test directory");
            let mut baseline = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join("baseline.json"))
                .expect("new baseline evidence");
            baseline
                .write_all(
                    &serde_json::to_vec(
                        &serde_json::json!({"capture": capture_before, "render": render_before}),
                    )
                    .expect("baseline serialization"),
                )
                .expect("baseline saved");
            baseline.sync_all().expect("baseline flushed");
            let runtime = DefaultMicrophoneRuntime::new(directory.join("restore.json"))
                .expect("production worker initialization");
            struct RestoreGuard<'a>(&'a DefaultMicrophoneRuntime);
            impl Drop for RestoreGuard<'_> {
                fn drop(&mut self) {
                    let result = self.0.restore();
                    println!(
                        "default_microphone_live stage=scope_cleanup passed={}",
                        result.is_ok()
                    );
                }
            }
            let guard = RestoreGuard(&runtime);
            // Assertions are after cleanup. The guard also restores on an unexpected panic.
            let selected = (|| -> Result<(), PlatformError> {
                for (kind, target) in [("physical", physical_target), ("virtual", virtuals[0])] {
                    let snapshot = runtime.select(target.id.clone())?;
                    let capture = defaults(&enumerator, eCapture)?;
                    let render = defaults(&enumerator, eRender)?;
                    let capture_matches = capture.iter().all(|id| id == &target.id);
                    let render_unchanged = render == render_before;
                    println!("default_microphone_live stage=selected kind={kind} capture_all_match={capture_matches} render_unchanged={render_unchanged}");
                    if !capture_matches || !render_unchanged || snapshot.phase != "ready" {
                        return Err(local_failure("live independent selection readback failed"));
                    }
                    runtime.release_session()?;
                    let restored = defaults(&enumerator, eCapture)? == capture_before;
                    let render_unchanged = defaults(&enumerator, eRender)? == render_before;
                    println!("default_microphone_live stage=connection_released kind={kind} capture_baseline_restored={restored} render_unchanged={render_unchanged}");
                    if !restored || !render_unchanged || runtime.snapshot()?.restore_pending {
                        return Err(local_failure("live connection release readback failed"));
                    }
                    runtime.release_session()?;
                }
                if mode == "allow-manual-capture-switch" {
                    runtime.select(virtuals[0].id.clone())?;
                    let selected = runtime.use_now(physical_target.id.clone())?;
                    let capture_restored = defaults(&enumerator, eCapture)? == capture_before;
                    let render_unchanged = defaults(&enumerator, eRender)? == render_before;
                    println!("default_microphone_live stage=manual_committed capture_baseline_restored={capture_restored} render_unchanged={render_unchanged} restore_pending={}", selected.restore_pending);
                    if !capture_restored
                        || !render_unchanged
                        || selected.restore_pending
                        || !selected.external_change_observed
                    {
                        return Err(local_failure("live manual commit readback failed"));
                    }
                    runtime.release_session()?;
                    let manual_preserved = defaults(&enumerator, eCapture)? == capture_before;
                    if !manual_preserved {
                        return Err(local_failure("live manual choice was reverted on release"));
                    }
                    runtime.select(virtuals[0].id.clone())?;
                    runtime.release_session()?;
                    let fresh_baseline_restored =
                        defaults(&enumerator, eCapture)? == capture_before;
                    println!("default_microphone_live stage=manual_new_lease manual_preserved={manual_preserved} fresh_baseline_restored={fresh_baseline_restored}");
                    if !fresh_baseline_restored || defaults(&enumerator, eRender)? != render_before
                    {
                        return Err(local_failure("live manual fresh baseline readback failed"));
                    }
                }
                Ok(())
            })();
            let restored = runtime.restore();
            let capture_after = defaults(&enumerator, eCapture);
            let render_after = defaults(&enumerator, eRender);
            let capture_restored = capture_after
                .as_ref()
                .is_ok_and(|actual| actual == &capture_before);
            let render_unchanged = render_after
                .as_ref()
                .is_ok_and(|actual| actual == &render_before);
            println!("default_microphone_live stage=restored capture_baseline_restored={capture_restored} render_unchanged={render_unchanged}");
            drop(guard);
            assert!(
                selected.is_ok(),
                "bounded selection failed; cleanup attempted"
            );
            assert!(restored.is_ok(), "restore failed; journal retained");
            assert!(
                capture_restored && render_unchanged,
                "independent baseline readback failed; journal retained"
            );
            assert!(
                runtime.select(physical_target.id.clone()).is_err(),
                "exit gate rejects later selection"
            );
            assert!(
                runtime.use_now(physical_target.id.clone()).is_err(),
                "exit gate rejects later manual selection"
            );
        }
    }
}
