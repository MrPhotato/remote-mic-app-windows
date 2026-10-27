//! Host coordination for the temporary Windows recording-device selection.
//! Capture preference is independent of SayAll's WASAPI render route.
use crate::{platform::PlatformRuntime, settings::SettingsStore};
use sayall_windows::{
    AudioPhase, AudioSnapshot, ConnectionPhase, ConnectionSnapshot, MicrophoneEndpoint,
    MicrophoneSelectionSnapshot, PlatformError,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const SETTINGS_SAVE_FAILED: &str = "microphone_settings_save_failed";

trait MicrophoneApi: Send + Sync {
    fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError>;
    fn use_now(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError>;
    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError>;
    fn restore(&self) -> Result<(), PlatformError>;
    fn release_session(&self) -> Result<(), PlatformError>;
}

impl MicrophoneApi for sayall_windows::DefaultMicrophoneRuntime {
    fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Self::select(self, id)
    }
    fn use_now(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Self::use_now(self, id)
    }
    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Self::snapshot(self)
    }
    fn restore(&self) -> Result<(), PlatformError> {
        Self::restore(self)
    }
    fn release_session(&self) -> Result<(), PlatformError> {
        Self::release_session(self)
    }
}

struct UnavailableMicrophone(Option<String>);

impl MicrophoneApi for UnavailableMicrophone {
    fn select(&self, _: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Err(self
            .0
            .clone()
            .map(PlatformError::Audio)
            .unwrap_or(PlatformError::UnsupportedPlatform))
    }
    fn use_now(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        self.select(id)
    }
    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Ok(MicrophoneSelectionSnapshot {
            phase: if self.0.is_some() {
                "failed"
            } else {
                "unconfigured"
            }
            .into(),
            last_error: self.0.clone(),
            ..Default::default()
        })
    }
    fn restore(&self) -> Result<(), PlatformError> {
        Ok(())
    }
    fn release_session(&self) -> Result<(), PlatformError> {
        Ok(())
    }
}

// Only compiled into the explicit CI simulation build. It substitutes the
// recording-device API while keeping host validation, render selection,
// persistence, startup and shutdown on their normal paths.
#[cfg(feature = "runtime-simulation")]
struct SimulatedMicrophone {
    state: Mutex<SimulatedMicrophoneState>,
}

#[cfg(feature = "runtime-simulation")]
struct SimulatedMicrophoneState {
    snapshot: MicrophoneSelectionSnapshot,
    closed: bool,
}

#[cfg(feature = "runtime-simulation")]
impl Default for SimulatedMicrophone {
    fn default() -> Self {
        let original = crate::platform::SimulatedPlatform::microphone_endpoints()
            .into_iter()
            .find(|endpoint| {
                endpoint.id == crate::platform::SimulatedPlatform::PHYSICAL_MICROPHONE_ID
            })
            .expect("simulation original capture exists");
        Self {
            state: Mutex::new(SimulatedMicrophoneState {
                snapshot: MicrophoneSelectionSnapshot {
                    current_microphone_id: Some(original.id),
                    current_microphone_name: Some(original.name),
                    phase: "unconfigured".into(),
                    ..Default::default()
                },
                closed: false,
            }),
        }
    }
}

#[cfg(feature = "runtime-simulation")]
impl MicrophoneApi for SimulatedMicrophone {
    fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(PlatformError::Audio(
                "CI simulation microphone is closed".into(),
            ));
        }
        let endpoint = crate::platform::SimulatedPlatform::microphone_endpoints()
            .into_iter()
            .find(|endpoint| endpoint.id == id)
            .ok_or_else(|| PlatformError::Audio("CI simulation capture is unknown".into()))?;
        state.snapshot = MicrophoneSelectionSnapshot {
            target_microphone_id: Some(endpoint.id.clone()),
            target_microphone_name: Some(endpoint.name.clone()),
            current_microphone_id: Some(endpoint.id),
            current_microphone_name: Some(endpoint.name),
            phase: "ready".into(),
            restore_pending: true,
            ..Default::default()
        };
        Ok(state.snapshot.clone())
    }

    fn use_now(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        self.select(id)?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.snapshot.target_microphone_id = None;
        state.snapshot.target_microphone_name = None;
        state.snapshot.phase = "external".into();
        state.snapshot.external_change_observed = true;
        state.snapshot.restore_pending = false;
        Ok(state.snapshot.clone())
    }

    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .clone())
    }

    fn restore(&self) -> Result<(), PlatformError> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        self.release_session()
    }

    fn release_session(&self) -> Result<(), PlatformError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let external =
            state.snapshot.phase == "external" || state.snapshot.external_change_observed;
        let original = Self::default().snapshot()?;
        if !external {
            state.snapshot.current_microphone_id = original.current_microphone_id;
            state.snapshot.current_microphone_name = original.current_microphone_name;
        }
        state.snapshot.target_microphone_id = None;
        state.snapshot.target_microphone_name = None;
        state.snapshot.external_change_observed = external;
        state.snapshot.phase = if external || state.closed {
            "external"
        } else {
            "unconfigured"
        }
        .into();
        state.snapshot.restore_pending = false;
        Ok(())
    }
}

#[cfg(feature = "runtime-simulation")]
fn simulation_runtime(requested: bool) -> Option<Arc<dyn MicrophoneApi>> {
    requested.then(|| Arc::new(SimulatedMicrophone::default()) as Arc<dyn MicrophoneApi>)
}

trait CaptureRoutes {
    fn connection(&self) -> ConnectionSnapshot;
    fn microphones(&self) -> Result<Vec<MicrophoneEndpoint>, PlatformError>;
    fn audio(&self) -> AudioSnapshot;
    fn open_render(&self, id: String) -> Result<AudioSnapshot, PlatformError>;
}

struct PlatformRoutes<'a>(&'a dyn PlatformRuntime);
impl CaptureRoutes for PlatformRoutes<'_> {
    fn connection(&self) -> ConnectionSnapshot {
        self.0.connection_snapshot()
    }
    fn microphones(&self) -> Result<Vec<MicrophoneEndpoint>, PlatformError> {
        self.0.list_microphones()
    }
    fn audio(&self) -> AudioSnapshot {
        self.0.audio_snapshot()
    }
    fn open_render(&self, id: String) -> Result<AudioSnapshot, PlatformError> {
        self.0.select_audio_endpoint(id)
    }
}

pub struct MicrophoneController {
    runtime: Arc<dyn MicrophoneApi>,
    selection: Mutex<()>,
    closing: AtomicBool,
    startup_claimed: AtomicBool,
    startup_error: Mutex<Option<String>>,
    saved_target: Mutex<Option<(String, Option<String>)>>,
    active_link: Mutex<Option<u64>>,
    external_override: AtomicBool,
    monitor_started: AtomicBool,
    connection_retry: Mutex<Option<(Option<u64>, Instant)>>,
    enabled: bool,
}

impl MicrophoneController {
    pub fn new(settings: &SettingsStore, simulation: bool) -> Self {
        #[cfg(feature = "runtime-simulation")]
        if let Some(runtime) = simulation_runtime(simulation) {
            return Self::with_runtime(runtime);
        }
        let runtime: Arc<dyn MicrophoneApi> = if cfg!(windows) && !simulation {
            match sayall_windows::DefaultMicrophoneRuntime::new(settings.microphone_journal_path())
            {
                Ok(runtime) => Arc::new(runtime),
                Err(error) => {
                    note("initialize", "failed", "runtime_unavailable");
                    Arc::new(UnavailableMicrophone(Some(error.to_string())))
                }
            }
        } else {
            Arc::new(UnavailableMicrophone(None))
        };
        let mut controller = Self::with_runtime(runtime);
        controller.enabled = cfg!(windows) && !simulation;
        controller
    }

    fn with_runtime(runtime: Arc<dyn MicrophoneApi>) -> Self {
        Self {
            runtime,
            selection: Mutex::new(()),
            closing: AtomicBool::new(false),
            startup_claimed: AtomicBool::new(false),
            startup_error: Mutex::new(None),
            saved_target: Mutex::new(None),
            active_link: Mutex::new(None),
            external_override: AtomicBool::new(false),
            monitor_started: AtomicBool::new(false),
            connection_retry: Mutex::new(None),
            enabled: true,
        }
    }

    pub fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        let mut snapshot = self.runtime.snapshot()?;
        // The target is intent, while current always comes from Windows. A
        // startup validation failure must not erase the saved target from UI.
        if snapshot.target_microphone_id.is_none()
            || self
                .active_link
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
        {
            if let Some((id, name)) = self
                .saved_target
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
            {
                snapshot.target_microphone_id = Some(id);
                snapshot.target_microphone_name = name;
            }
        }
        if self.external_override.load(Ordering::SeqCst) {
            snapshot.phase = "external".into();
        } else if self
            .active_link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
            && snapshot.target_microphone_id.is_some()
            && !snapshot.restore_pending
            && snapshot.phase != "failed"
        {
            snapshot.phase = "waiting_connection".into();
        }
        if let Some(error) = self
            .startup_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            snapshot.phase = "failed".into();
            snapshot.last_error = Some(error);
        }
        Ok(snapshot)
    }

    pub fn select(
        &self,
        id: String,
        platform: &dyn PlatformRuntime,
        settings: &SettingsStore,
    ) -> Result<MicrophoneSelectionSnapshot, String> {
        if !platform
            .list_microphones()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|capture| capture.id == id && capture.is_virtual_cable_candidate)
        {
            note(
                "validate",
                "failed",
                "remote_target_requires_virtual_capture",
            );
            return Err("遥控器目标需要选择虚拟麦克风；普通麦克风请直接使用。".into());
        }
        self.select_with_routes(id, &PlatformRoutes(platform), settings)
    }

    pub fn use_now(
        &self,
        id: String,
        platform: &dyn PlatformRuntime,
        settings: &SettingsStore,
    ) -> Result<MicrophoneSelectionSnapshot, String> {
        self.use_now_with_routes(id, &PlatformRoutes(platform), settings)
    }

    fn use_now_with_routes(
        &self,
        id: String,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) -> Result<MicrophoneSelectionSnapshot, String> {
        let started = Instant::now();
        let _guard = self.selection.lock().unwrap_or_else(|e| e.into_inner());
        let result = (|| {
            self.check_open()?;
            let captures = routes.microphones().map_err(|error| error.to_string())?;
            if !captures
                .iter()
                .any(|capture| capture.id == id && !capture.is_virtual_cable_candidate)
            {
                note("use_now", "failed", "physical_capture_unavailable");
                return Err("所选普通麦克风当前不可用，已保留原设置。".into());
            }
            if matches!(
                routes.audio().phase,
                AudioPhase::Streaming | AudioPhase::Draining
            ) {
                note("use_now", "failed", "audio_busy");
                return Err(PlatformError::AudioBusy.to_string());
            }
            if !self.startup_claimed.load(Ordering::SeqCst) {
                self.load_preference_locked(routes, settings)?;
            }
            self.check_open()?;
            let selected = match self.runtime.use_now(id.clone()) {
                Ok(selected) => selected,
                Err(error) => {
                    // A committed manual choice can outlive a failed final
                    // readback. Never let a reconnect reclaim an uncertain or
                    // already relinquished choice after this explicit request.
                    let observed = self.runtime.snapshot();
                    if observed.as_ref().map_or(true, |snapshot| {
                        snapshot.external_change_observed && !snapshot.restore_pending
                    }) {
                        self.external_override.store(true, Ordering::SeqCst);
                        note("use_now", "unconfirmed", "manual_result_preserved");
                    }
                    return Err(error.to_string());
                }
            };
            // Ok means the platform already committed the manual handoff.
            // A later external change can make the final snapshot unconfirmed,
            // but must not let the first ready connection reclaim the default.
            self.external_override.store(true, Ordering::SeqCst);
            if selected.current_microphone_id.as_deref() != Some(id.as_str())
                || selected.restore_pending
                || !selected.external_change_observed
                || selected.phase != "external"
            {
                note("use_now", "failed", "manual_selection_unconfirmed");
                return Err("麦克风切换未确认，请刷新后重试。".into());
            }
            *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *self
                .connection_retry
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            note(
                "use_now",
                "passed",
                "manual_default_preserved_remote_target_unchanged",
            );
            self.snapshot().map_err(|error| error.to_string())
        })();
        terminal("use_now", result.is_ok(), started);
        result
    }

    fn load_preference_locked(
        &self,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) -> Result<(), String> {
        let saved = settings.load().map_err(|error| {
            note("load_preference", "failed", "settings_unavailable");
            error
        })?;
        *self.saved_target.lock().unwrap_or_else(|e| e.into_inner()) = saved
            .target_microphone_id
            .clone()
            .map(|id| (id, saved.target_microphone_name.clone()));
        // Recover old leases before acquiring any new system default baseline.
        if self
            .runtime
            .snapshot()
            .map_err(|error| error.to_string())?
            .restore_pending
        {
            self.release_locked()?;
        }
        let captures = routes.microphones().map_err(|error| error.to_string())?;
        let legacy_physical = saved.target_microphone_id.as_ref().is_some_and(|id| {
            captures
                .iter()
                .any(|capture| &capture.id == id && !capture.is_virtual_cable_candidate)
        });
        if legacy_physical {
            // Do not present or auto-apply a known old physical target. Keep the
            // persisted value until a uniquely verified virtual replacement can
            // be saved, so missing/ambiguous hardware is never guessed.
            *self.saved_target.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        if saved.target_microphone_id.is_none() || legacy_physical {
            if let Some(id) = startup_capture(
                &captures,
                &routes.audio(),
                saved.audio_endpoint_id.as_deref(),
            ) {
                self.save_waiting_preference(id, routes, settings)?;
                if legacy_physical {
                    note(
                        "migrate_preference",
                        "passed",
                        "physical_target_replaced_by_verified_virtual",
                    );
                }
            } else {
                note("load_preference", "skipped", "no_unique_virtual_capture");
            }
        }
        self.startup_claimed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn select_with_routes(
        &self,
        id: String,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) -> Result<MicrophoneSelectionSnapshot, String> {
        let started = Instant::now();
        let _guard = self.selection.lock().unwrap_or_else(|e| e.into_inner());
        *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let requested_id = id.clone();
        let result = (|| {
            self.check_open()?;
            let connection = ready_link(&routes.connection());
            let previous = *self.active_link.lock().unwrap_or_else(|e| e.into_inner());
            if previous.is_none()
                && self
                    .runtime
                    .snapshot()
                    .map_err(|e| e.to_string())?
                    .restore_pending
            {
                self.release_locked()?;
            }
            if previous.is_some() && previous != connection {
                self.release_locked()?;
            }
            if connection.is_none() {
                self.save_waiting_preference(id, routes, settings)?;
                self.external_override.store(false, Ordering::SeqCst);
                return self.snapshot().map_err(|e| e.to_string());
            }
            *self.active_link.lock().unwrap_or_else(|e| e.into_inner()) = connection;
            self.select_locked(id, routes, settings)
        })();
        let applied_despite_save_failure = result
            .as_ref()
            .is_err_and(|error| error == SETTINGS_SAVE_FAILED)
            && self
                .active_link
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            && self.runtime.snapshot().is_ok_and(|snapshot| {
                snapshot.phase == "ready"
                    && snapshot.target_microphone_id.as_deref() == Some(requested_id.as_str())
                    && snapshot.current_microphone_id.as_deref() == Some(requested_id.as_str())
            });
        if result.is_ok() || applied_despite_save_failure {
            // Only a saved/applied user choice supersedes preference loading.
            // Invalid endpoints and persistence errors must leave a pending
            // initialization retry able to load the previous saved target.
            self.startup_claimed.store(true, Ordering::SeqCst);
            *self
                .connection_retry
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
        }
        terminal("select", result.is_ok(), started);
        result
    }

    fn select_locked(
        &self,
        id: String,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) -> Result<MicrophoneSelectionSnapshot, String> {
        self.check_open()?;
        let expected_link = *self.active_link.lock().unwrap_or_else(|e| e.into_inner());
        if expected_link.is_none() || ready_link(&routes.connection()) != expected_link {
            self.release_locked()?;
            self.save_waiting_preference(id, routes, settings)?;
            return self.snapshot().map_err(|e| e.to_string());
        }
        note("select", "started", "capture_requested");
        let microphones = routes.microphones().map_err(|e| {
            note("enumerate", "failed", "capture_list_unavailable");
            e.to_string()
        })?;
        let microphone = microphones
            .iter()
            .find(|microphone| microphone.id == id)
            .ok_or_else(|| {
                note("validate", "failed", "capture_unavailable");
                "所选麦克风当前不可用，已保留原设置。".to_owned()
            })?;
        if !microphone.is_virtual_cable_candidate {
            note(
                "validate",
                "failed",
                "remote_target_requires_virtual_capture",
            );
            return Err("遥控器目标需要选择虚拟麦克风；普通麦克风请直接使用。".into());
        }
        let audio = routes.audio();
        if matches!(audio.phase, AudioPhase::Streaming | AudioPhase::Draining) {
            note("validate", "failed", "audio_busy");
            return Err(PlatformError::AudioBusy.to_string());
        }
        if microphone.is_virtual_cable_candidate {
            let render =
                choose_verified_render(microphone, &microphones, &audio).ok_or_else(|| {
                    note("validate", "failed", "render_pair_unconfirmed");
                    "未能确认这个麦克风的传声线路，已保留原设置。".to_owned()
                })?;
            if audio.phase != AudioPhase::Ready
                || audio.selected_endpoint_id.as_deref() != Some(render.as_str())
            {
                self.check_open()?;
                let opened = routes.open_render(render).map_err(|e| {
                    note("prepare_render", "failed", "route_unavailable");
                    e.to_string()
                })?;
                if opened.phase != AudioPhase::Ready
                    || !opened.selected_endpoint_id.as_ref().is_some_and(|id| {
                        microphone.render_endpoint_ids.contains(id)
                            && microphones
                                .iter()
                                .filter(|other| other.render_endpoint_ids.contains(id))
                                .count()
                                == 1
                    })
                {
                    note("select", "failed", "render_not_ready_or_wrong_capture");
                    return Err("麦克风传声线路未就绪，Windows 默认麦克风未切换。".into());
                }
            }
            note("prepare_render", "passed", "verified_virtual_route_ready");
        } else {
            note("prepare_render", "skipped", "physical_capture");
        }
        self.check_open()?;
        if ready_link(&routes.connection()) != expected_link {
            self.release_locked()?;
            self.save_waiting_preference(id, routes, settings)?;
            return self.snapshot().map_err(|e| e.to_string());
        }
        // The runtime's final restore also atomically closes its own Select gate;
        // a timed-out or late request cannot overwrite the restored default.
        let selected = self.runtime.select(id.clone()).map_err(|e| {
            note("set_default", "failed", "runtime_selection_failed");
            e.to_string()
        })?;
        if selected.phase != "ready"
            || selected.target_microphone_id.as_deref() != Some(id.as_str())
            || selected.current_microphone_id.as_deref() != Some(id.as_str())
        {
            note("select", "failed", "selection_not_confirmed");
            return Err(selected
                .last_error
                .unwrap_or_else(|| "麦克风切换未确认，请刷新后重试。".into()));
        }
        let Some(name) = selected.target_microphone_name.clone() else {
            note("set_default", "failed", "target_name_missing");
            return Err("麦克风切换结果缺少设备名称，请刷新后重试。".into());
        };
        // The requested Capture was independently confirmed. This deliberate
        // switch is valid even if persistence subsequently fails.
        self.external_override.store(false, Ordering::SeqCst);
        let persisted = !(settings.sync_audio_endpoint(|| routes.audio()).is_err()
            || settings
                .save_target_microphone(id.clone(), name.clone())
                .is_err());
        if persisted {
            *self.saved_target.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, Some(name)));
        }
        if ready_link(&routes.connection()) != expected_link || self.closing.load(Ordering::SeqCst)
        {
            note("select", "cancelled", "connection_changed_during_selection");
            self.release_locked()?;
            return self.snapshot().map_err(|e| e.to_string());
        }
        if !persisted {
            note("select", "failed", "settings_save_failed");
            return Err(SETTINGS_SAVE_FAILED.into());
        }
        note("select", "passed", "capture_selected_and_saved");
        Ok(selected)
    }

    fn save_waiting_preference(
        &self,
        id: String,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) -> Result<(), String> {
        let captures = routes.microphones().map_err(|e| e.to_string())?;
        let capture = captures
            .iter()
            .find(|capture| capture.id == id)
            .ok_or_else(|| "所选麦克风当前不可用，已保留原设置。".to_owned())?;
        if !capture.is_virtual_cable_candidate {
            note(
                "save_preference",
                "failed",
                "remote_target_requires_virtual_capture",
            );
            return Err("遥控器目标需要选择虚拟麦克风；普通麦克风请直接使用。".into());
        }
        if capture.is_virtual_cable_candidate
            && choose_verified_render(capture, &captures, &routes.audio()).is_none()
        {
            note("save_preference", "failed", "render_pair_unconfirmed");
            return Err("未能确认这个麦克风的传声线路，已保留原设置。".into());
        }
        settings
            .save_target_microphone(id.clone(), capture.name.clone())
            .map_err(|_| SETTINGS_SAVE_FAILED.to_owned())?;
        *self.saved_target.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((id, Some(capture.name.clone())));
        note("save_preference", "passed", "waiting_for_remote_connection");
        Ok(())
    }

    pub fn reconcile_connection(&self, platform: &dyn PlatformRuntime, settings: &SettingsStore) {
        self.reconcile_connection_with_routes(&PlatformRoutes(platform), settings);
    }

    /// Connection ownership is independent of page visibility and app startup.
    /// Poll only the existing BLE snapshot; never operate the Bluetooth adapter.
    pub fn start_connection_monitor(
        self: &Arc<Self>,
        platform: Arc<dyn PlatformRuntime>,
        settings: SettingsStore,
    ) {
        if self.monitor_started.swap(true, Ordering::SeqCst) || !self.enabled {
            return;
        }
        let controller = Arc::clone(self);
        let result = std::thread::Builder::new()
            .name("sayall-microphone-connection".into())
            .spawn(move || {
                note(
                    "connection_monitor",
                    "started",
                    "read_only_connection_snapshot",
                );
                while !controller.closing.load(Ordering::SeqCst) {
                    controller.reconcile_connection(platform.as_ref(), &settings);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                note("connection_monitor", "stopped", "application_exiting");
            });
        if result.is_err() {
            self.monitor_started.store(false, Ordering::SeqCst);
            *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) =
                Some("无法启动麦克风连接监测，尚未自动切换系统设备。".into());
            note("connection_monitor", "failed", "worker_unavailable");
        }
    }

    fn reconcile_connection_with_routes(
        &self,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) {
        let started = Instant::now();
        if !self.enabled {
            return;
        }
        let _guard = self.selection.lock().unwrap_or_else(|e| e.into_inner());
        if self.closing.load(Ordering::SeqCst) {
            return;
        }
        let initialize = !self.startup_claimed.load(Ordering::SeqCst);
        let connection = ready_link(&routes.connection());
        let previous = *self.active_link.lock().unwrap_or_else(|e| e.into_inner());
        let retry = *self
            .connection_retry
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if retry.is_some_and(|(failed_link, deadline)| {
            failed_link == connection && Instant::now() < deadline
        }) {
            return;
        }
        if !initialize && connection == previous && retry.is_none() {
            return;
        }
        let result = (|| {
            self.check_open()?;
            if initialize {
                self.load_preference_locked(routes, settings)?;
            }
            if previous.is_some() && previous != connection {
                note("connection", "changed", "previous_connection_ended");
                self.release_locked()?;
            }
            let Some(link) = ready_link(&routes.connection()) else {
                note("connection", "waiting", "remote_not_ready");
                return Ok(());
            };
            *self.active_link.lock().unwrap_or_else(|e| e.into_inner()) = Some(link);
            let observed = self.runtime.snapshot().map_err(|e| e.to_string())?;
            if previous.is_some()
                && (observed.phase == "external" || observed.external_change_observed)
            {
                self.external_override.store(true, Ordering::SeqCst);
            }
            if self.external_override.load(Ordering::SeqCst) {
                note("connection", "skipped", "external_default_preserved");
                return Ok(());
            }
            *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let target = self
                .saved_target
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if let Some((id, _)) = target {
                self.select_locked(id, routes, settings)?;
                note("connection", "passed", "saved_capture_applied");
            } else {
                note("connection", "skipped", "no_unique_capture_preference");
            }
            Ok::<(), String>(())
        })();
        terminal("connection", result.is_ok(), started);
        if result.is_ok() {
            *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        *self
            .connection_retry
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = if result.is_err() {
            // Retry a failed audio operation, not Bluetooth. The fixed backoff
            // prevents transient failures from flooding the 500 ms observer.
            Some((
                connection,
                Instant::now() + std::time::Duration::from_secs(5),
            ))
        } else {
            None
        };
        if let Err(error) = result {
            // A successful system switch remains ready even if persistence
            // failed. The stable IPC error is only for the selecting caller.
            if error != SETTINGS_SAVE_FAILED {
                *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
            }
            note("connection", "failed", "capture_not_applied");
        }
    }

    fn release_locked(&self) -> Result<(), String> {
        let started = Instant::now();
        let observed = self.runtime.snapshot();
        if observed
            .as_ref()
            .is_ok_and(|snapshot| snapshot.phase == "external" || snapshot.external_change_observed)
        {
            self.external_override.store(true, Ordering::SeqCst);
            note("release", "observed", "external_default_preserved");
        }
        let result = self
            .runtime
            .release_session()
            .map_err(|e| e.to_string())
            .and_then(|()| {
                let restored = self.runtime.snapshot().map_err(|e| e.to_string())?;
                if restored.phase == "external" || restored.external_change_observed {
                    self.external_override.store(true, Ordering::SeqCst);
                }
                Ok(())
            });
        terminal("release", result.is_ok(), started);
        // Keep the previous generation on failure so a later connection cannot
        // silently acquire a new baseline over an unresolved restoration.
        if result.is_ok() {
            *self.active_link.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        result
    }

    pub fn begin_exit(&self) {
        self.closing.store(true, Ordering::SeqCst);
    }

    pub fn restore(&self) -> Result<(), PlatformError> {
        let started = Instant::now();
        self.begin_exit();
        // Do not wait on a host selection mutex here: the platform worker owns
        // ordering and cancels queued Select operations when restore is requested.
        let result = self.runtime.restore();
        terminal("restore", result.is_ok(), started);
        result
    }

    fn check_open(&self) -> Result<(), String> {
        if !self.enabled {
            note("validate", "failed", "platform_unsupported");
            return Err(PlatformError::UnsupportedPlatform.to_string());
        }
        if self.closing.load(Ordering::SeqCst) {
            note("validate", "failed", "application_exiting");
            Err("应用正在退出，无法切换麦克风。".into())
        } else {
            Ok(())
        }
    }
}

fn ready_link(connection: &ConnectionSnapshot) -> Option<u64> {
    matches!(
        connection.phase,
        ConnectionPhase::Ready | ConnectionPhase::Streaming | ConnectionPhase::Draining
    )
    .then_some(connection.connection_generation)
}

fn choose_verified_render(
    microphone: &MicrophoneEndpoint,
    microphones: &[MicrophoneEndpoint],
    audio: &AudioSnapshot,
) -> Option<String> {
    let verified: Vec<_> = microphone
        .render_endpoint_ids
        .iter()
        .filter(|id| {
            microphones
                .iter()
                .filter(|other| other.render_endpoint_ids.contains(id))
                .count()
                == 1
        })
        .collect();
    audio
        .selected_endpoint_id
        .as_ref()
        .filter(|id| verified.contains(id))
        .cloned()
        .or_else(|| verified.first().map(|id| (*id).clone()))
}

fn startup_capture(
    microphones: &[MicrophoneEndpoint],
    audio: &AudioSnapshot,
    saved_render: Option<&str>,
) -> Option<String> {
    let actual = (audio.phase == AudioPhase::Ready)
        .then_some(audio.selected_endpoint_id.as_deref())
        .flatten();
    if let Some(render) = actual.or(saved_render) {
        let mut matching = microphones.iter().filter(|mic| {
            mic.is_virtual_cable_candidate && mic.render_endpoint_ids.iter().any(|id| id == render)
        });
        let first = matching.next()?;
        return matching.next().is_none().then(|| first.id.clone());
    }
    let mut virtuals = microphones
        .iter()
        .filter(|mic| mic.is_virtual_cable_candidate && !mic.render_endpoint_ids.is_empty());
    let first = virtuals.next()?;
    virtuals.next().is_none().then(|| first.id.clone())
}

fn note(action: &str, result: &str, reason: &str) {
    sayall_windows::gatt_note(format!(
        "microphone_selection action={action} terminal_result={result} reason={reason}"
    ));
}

fn terminal(action: &str, passed: bool, started: Instant) {
    sayall_windows::gatt_note(format!(
        "microphone_selection action={action} phase=completed terminal_result={} elapsed_ms={}",
        if passed { "passed" } else { "failed" },
        started.elapsed().as_millis()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[derive(Default)]
    struct FakeMicrophone {
        calls: Mutex<Vec<String>>,
        current: Mutex<MicrophoneSelectionSnapshot>,
        fail: AtomicBool,
        fail_snapshot: std::sync::atomic::AtomicUsize,
        fail_release: AtomicBool,
        manual_external_current: Mutex<Option<String>>,
    }

    impl MicrophoneApi for FakeMicrophone {
        fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            self.calls.lock().unwrap().push(format!("select:{id}"));
            if self.fail.load(Ordering::SeqCst) {
                return Err(PlatformError::Audio("test selection failure".into()));
            }
            let selected = MicrophoneSelectionSnapshot {
                target_microphone_id: Some(id.clone()),
                target_microphone_name: Some(format!("Name {id}")),
                current_microphone_id: Some(id.clone()),
                current_microphone_name: Some(format!("Name {id}")),
                phase: "ready".into(),
                restore_pending: true,
                ..Default::default()
            };
            *self.current.lock().unwrap() = selected.clone();
            Ok(selected)
        }
        fn use_now(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            self.calls.lock().unwrap().push(format!("use_now:{id}"));
            if self.fail.load(Ordering::SeqCst) {
                return Err(PlatformError::Audio("test selection failure".into()));
            }
            let id = self
                .manual_external_current
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(id);
            let snapshot = MicrophoneSelectionSnapshot {
                current_microphone_id: Some(id.clone()),
                current_microphone_name: Some(format!("Name {id}")),
                phase: "external".into(),
                external_change_observed: true,
                ..Default::default()
            };
            *self.current.lock().unwrap() = snapshot.clone();
            Ok(snapshot)
        }
        fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            if self
                .fail_snapshot
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                    count.checked_sub(1)
                })
                .is_ok()
            {
                return Err(PlatformError::Audio(
                    "test transient snapshot failure".into(),
                ));
            }
            Ok(self.current.lock().unwrap().clone())
        }
        fn restore(&self) -> Result<(), PlatformError> {
            self.calls.lock().unwrap().push("restore".into());
            Ok(())
        }
        fn release_session(&self) -> Result<(), PlatformError> {
            self.calls.lock().unwrap().push("release".into());
            if self.fail_release.load(Ordering::SeqCst) {
                return Err(PlatformError::Audio(
                    "test transient restore failure".into(),
                ));
            }
            let mut snapshot = self.current.lock().unwrap();
            if snapshot.phase != "external" && !snapshot.external_change_observed {
                snapshot.phase = "unconfigured".into();
                snapshot.current_microphone_id = Some("original".into());
            }
            snapshot.restore_pending = false;
            Ok(())
        }
    }

    struct FakeRoutes {
        captures: Vec<MicrophoneEndpoint>,
        current: Mutex<AudioSnapshot>,
        opens: Mutex<Vec<String>>,
        opened: Option<AudioSnapshot>,
        fail: bool,
        connection: Mutex<ConnectionSnapshot>,
        fail_enumerate: AtomicBool,
        connection_sequence: Mutex<std::collections::VecDeque<ConnectionSnapshot>>,
    }

    impl FakeRoutes {
        fn new(captures: Vec<MicrophoneEndpoint>) -> Self {
            Self {
                captures,
                current: Mutex::new(AudioSnapshot::default()),
                opens: Mutex::new(Vec::new()),
                opened: None,
                fail: false,
                connection: Mutex::new(ConnectionSnapshot {
                    phase: ConnectionPhase::Ready,
                    connection_generation: 1,
                    ..Default::default()
                }),
                fail_enumerate: AtomicBool::new(false),
                connection_sequence: Mutex::new(std::collections::VecDeque::new()),
            }
        }
    }

    impl CaptureRoutes for FakeRoutes {
        fn connection(&self) -> ConnectionSnapshot {
            if let Some(snapshot) = self.connection_sequence.lock().unwrap().pop_front() {
                return snapshot;
            }
            self.connection.lock().unwrap().clone()
        }
        fn microphones(&self) -> Result<Vec<MicrophoneEndpoint>, PlatformError> {
            if self.fail_enumerate.load(Ordering::SeqCst) {
                return Err(PlatformError::Audio("test enumeration failure".into()));
            }
            Ok(self.captures.clone())
        }
        fn audio(&self) -> AudioSnapshot {
            self.current.lock().unwrap().clone()
        }
        fn open_render(&self, id: String) -> Result<AudioSnapshot, PlatformError> {
            self.opens.lock().unwrap().push(id.clone());
            if self.fail {
                return Err(PlatformError::Audio("test route failure".into()));
            }
            let opened = self.opened.clone().unwrap_or_else(|| ready(&id));
            *self.current.lock().unwrap() = opened.clone();
            Ok(opened)
        }
    }

    fn capture(id: &str, renders: &[&str]) -> MicrophoneEndpoint {
        MicrophoneEndpoint {
            id: id.into(),
            name: format!("Name {id}"),
            is_virtual_cable_candidate: !renders.is_empty(),
            is_default: false,
            is_default_communications: false,
            render_endpoint_ids: renders.iter().map(|id| (*id).into()).collect(),
        }
    }

    fn ready(id: &str) -> AudioSnapshot {
        AudioSnapshot {
            phase: AudioPhase::Ready,
            selected_endpoint_id: Some(id.into()),
            selected_endpoint_name: Some(format!("Name {id}")),
            ..AudioSnapshot::default()
        }
    }

    fn store() -> SettingsStore {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/local-launch/microphone-host-tests")
            .join(format!("{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        SettingsStore::new(directory.join("settings.json"))
    }

    fn fixture() -> (Arc<FakeMicrophone>, MicrophoneController, SettingsStore) {
        let api = Arc::new(FakeMicrophone::default());
        let controller = MicrophoneController::with_runtime(api.clone());
        (api, controller, store())
    }

    fn expire_retry(controller: &MicrophoneController) {
        if let Some((_, deadline)) = controller.connection_retry.lock().unwrap().as_mut() {
            *deadline = Instant::now();
        }
    }

    #[test]
    fn manual_physical_choice_keeps_virtual_target_and_is_not_reclaimed() {
        for connected in [false, true] {
            let (api, controller, store) = fixture();
            store
                .save_target_microphone("cable".into(), "Cable".into())
                .unwrap();
            let routes = FakeRoutes::new(vec![
                capture("cable", &["render"]),
                capture("physical", &[]),
            ]);
            if !connected {
                routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
            }
            controller.reconcile_connection_with_routes(&routes, &store);
            let selected = controller
                .use_now_with_routes("physical".into(), &routes, &store)
                .unwrap();
            assert_eq!(selected.target_microphone_id.as_deref(), Some("cable"));
            assert_eq!(selected.current_microphone_id.as_deref(), Some("physical"));
            assert_eq!(selected.phase, "external");
            assert!(!selected.restore_pending);
            assert_eq!(
                store.load().unwrap().target_microphone_id.as_deref(),
                Some("cable")
            );
            api.calls.lock().unwrap().clear();
            routes.connection.lock().unwrap().phase = ConnectionPhase::Disconnected;
            controller.reconcile_connection_with_routes(&routes, &store);
            routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
            routes.connection.lock().unwrap().connection_generation += 1;
            controller.reconcile_connection_with_routes(&routes, &store);
            assert!(!api
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call.starts_with("select:")));
            assert_eq!(
                controller
                    .snapshot()
                    .unwrap()
                    .current_microphone_id
                    .as_deref(),
                Some("physical")
            );
            controller
                .select_with_routes("cable".into(), &routes, &store)
                .unwrap();
            assert!(!controller.external_override.load(Ordering::SeqCst));
            assert_eq!(
                controller
                    .snapshot()
                    .unwrap()
                    .current_microphone_id
                    .as_deref(),
                Some("cable")
            );
        }
    }

    #[test]
    fn manual_choice_rejects_virtual_missing_busy_and_exiting_without_writing() {
        for scenario in ["virtual", "missing", "busy", "exiting"] {
            let (api, controller, store) = fixture();
            let routes = FakeRoutes::new(vec![
                capture("cable", &["render"]),
                capture("physical", &[]),
            ]);
            if scenario == "busy" {
                routes.current.lock().unwrap().phase = AudioPhase::Streaming;
            }
            if scenario == "exiting" {
                controller.begin_exit();
            }
            let id = match scenario {
                "virtual" => "cable",
                "missing" => "missing",
                _ => "physical",
            };
            assert!(controller
                .use_now_with_routes(id.into(), &routes, &store)
                .is_err());
            assert!(api.calls.lock().unwrap().is_empty());
            assert!(routes.opens.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn failed_manual_choice_keeps_saved_virtual_target_and_does_not_set_override() {
        let (api, controller, store) = fixture();
        store
            .save_target_microphone("cable".into(), "Cable".into())
            .unwrap();
        let routes = FakeRoutes::new(vec![
            capture("cable", &["render"]),
            capture("physical", &[]),
        ]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        api.fail.store(true, Ordering::SeqCst);
        assert!(controller
            .use_now_with_routes("physical".into(), &routes, &store)
            .is_err());
        assert!(!controller.external_override.load(Ordering::SeqCst));
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("cable")
        );
        assert_eq!(
            controller
                .snapshot()
                .unwrap()
                .target_microphone_id
                .as_deref(),
            Some("cable")
        );
    }

    #[test]
    fn committed_manual_choice_with_unconfirmed_readback_is_not_reclaimed_on_first_ready_or_reconnect(
    ) {
        let (api, controller, store) = fixture();
        store
            .save_target_microphone("cable".into(), "Cable".into())
            .unwrap();
        let routes = FakeRoutes::new(vec![
            capture("cable", &["render"]),
            capture("physical", &[]),
        ]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        *api.manual_external_current.lock().unwrap() = Some("external-after-commit".into());
        assert!(controller
            .use_now_with_routes("physical".into(), &routes, &store)
            .is_err());
        assert!(controller.external_override.load(Ordering::SeqCst));
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("cable")
        );
        for phase in [
            ConnectionPhase::Ready,
            ConnectionPhase::Disconnected,
            ConnectionPhase::Ready,
        ] {
            let mut connection = routes.connection.lock().unwrap();
            connection.phase = phase;
            connection.connection_generation += 1;
            drop(connection);
            controller.reconcile_connection_with_routes(&routes, &store);
        }
        assert!(api
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| !call.starts_with("select:")));
        assert_eq!(
            controller
                .snapshot()
                .unwrap()
                .current_microphone_id
                .as_deref(),
            Some("external-after-commit")
        );
        assert!(routes.opens.lock().unwrap().is_empty());
    }

    #[test]
    fn legacy_physical_target_migrates_only_to_a_verified_unique_virtual_capture() {
        let (api, controller, store) = fixture();
        store
            .save_target_microphone("physical".into(), "Physical".into())
            .unwrap();
        let routes = FakeRoutes::new(vec![
            capture("physical", &[]),
            capture("cable", &["render"]),
        ]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(api.calls.lock().unwrap().is_empty());
        assert!(routes.opens.lock().unwrap().is_empty());
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("cable")
        );
        assert_eq!(
            controller
                .snapshot()
                .unwrap()
                .target_microphone_id
                .as_deref(),
            Some("cable")
        );
        routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable"]);
    }

    #[test]
    fn legacy_physical_target_is_not_auto_applied_when_virtual_capture_is_missing_or_ambiguous() {
        for virtuals in [
            vec![],
            vec![
                capture("one", &["render-one"]),
                capture("two", &["render-two"]),
            ],
        ] {
            let (api, controller, store) = fixture();
            store
                .save_target_microphone("physical".into(), "Physical".into())
                .unwrap();
            let mut captures = vec![capture("physical", &[])];
            captures.extend(virtuals);
            let routes = FakeRoutes::new(captures);
            controller.reconcile_connection_with_routes(&routes, &store);
            assert!(api.calls.lock().unwrap().is_empty());
            assert!(routes.opens.lock().unwrap().is_empty());
            assert!(controller
                .snapshot()
                .unwrap()
                .target_microphone_id
                .is_none());
            assert_eq!(
                store.load().unwrap().target_microphone_id.as_deref(),
                Some("physical")
            );
            controller
                .use_now_with_routes("physical".into(), &routes, &store)
                .unwrap();
            assert_eq!(
                controller
                    .snapshot()
                    .unwrap()
                    .current_microphone_id
                    .as_deref(),
                Some("physical")
            );
        }
    }

    #[test]
    fn missing_legacy_physical_target_cannot_be_auto_applied_when_it_reappears() {
        let (api, controller, store) = fixture();
        store
            .save_target_microphone("physical".into(), "Physical".into())
            .unwrap();
        let mut routes = FakeRoutes::new(vec![]);
        controller.reconcile_connection_with_routes(&routes, &store);
        routes.captures.push(capture("physical", &[]));
        expire_retry(&controller);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(api.calls.lock().unwrap().is_empty());
        assert_eq!(controller.snapshot().unwrap().phase, "failed");
        assert!(controller
            .select_with_routes("physical".into(), &routes, &store)
            .is_err());
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        assert!(controller
            .select_with_routes("physical".into(), &routes, &store)
            .is_err());
        assert!(api
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| call == "release"));
    }

    #[test]
    fn unready_connections_save_only_and_ready_disconnect_reconnect_pair_leases() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        for phase in [
            ConnectionPhase::Idle,
            ConnectionPhase::Connecting,
            ConnectionPhase::Discovering,
            ConnectionPhase::AwaitingCapabilities,
            ConnectionPhase::Reconnecting,
            ConnectionPhase::Suspended,
            ConnectionPhase::Disconnected,
            ConnectionPhase::Failed,
        ] {
            routes.connection.lock().unwrap().phase = phase;
            let saved = controller
                .select_with_routes("cable".into(), &routes, &store)
                .unwrap();
            assert_eq!(saved.phase, "waiting_connection");
            assert!(!saved.restore_pending);
            assert!(api.calls.lock().unwrap().is_empty());
            assert!(routes.opens.lock().unwrap().is_empty());
        }
        routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
        controller.reconcile_connection_with_routes(&routes, &store);
        for phase in [
            ConnectionPhase::Streaming,
            ConnectionPhase::Draining,
            ConnectionPhase::Ready,
        ] {
            routes.connection.lock().unwrap().phase = phase;
            routes.connection.lock().unwrap().generation += 1;
            controller.reconcile_connection_with_routes(&routes, &store);
        }
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable"]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Disconnected;
        controller.reconcile_connection_with_routes(&routes, &store);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(controller.snapshot().unwrap().phase, "waiting_connection");
        routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
        routes.connection.lock().unwrap().connection_generation += 1;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(
            *api.calls.lock().unwrap(),
            ["select:cable", "release", "select:cable"]
        );
        // A complete disconnect/reconnect between polls is still a new lease.
        routes.connection.lock().unwrap().connection_generation += 1;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(
            *api.calls.lock().unwrap(),
            [
                "select:cable",
                "release",
                "select:cable",
                "release",
                "select:cable"
            ]
        );
    }

    #[test]
    fn offline_startup_recovers_crash_journal_without_selecting() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        api.current.lock().unwrap().restore_pending = true;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["release"]);
        assert!(routes.opens.lock().unwrap().is_empty());
        assert_eq!(controller.snapshot().unwrap().phase, "waiting_connection");
    }

    #[test]
    fn same_connection_retries_transient_audio_failure_with_backoff() {
        for failure in ["busy", "route", "system"] {
            let (api, controller, store) = fixture();
            let mut routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
            store
                .save_target_microphone("cable".into(), "Cable".into())
                .unwrap();
            match failure {
                "busy" => routes.current.lock().unwrap().phase = AudioPhase::Streaming,
                "route" => routes.fail = true,
                _ => api.fail.store(true, Ordering::SeqCst),
            }
            controller.reconcile_connection_with_routes(&routes, &store);
            assert_eq!(controller.snapshot().unwrap().phase, "failed");
            let calls = api.calls.lock().unwrap().len();
            let opens = routes.opens.lock().unwrap().len();
            controller.reconcile_connection_with_routes(&routes, &store);
            assert_eq!(api.calls.lock().unwrap().len(), calls);
            assert_eq!(routes.opens.lock().unwrap().len(), opens);
            *routes.current.lock().unwrap() = AudioSnapshot::default();
            routes.fail = false;
            api.fail.store(false, Ordering::SeqCst);
            expire_retry(&controller);
            controller.reconcile_connection_with_routes(&routes, &store);
            assert_eq!(controller.snapshot().unwrap().phase, "ready", "{failure}");
        }
    }

    #[test]
    fn initialization_enumeration_failure_retries_while_still_offline() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        routes.fail_enumerate.store(true, Ordering::SeqCst);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(!controller.startup_claimed.load(Ordering::SeqCst));
        routes.fail_enumerate.store(false, Ordering::SeqCst);
        expire_retry(&controller);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(controller.startup_claimed.load(Ordering::SeqCst));
        assert!(api.calls.lock().unwrap().is_empty());
        assert_eq!(controller.snapshot().unwrap().phase, "waiting_connection");
    }

    #[test]
    fn failed_phase_external_change_blocks_automatic_retry_and_reconnect() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        controller.reconcile_connection_with_routes(&routes, &store);
        {
            let mut current = api.current.lock().unwrap();
            current.phase = "failed".into();
            current.current_microphone_id = Some("external".into());
            current.external_change_observed = true;
        }
        *controller.connection_retry.lock().unwrap() = Some((Some(1), Instant::now()));
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable"]);
        routes.connection.lock().unwrap().connection_generation += 1;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable", "release"]);
        assert_eq!(controller.snapshot().unwrap().phase, "external");
        controller
            .select_with_routes("cable".into(), &routes, &store)
            .unwrap();
        assert_eq!(
            *api.calls.lock().unwrap(),
            ["select:cable", "release", "select:cable"]
        );
        assert_eq!(controller.snapshot().unwrap().phase, "ready");
    }

    #[test]
    fn disconnect_during_system_selection_releases_before_reporting_waiting() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        let ready = routes.connection();
        let disconnected = ConnectionSnapshot {
            phase: ConnectionPhase::Disconnected,
            ..ready.clone()
        };
        *routes.connection.lock().unwrap() = disconnected.clone();
        routes.connection_sequence.lock().unwrap().extend([
            ready.clone(),
            ready.clone(),
            ready,
            disconnected,
        ]);
        let result = controller
            .select_with_routes("cable".into(), &routes, &store)
            .unwrap();
        assert_eq!(result.phase, "waiting_connection");
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable", "release"]);
        assert!(!result.restore_pending);
    }

    #[test]
    fn transient_release_snapshot_failure_does_not_become_external_override() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        controller.reconcile_connection_with_routes(&routes, &store);
        api.fail_snapshot.store(1, Ordering::SeqCst);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Disconnected;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(!controller.external_override.load(Ordering::SeqCst));
        routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
        routes.connection.lock().unwrap().connection_generation += 1;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(
            *api.calls.lock().unwrap(),
            ["select:cable", "release", "select:cable"]
        );
    }

    #[test]
    fn failed_disconnect_restore_retries_before_any_new_lease() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        controller.reconcile_connection_with_routes(&routes, &store);
        api.fail_release.store(true, Ordering::SeqCst);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Disconnected;
        controller.reconcile_connection_with_routes(&routes, &store);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable", "release"]);
        assert!(controller.snapshot().unwrap().restore_pending);
        api.fail_release.store(false, Ordering::SeqCst);
        expire_retry(&controller);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(controller.snapshot().unwrap().phase, "waiting_connection");
        routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
        routes.connection.lock().unwrap().connection_generation += 1;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(
            *api.calls.lock().unwrap(),
            ["select:cable", "release", "release", "select:cable"]
        );
    }

    #[test]
    fn startup_snapshot_error_retries_without_claiming_initialization() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
        api.fail_snapshot.store(1, Ordering::SeqCst);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(!controller.startup_claimed.load(Ordering::SeqCst));
        expire_retry(&controller);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(controller.snapshot().unwrap().phase, "waiting_connection");
        assert!(api.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn failed_explicit_choice_does_not_claim_pending_saved_preference_loading() {
        for failure in ["invalid", "snapshot", "save"] {
            let (api, controller, store) = fixture();
            store
                .save_target_microphone("cable".into(), "Cable".into())
                .unwrap();
            let path = store
                .microphone_journal_path()
                .with_file_name("settings.json");
            let original = std::fs::read(&path).unwrap();
            let routes = FakeRoutes::new(vec![
                capture("cable", &["render"]),
                capture("physical", &[]),
            ]);
            routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
            // Initialization has failed before loading any saved target.
            std::fs::write(&path, b"invalid settings").unwrap();
            controller.reconcile_connection_with_routes(&routes, &store);
            assert!(controller.saved_target.lock().unwrap().is_none());
            let retry = *controller.connection_retry.lock().unwrap();
            std::fs::write(&path, &original).unwrap();
            if failure == "snapshot" {
                api.fail_snapshot.store(1, Ordering::SeqCst);
            }
            if failure == "save" {
                std::fs::write(&path, b"invalid settings").unwrap();
            }
            let id = if failure == "invalid" {
                "missing"
            } else {
                "cable"
            };
            let result = controller.select_with_routes(id.into(), &routes, &store);
            assert!(result.is_err());
            if failure == "save" {
                assert_eq!(result.unwrap_err(), SETTINGS_SAVE_FAILED);
            }
            assert!(!controller.startup_claimed.load(Ordering::SeqCst));
            assert_eq!(*controller.connection_retry.lock().unwrap(), retry);
            std::fs::write(&path, &original).unwrap();
            routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
            controller.reconcile_connection_with_routes(&routes, &store);
            assert_eq!(*api.calls.lock().unwrap(), ["select:cable"], "{failure}");
            assert_eq!(controller.snapshot().unwrap().phase, "ready");
        }
    }

    #[test]
    fn failed_explicit_choice_does_not_revoke_external_default_protection() {
        for failure in ["invalid", "save"] {
            let (api, controller, store) = fixture();
            store
                .save_target_microphone("cable".into(), "Cable".into())
                .unwrap();
            let path = store
                .microphone_journal_path()
                .with_file_name("settings.json");
            let original = std::fs::read(&path).unwrap();
            let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
            routes.connection.lock().unwrap().phase = ConnectionPhase::Idle;
            controller.external_override.store(true, Ordering::SeqCst);
            if failure == "save" {
                std::fs::write(&path, b"invalid settings").unwrap();
            }
            let id = if failure == "invalid" {
                "missing"
            } else {
                "cable"
            };
            assert!(controller
                .select_with_routes(id.into(), &routes, &store)
                .is_err());
            assert!(controller.external_override.load(Ordering::SeqCst));
            std::fs::write(&path, &original).unwrap();
            routes.connection.lock().unwrap().phase = ConnectionPhase::Ready;
            controller.reconcile_connection_with_routes(&routes, &store);
            assert!(api.calls.lock().unwrap().is_empty());
            assert_eq!(controller.snapshot().unwrap().phase, "external");
        }
    }

    #[test]
    fn applied_selection_with_save_failure_keeps_the_actual_target_truthful() {
        let (_, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![
            capture("first", &["render-first"]),
            capture("second", &["render-second"]),
        ]);
        controller
            .select_with_routes("first".into(), &routes, &store)
            .unwrap();
        let path = store
            .microphone_journal_path()
            .with_file_name("settings.json");
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"invalid settings").unwrap();
        controller.external_override.store(true, Ordering::SeqCst);
        assert_eq!(
            controller
                .select_with_routes("second".into(), &routes, &store)
                .unwrap_err(),
            SETTINGS_SAVE_FAILED
        );
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "ready");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("second"));
        assert_eq!(
            snapshot.current_microphone_id,
            snapshot.target_microphone_id
        );
        assert!(!controller.external_override.load(Ordering::SeqCst));
        assert!(controller.startup_claimed.load(Ordering::SeqCst));
        std::fs::write(&path, &original).unwrap();
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("first")
        );
    }

    #[cfg(feature = "runtime-simulation")]
    #[test]
    fn simulation_waits_for_connection_then_selects_and_persists_shared_cable_capture() {
        use crate::platform::SimulatedPlatform;

        let store = store();
        let platform = SimulatedPlatform::default();
        let controller = MicrophoneController::new(&store, true);
        let before = controller.snapshot().unwrap();
        assert_eq!(before.phase, "unconfigured");
        assert_eq!(
            before.current_microphone_id.as_deref(),
            Some(SimulatedPlatform::PHYSICAL_MICROPHONE_ID)
        );
        controller.reconcile_connection(&platform, &store);
        let waiting = controller.snapshot().unwrap();
        assert_eq!(waiting.phase, "waiting_connection");
        assert_eq!(waiting.current_microphone_id, before.current_microphone_id);
        assert!(!waiting.restore_pending);
        let remote = platform.scan_paired_remotes().unwrap().remove(0);
        platform.connect_remote(remote.id).unwrap();
        controller.reconcile_connection(&platform, &store);
        let selected = controller.snapshot().unwrap();
        assert_eq!(selected.phase, "ready");
        assert_eq!(
            selected.target_microphone_id.as_deref(),
            Some(SimulatedPlatform::CABLE_MICROPHONE_ID)
        );
        assert_eq!(
            selected.current_microphone_id,
            selected.target_microphone_id
        );
        assert!(selected.restore_pending);

        let captures = platform.list_microphones().unwrap();
        assert_eq!(captures.len(), 2);
        let cable = captures
            .iter()
            .find(|capture| capture.id == SimulatedPlatform::CABLE_MICROPHONE_ID)
            .unwrap();
        assert_eq!(
            selected.target_microphone_name.as_deref(),
            Some(cable.name.as_str())
        );
        let audio = platform.audio_snapshot();
        assert_eq!(audio.phase, AudioPhase::Ready);
        assert!(cable
            .render_endpoint_ids
            .contains(audio.selected_endpoint_id.as_ref().unwrap()));
        let saved = store.load().unwrap();
        assert_eq!(saved.target_microphone_id, selected.target_microphone_id);
        assert_eq!(
            saved.target_microphone_name,
            selected.target_microphone_name
        );
        assert_eq!(saved.audio_endpoint_id, audio.selected_endpoint_id);
        assert!(!store.microphone_journal_path().exists());
    }

    #[cfg(feature = "runtime-simulation")]
    #[test]
    fn simulation_selects_physical_then_cable_and_restores_original_on_exit() {
        use crate::platform::SimulatedPlatform;

        let store = store();
        let platform = SimulatedPlatform::default();
        let controller = MicrophoneController::new(&store, true);
        let original = controller.snapshot().unwrap();
        let remote = platform.scan_paired_remotes().unwrap().remove(0);
        platform.connect_remote(remote.id).unwrap();
        controller.reconcile_connection(&platform, &store);
        let render = platform.audio_snapshot();

        for id in [
            SimulatedPlatform::PHYSICAL_MICROPHONE_ID,
            SimulatedPlatform::CABLE_MICROPHONE_ID,
        ] {
            let physical = id == SimulatedPlatform::PHYSICAL_MICROPHONE_ID;
            let selected = if physical {
                controller.use_now(id.into(), &platform, &store).unwrap()
            } else {
                controller.select(id.into(), &platform, &store).unwrap()
            };
            assert_eq!(selected.phase, if physical { "external" } else { "ready" });
            assert_eq!(selected.current_microphone_id.as_deref(), Some(id));
            assert_eq!(
                selected.target_microphone_id.as_deref(),
                Some(SimulatedPlatform::CABLE_MICROPHONE_ID)
            );
            assert_eq!(
                store.load().unwrap().target_microphone_id.as_deref(),
                Some(SimulatedPlatform::CABLE_MICROPHONE_ID)
            );
            assert_eq!(
                platform.audio_snapshot().selected_endpoint_id,
                render.selected_endpoint_id
            );
        }

        controller.restore().unwrap();
        let restored = controller.snapshot().unwrap();
        assert_eq!(
            restored.current_microphone_id,
            original.current_microphone_id
        );
        assert_eq!(
            restored.current_microphone_name,
            original.current_microphone_name
        );
        assert_eq!(restored.phase, "external");
        assert!(!restored.restore_pending);
        assert!(controller
            .select(
                SimulatedPlatform::CABLE_MICROPHONE_ID.into(),
                &platform,
                &store
            )
            .is_err());
        // Even a Select already queued at the API boundary cannot reapply a
        // target after restore, matching the real runtime's shutdown gate.
        assert!(controller
            .runtime
            .select(SimulatedPlatform::CABLE_MICROPHONE_ID.into())
            .is_err());
        assert_eq!(
            controller.snapshot().unwrap().current_microphone_id,
            original.current_microphone_id
        );
        controller.restore().unwrap();
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some(SimulatedPlatform::CABLE_MICROPHONE_ID)
        );
        assert!(!store.microphone_journal_path().exists());
    }

    #[cfg(feature = "runtime-simulation")]
    #[test]
    fn simulation_runtime_requires_an_explicit_request_and_rejects_unknown_capture() {
        assert!(simulation_runtime(false).is_none());
        let runtime = simulation_runtime(true).unwrap();
        let original = runtime.snapshot().unwrap();
        assert!(runtime.select("not-a-simulated-capture".into()).is_err());
        let after = runtime.snapshot().unwrap();
        assert_eq!(after.current_microphone_id, original.current_microphone_id);
        assert_eq!(after.phase, "unconfigured");
        assert!(!after.restore_pending);
    }

    #[test]
    fn physical_capture_does_not_open_render_and_preserves_other_settings() {
        let (api, controller, store) = fixture();
        store.save_launch_at_login(true).unwrap();
        store
            .sync_audio_endpoint(|| ready("existing-render"))
            .unwrap();
        let routes = FakeRoutes::new(vec![capture("physical", &[])]);
        let selected = controller
            .use_now_with_routes("physical".into(), &routes, &store)
            .unwrap();
        assert_eq!(selected.phase, "external");
        assert_eq!(*api.calls.lock().unwrap(), ["use_now:physical"]);
        assert!(routes.opens.lock().unwrap().is_empty());
        let saved = store.load().unwrap();
        assert!(saved.target_microphone_id.is_none());
        assert!(saved.target_microphone_name.is_none());
        assert_eq!(saved.audio_endpoint_id.as_deref(), Some("existing-render"));
        assert!(saved.launch_at_login);
    }

    #[test]
    fn virtual_selection_saves_actual_same_capture_fallback_render() {
        let (api, controller, store) = fixture();
        let mut routes = FakeRoutes::new(vec![capture("cable", &["render-a", "render-b"])]);
        routes.opened = Some(ready("render-b"));
        controller
            .select_with_routes("cable".into(), &routes, &store)
            .unwrap();
        assert_eq!(*routes.opens.lock().unwrap(), ["render-a"]);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable"]);
        assert_eq!(
            store.load().unwrap().audio_endpoint_id.as_deref(),
            Some("render-b")
        );
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("cable")
        );
    }

    #[test]
    fn failed_or_wrong_capture_route_never_switches_default_or_saves_target() {
        for route_failure in [true, false] {
            let (api, controller, store) = fixture();
            store
                .save_target_microphone("previous".into(), "Previous".into())
                .unwrap();
            let mut routes = FakeRoutes::new(vec![capture("cable", &["render-a"])]);
            routes.fail = route_failure;
            routes.opened = Some(ready("unrelated-render"));
            assert!(controller
                .select_with_routes("cable".into(), &routes, &store)
                .is_err());
            assert!(api.calls.lock().unwrap().is_empty());
            assert_eq!(
                store.load().unwrap().target_microphone_id.as_deref(),
                Some("previous")
            );
        }
    }

    #[test]
    fn ambiguous_pair_is_rejected_before_opening_or_switching() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![
            capture("first", &["shared-render"]),
            capture("second", &["shared-render"]),
        ]);
        assert!(controller
            .select_with_routes("first".into(), &routes, &store)
            .is_err());
        assert!(api.calls.lock().unwrap().is_empty());
        assert!(routes.opens.lock().unwrap().is_empty());
    }

    #[test]
    fn missing_or_busy_capture_does_not_mutate_runtime_or_preference() {
        for busy in [false, true] {
            let (api, controller, store) = fixture();
            store
                .save_target_microphone("previous".into(), "Previous".into())
                .unwrap();
            let routes = FakeRoutes::new(if busy {
                vec![capture("cable", &["render"])]
            } else {
                vec![]
            });
            if busy {
                routes.current.lock().unwrap().phase = AudioPhase::Streaming;
            }
            assert!(controller
                .select_with_routes("cable".into(), &routes, &store)
                .is_err());
            assert!(api.calls.lock().unwrap().is_empty());
            assert_eq!(
                store.load().unwrap().target_microphone_id.as_deref(),
                Some("previous")
            );
        }
    }

    #[test]
    fn failed_system_select_does_not_save_target() {
        let (api, controller, store) = fixture();
        api.fail.store(true, Ordering::SeqCst);
        store
            .save_target_microphone("previous".into(), "Previous".into())
            .unwrap();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        assert!(controller
            .select_with_routes("cable".into(), &routes, &store)
            .is_err());
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("previous")
        );
    }

    #[test]
    fn save_failure_has_stable_error_and_snapshot_remains_truthful() {
        let (_, controller, good_store) = fixture();
        let path = good_store
            .microphone_journal_path()
            .with_file_name("blocked-settings.json");
        std::fs::create_dir(&path).unwrap();
        let blocked = SettingsStore::new(path);
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        assert_eq!(
            controller
                .select_with_routes("cable".into(), &routes, &blocked)
                .unwrap_err(),
            SETTINGS_SAVE_FAILED
        );
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "ready");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("cable"));
    }

    #[test]
    fn external_default_change_never_overwrites_saved_target() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("target", &["render"])]);
        controller
            .select_with_routes("target".into(), &routes, &store)
            .unwrap();
        {
            let mut snapshot = api.current.lock().unwrap();
            snapshot.current_microphone_id = Some("external".into());
            snapshot.phase = "external".into();
        }
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "external");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("target"));
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("target")
        );
    }

    #[test]
    fn exit_gate_rejects_selection_and_startup_and_restores() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        controller.begin_exit();
        assert!(controller
            .select_with_routes("cable".into(), &routes, &store)
            .is_err());
        controller.reconcile_connection_with_routes(&routes, &store);
        controller.restore().unwrap();
        assert_eq!(*api.calls.lock().unwrap(), ["restore"]);
        assert!(routes.opens.lock().unwrap().is_empty());
    }

    #[test]
    fn startup_saved_capture_is_exact_and_never_falls_back_when_missing() {
        let (api, controller, store) = fixture();
        *api.current.lock().unwrap() = MicrophoneSelectionSnapshot {
            current_microphone_id: Some("current-system-mic".into()),
            current_microphone_name: Some("Current system microphone".into()),
            ..Default::default()
        };
        store
            .save_target_microphone("missing".into(), "Missing".into())
            .unwrap();
        let routes = FakeRoutes::new(vec![capture("cable", &["render"])]);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(api.calls.lock().unwrap().is_empty());
        assert!(routes.opens.lock().unwrap().is_empty());
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "failed");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("missing"));
        assert_eq!(snapshot.target_microphone_name.as_deref(), Some("Missing"));
        assert_eq!(
            snapshot.current_microphone_id.as_deref(),
            Some("current-system-mic")
        );
        assert_eq!(
            snapshot.current_microphone_name.as_deref(),
            Some("Current system microphone")
        );
        assert_eq!(
            store.load().unwrap().target_microphone_id.as_deref(),
            Some("missing")
        );
    }

    #[test]
    fn startup_route_failure_keeps_saved_target_and_actual_current_without_overriding_later_selection(
    ) {
        let (api, controller, store) = fixture();
        *api.current.lock().unwrap() = MicrophoneSelectionSnapshot {
            current_microphone_id: Some("physical".into()),
            current_microphone_name: Some("Actual physical microphone".into()),
            ..Default::default()
        };
        store
            .save_target_microphone("cable".into(), "Saved cable".into())
            .unwrap();
        let mut routes = FakeRoutes::new(vec![
            capture("cable", &["render"]),
            capture("physical", &[]),
        ]);
        routes.fail = true;
        controller.reconcile_connection_with_routes(&routes, &store);
        assert!(api.calls.lock().unwrap().is_empty());
        assert_eq!(*routes.opens.lock().unwrap(), ["render"]);
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "failed");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("cable"));
        assert_eq!(
            snapshot.target_microphone_name.as_deref(),
            Some("Saved cable")
        );
        assert_eq!(snapshot.current_microphone_id.as_deref(), Some("physical"));
        assert_eq!(
            snapshot.current_microphone_name.as_deref(),
            Some("Actual physical microphone")
        );

        controller
            .use_now_with_routes("physical".into(), &routes, &store)
            .unwrap();
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "external");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("cable"));
        assert_eq!(
            snapshot.target_microphone_name.as_deref(),
            Some("Saved cable")
        );
    }

    #[test]
    fn startup_applies_once_and_explicit_selection_supersedes_it() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![
            capture("cable", &["render"]),
            capture("physical", &[]),
        ]);
        controller.reconcile_connection_with_routes(&routes, &store);
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable"]);

        let (api, controller, store) = fixture();
        controller
            .use_now_with_routes("physical".into(), &routes, &store)
            .unwrap();
        controller.reconcile_connection_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["use_now:physical"]);
    }

    #[test]
    fn startup_uses_actual_route_then_saved_route_and_never_guesses_ambiguous_cables() {
        let captures = vec![
            capture("first", &["render-a"]),
            capture("second", &["render-b"]),
        ];
        assert_eq!(
            startup_capture(&captures, &ready("render-b"), Some("render-a")),
            Some("second".into())
        );
        assert_eq!(
            startup_capture(&captures, &AudioSnapshot::default(), Some("render-a")),
            Some("first".into())
        );
        assert_eq!(
            startup_capture(&captures, &AudioSnapshot::default(), Some("missing")),
            None
        );
        assert_eq!(
            startup_capture(&captures, &AudioSnapshot::default(), None),
            None
        );
        assert_eq!(
            startup_capture(&captures[..1], &AudioSnapshot::default(), None),
            Some("first".into())
        );
        assert_eq!(
            startup_capture(&[capture("physical", &[])], &AudioSnapshot::default(), None),
            None
        );
    }
}
