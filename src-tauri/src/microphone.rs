//! Host coordination for the temporary Windows recording-device selection.
//! Capture preference is independent of SayAll's WASAPI render route.
use crate::{platform::PlatformRuntime, settings::SettingsStore};
use sayall_windows::{
    AudioPhase, AudioSnapshot, MicrophoneEndpoint, MicrophoneSelectionSnapshot, PlatformError,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const SETTINGS_SAVE_FAILED: &str = "microphone_settings_save_failed";

trait MicrophoneApi: Send + Sync {
    fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError>;
    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError>;
    fn restore(&self) -> Result<(), PlatformError>;
}

impl MicrophoneApi for sayall_windows::DefaultMicrophoneRuntime {
    fn select(&self, id: String) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Self::select(self, id)
    }
    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Self::snapshot(self)
    }
    fn restore(&self) -> Result<(), PlatformError> {
        Self::restore(self)
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

    fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .clone())
    }

    fn restore(&self) -> Result<(), PlatformError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        let original = Self::default().snapshot()?;
        state.snapshot.current_microphone_id = original.current_microphone_id;
        state.snapshot.current_microphone_name = original.current_microphone_name;
        if state.snapshot.target_microphone_id.is_some()
            && state.snapshot.target_microphone_id != state.snapshot.current_microphone_id
        {
            state.snapshot.phase = "external".into();
        }
        state.snapshot.restore_pending = false;
        Ok(())
    }
}

#[cfg(feature = "runtime-simulation")]
fn simulation_runtime(requested: bool) -> Option<Arc<dyn MicrophoneApi>> {
    requested.then(|| Arc::new(SimulatedMicrophone::default()) as Arc<dyn MicrophoneApi>)
}

trait CaptureRoutes {
    fn microphones(&self) -> Result<Vec<MicrophoneEndpoint>, PlatformError>;
    fn audio(&self) -> AudioSnapshot;
    fn open_render(&self, id: String) -> Result<AudioSnapshot, PlatformError>;
}

struct PlatformRoutes<'a>(&'a dyn PlatformRuntime);
impl CaptureRoutes for PlatformRoutes<'_> {
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
            enabled: true,
        }
    }

    pub fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
        let mut snapshot = self.runtime.snapshot()?;
        // The target is intent, while current always comes from Windows. A
        // startup validation failure must not erase the saved target from UI.
        if snapshot.target_microphone_id.is_none() {
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
        self.select_with_routes(id, &PlatformRoutes(platform), settings)
    }

    fn select_with_routes(
        &self,
        id: String,
        routes: &dyn CaptureRoutes,
        settings: &SettingsStore,
    ) -> Result<MicrophoneSelectionSnapshot, String> {
        let started = Instant::now();
        // An explicit user selection supersedes startup inference even if Ready's
        // background task has not started yet.
        self.startup_claimed.store(true, Ordering::SeqCst);
        let _guard = self.selection.lock().unwrap_or_else(|e| e.into_inner());
        *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let result = self.select_locked(id, routes, settings);
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
        if settings.sync_audio_endpoint(|| routes.audio()).is_err()
            || settings.save_target_microphone(id, name).is_err()
        {
            note("select", "failed", "settings_save_failed");
            return Err(SETTINGS_SAVE_FAILED.into());
        }
        note("select", "passed", "capture_selected_and_saved");
        Ok(selected)
    }

    pub fn apply_startup(&self, platform: &dyn PlatformRuntime, settings: &SettingsStore) {
        self.apply_startup_with_routes(&PlatformRoutes(platform), settings);
    }

    fn apply_startup_with_routes(&self, routes: &dyn CaptureRoutes, settings: &SettingsStore) {
        let started = Instant::now();
        if !self.enabled {
            return;
        }
        // Check the claim after obtaining the lock: a user selection that won
        // the lock must not be followed by a delayed startup selection.
        let _guard = self.selection.lock().unwrap_or_else(|e| e.into_inner());
        if self.startup_claimed.swap(true, Ordering::SeqCst) || self.closing.load(Ordering::SeqCst)
        {
            return;
        }
        let result = (|| {
            self.check_open()?;
            let saved = settings.load().map_err(|error| {
                note("load_preference", "failed", "settings_unavailable");
                error
            })?;
            *self.saved_target.lock().unwrap_or_else(|e| e.into_inner()) = saved
                .target_microphone_id
                .clone()
                .map(|id| (id, saved.target_microphone_name.clone()));
            let id = if let Some(id) = saved.target_microphone_id {
                Some(id)
            } else {
                let microphones = routes.microphones().map_err(|error| {
                    note("enumerate", "failed", "startup_capture_list_unavailable");
                    error.to_string()
                })?;
                startup_capture(
                    &microphones,
                    &routes.audio(),
                    saved.audio_endpoint_id.as_deref(),
                )
            };
            if let Some(id) = id {
                self.select_locked(id, routes, settings)?;
                note("startup", "passed", "saved_or_verified_capture_applied");
            } else {
                note("startup", "skipped", "no_unique_capture_preference");
            }
            Ok::<(), String>(())
        })();
        terminal("startup", result.is_ok(), started);
        if let Err(error) = result {
            // A successful system switch remains ready even if persistence
            // failed. The stable IPC error is only for the selecting caller.
            if error != SETTINGS_SAVE_FAILED {
                *self.startup_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
            }
            note("startup", "failed", "capture_not_applied");
        }
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
        fn snapshot(&self) -> Result<MicrophoneSelectionSnapshot, PlatformError> {
            Ok(self.current.lock().unwrap().clone())
        }
        fn restore(&self) -> Result<(), PlatformError> {
            self.calls.lock().unwrap().push("restore".into());
            Ok(())
        }
    }

    struct FakeRoutes {
        captures: Vec<MicrophoneEndpoint>,
        current: Mutex<AudioSnapshot>,
        opens: Mutex<Vec<String>>,
        opened: Option<AudioSnapshot>,
        fail: bool,
    }

    impl FakeRoutes {
        fn new(captures: Vec<MicrophoneEndpoint>) -> Self {
            Self {
                captures,
                current: Mutex::new(AudioSnapshot::default()),
                opens: Mutex::new(Vec::new()),
                opened: None,
                fail: false,
            }
        }
    }

    impl CaptureRoutes for FakeRoutes {
        fn microphones(&self) -> Result<Vec<MicrophoneEndpoint>, PlatformError> {
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

    #[cfg(feature = "runtime-simulation")]
    #[test]
    fn simulation_startup_selects_and_persists_the_shared_cable_capture() {
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
        controller.apply_startup(&platform, &store);
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
        controller.apply_startup(&platform, &store);
        let render = platform.audio_snapshot();

        for id in [
            SimulatedPlatform::PHYSICAL_MICROPHONE_ID,
            SimulatedPlatform::CABLE_MICROPHONE_ID,
        ] {
            let selected = controller.select(id.into(), &platform, &store).unwrap();
            assert_eq!(selected.phase, "ready");
            assert_eq!(selected.current_microphone_id.as_deref(), Some(id));
            assert_eq!(selected.target_microphone_id.as_deref(), Some(id));
            assert_eq!(
                store.load().unwrap().target_microphone_id.as_deref(),
                Some(id)
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
            .select_with_routes("physical".into(), &routes, &store)
            .unwrap();
        assert_eq!(selected.phase, "ready");
        assert_eq!(*api.calls.lock().unwrap(), ["select:physical"]);
        assert!(routes.opens.lock().unwrap().is_empty());
        let saved = store.load().unwrap();
        assert_eq!(saved.target_microphone_id.as_deref(), Some("physical"));
        assert_eq!(
            saved.target_microphone_name.as_deref(),
            Some("Name physical")
        );
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
                vec![capture("physical", &[])]
            } else {
                vec![]
            });
            if busy {
                routes.current.lock().unwrap().phase = AudioPhase::Streaming;
            }
            assert!(controller
                .select_with_routes("physical".into(), &routes, &store)
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
        let routes = FakeRoutes::new(vec![capture("physical", &[])]);
        assert!(controller
            .select_with_routes("physical".into(), &routes, &store)
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
        let routes = FakeRoutes::new(vec![capture("physical", &[])]);
        assert_eq!(
            controller
                .select_with_routes("physical".into(), &routes, &blocked)
                .unwrap_err(),
            SETTINGS_SAVE_FAILED
        );
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "ready");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("physical"));
    }

    #[test]
    fn external_default_change_never_overwrites_saved_target() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![capture("target", &[])]);
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
        controller.apply_startup_with_routes(&routes, &store);
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
        controller.apply_startup_with_routes(&routes, &store);
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
        controller.apply_startup_with_routes(&routes, &store);
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
            .select_with_routes("physical".into(), &routes, &store)
            .unwrap();
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.phase, "ready");
        assert_eq!(snapshot.target_microphone_id.as_deref(), Some("physical"));
        assert_eq!(
            snapshot.target_microphone_name.as_deref(),
            Some("Name physical")
        );
    }

    #[test]
    fn startup_applies_once_and_explicit_selection_supersedes_it() {
        let (api, controller, store) = fixture();
        let routes = FakeRoutes::new(vec![
            capture("cable", &["render"]),
            capture("physical", &[]),
        ]);
        controller.apply_startup_with_routes(&routes, &store);
        controller.apply_startup_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:cable"]);

        let (api, controller, store) = fixture();
        controller
            .select_with_routes("physical".into(), &routes, &store)
            .unwrap();
        controller.apply_startup_with_routes(&routes, &store);
        assert_eq!(*api.calls.lock().unwrap(), ["select:physical"]);
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
