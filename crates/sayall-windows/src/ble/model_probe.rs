//! Optional model discovery. The voice worker only schedules/cancels work.
//! A completion lease keeps one global slot occupied until cancelled WinRT
//! operations actually finish and their late service results have been closed.
use super::*;
use std::sync::{atomic::AtomicBool, Condvar};

const MAX_FAILURES: u32 = 3;

#[derive(Default)]
pub(super) struct Cancellation {
    cancelled: AtomicBool,
    voice_preempted: AtomicBool,
    gate: Mutex<()>,
    wake: Condvar,
}

impl Cancellation {
    pub(super) fn cancel_for_voice(&self) {
        if !self.cancelled.load(Ordering::Acquire) {
            self.voice_preempted.store(true, Ordering::Release);
        }
        self.cancel();
    }

    fn reason(&self) -> &'static str {
        if self.voice_preempted.load(Ordering::Acquire) {
            "voice_preempted"
        } else {
            "connection_cancelled"
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let _gate = lock(&self.gate);
        self.wake.notify_all();
    }
}

struct Flight {
    generation: u64,
    attempt: u32,
    cancel: Arc<Cancellation>,
}

pub(super) struct Recovery {
    generation: u64,
    attempts: u32,
    failures: u32,
    retry_at: Option<Instant>,
    backoff: ReconnectBackoff,
    active: Option<Flight>,
    eligible: bool,
}

impl Recovery {
    pub(super) fn new() -> Self {
        Self {
            generation: 0,
            attempts: 0,
            failures: 0,
            retry_at: None,
            backoff: ReconnectBackoff::new(RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY),
            active: None,
            eligible: false,
        }
    }

    fn prepare(
        &mut self,
        snapshot: &ConnectionSnapshot,
        generation: u64,
        microphone_opened: bool,
        now: Instant,
    ) -> Option<(u32, Arc<Cancellation>)> {
        if self.generation != generation {
            self.cancel();
            self.generation = generation;
            self.attempts = 0;
            self.failures = 0;
            self.retry_at = None;
            self.backoff.reset();
        }
        self.eligible = snapshot.remote_model == RemoteModel::Unknown
            && snapshot.phase == ConnectionPhase::Ready
            && snapshot.voice_state == VoiceSessionState::Idle
            && !microphone_opened;
        if !self.eligible {
            if matches!(
                snapshot.phase,
                ConnectionPhase::Ready | ConnectionPhase::Streaming | ConnectionPhase::Draining
            ) && (microphone_opened || snapshot.voice_state != VoiceSessionState::Idle)
            {
                if let Some(active) = &self.active {
                    active.cancel.cancel_for_voice();
                }
            } else {
                self.cancel();
            }
            return None;
        }
        if self.active.is_some()
            || self.failures >= MAX_FAILURES
            || self.retry_at.is_some_and(|deadline| now < deadline)
        {
            return None;
        }
        self.attempts += 1;
        self.retry_at = None;
        let cancel = Arc::new(Cancellation::default());
        self.active = Some(Flight {
            generation,
            attempt: self.attempts,
            cancel: Arc::clone(&cancel),
        });
        Some((self.attempts, cancel))
    }

    pub(super) fn tick(
        &mut self,
        session: Option<&mut BleSession>,
        snapshot: &ConnectionSnapshot,
        generation: u64,
        sender: &Sender<WorkerMessage>,
    ) {
        let Some(session) = session else {
            // Update the epoch even between sessions, without launching a read.
            let mut unavailable = snapshot.clone();
            unavailable.phase = ConnectionPhase::Disconnected;
            self.prepare(&unavailable, generation, false, Instant::now());
            return;
        };
        let Some((attempt, cancel)) = self.prepare(
            snapshot,
            generation,
            session.microphone_opened,
            Instant::now(),
        ) else {
            return;
        };
        session.model_cancel = Some(Arc::clone(&cancel));
        let device = session.device.clone(); // Borrowed identity: never Close this shared device.
        let completion = Arc::new(Completion {
            sender: sender.clone(),
            generation,
            attempt,
            model: Mutex::new(None),
        });
        gatt_note(format!(
            "remote_model phase=attempt_started source=device_information attempt={attempt} connection_generation={generation}"
        ));
        let launch = thread::Builder::new()
            .name("sayall-model".to_owned())
            .spawn(move || {
                let context = ReadContext {
                    cancel,
                    deadline: Instant::now() + REQUEST_TIMEOUT,
                    completion,
                };
                let result = read_model(&device, &context);
                match result {
                    Ok(model) => {
                        *lock(&context.completion.model) = Some(model);
                        context.note("model_number", "passed", "recognized", Some(0), Some(0));
                    }
                    Err(error) => context.note(
                        error.step,
                        "failed",
                        error.reason,
                        error.status,
                        error.hresult,
                    ),
                }
                // Context and every acquired service are dropped before Completion
                // sends its result. A pending cancelled callback retains its lease.
            });
        if launch.is_err() {
            gatt_note(format!(
                "remote_model phase=attempt_completed result=failed step=spawn reason=thread_unavailable attempt={attempt} connection_generation={generation}"
            ));
            // The rejected closure drops Completion, which releases the slot.
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.retry_at
            .filter(|_| self.eligible && self.active.is_none())
    }

    pub(super) fn cancel(&self) {
        if let Some(active) = &self.active {
            active.cancel.cancel();
        }
    }

    pub(super) fn complete(
        &mut self,
        generation: u64,
        attempt: u32,
        model: Option<RemoteModel>,
        snapshot: &mut ConnectionSnapshot,
        now: Instant,
    ) {
        if !self
            .active
            .as_ref()
            .is_some_and(|flight| flight.generation == generation && flight.attempt == attempt)
        {
            return;
        }
        let flight = self.active.take().expect("matching model flight");
        if generation != self.generation || snapshot.connection_generation != generation {
            gatt_note(format!(
                "remote_model phase=apply result=ignored reason=stale_connection attempt={attempt} connection_generation={generation}"
            ));
            return;
        }
        if !flight.cancel.cancelled.load(Ordering::Acquire)
            && matches!(snapshot.phase, ConnectionPhase::Ready)
            && snapshot.voice_state == VoiceSessionState::Idle
            && snapshot.remote_model == RemoteModel::Unknown
        {
            if let Some(model @ (RemoteModel::Rc001 | RemoteModel::Rc003)) = model {
                snapshot.remote_model = model;
                self.retry_at = None;
                gatt_note(format!(
                    "remote_model phase=apply result=passed model={} attempt={attempt} connection_generation={generation}",
                    if model == RemoteModel::Rc003 { "rc003" } else { "rc001" }
                ));
                return;
            }
        }
        // Voice use is expected, not a failed model read. Keep a monotonic
        // request id for stale-result isolation, but charge only actual failures.
        let cancelled = flight.cancel.cancelled.load(Ordering::Acquire);
        if !cancelled {
            self.failures += 1;
        }
        if self.failures < MAX_FAILURES {
            let delay = if cancelled {
                RECONNECT_BASE_DELAY
            } else {
                self.backoff.schedule_next().1
            };
            self.retry_at = Some(now + delay);
            gatt_note(format!(
                "remote_model phase=retry_scheduled attempt={} failures={} reason={} delay_ms={} connection_generation={generation}",
                self.attempts + 1, self.failures,
                if cancelled { flight.cancel.reason() } else { "read_failed" },
                delay.as_millis()
            ));
        } else {
            self.retry_at = None;
            gatt_note(format!(
                "remote_model phase=exhausted result=unknown failures={MAX_FAILURES} connection_generation={generation}"
            ));
        }
    }
}

impl Drop for Recovery {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct Completion {
    sender: Sender<WorkerMessage>,
    generation: u64,
    attempt: u32,
    model: Mutex<Option<RemoteModel>>,
}

impl Drop for Completion {
    fn drop(&mut self) {
        let _ = self.sender.send(WorkerMessage::ModelRead {
            connection_generation: self.generation,
            attempt: self.attempt,
            model: *lock(&self.model),
        });
    }
}

struct ReadError {
    step: &'static str,
    reason: &'static str,
    status: Option<i32>,
    hresult: Option<i32>,
}

impl ReadError {
    fn reason(step: &'static str, reason: &'static str) -> Self {
        Self {
            step,
            reason,
            status: None,
            hresult: None,
        }
    }
    fn windows(step: &'static str, error: windows::core::Error) -> Self {
        Self {
            step,
            reason: "windows_api",
            status: None,
            hresult: Some(error.code().0),
        }
    }
}

struct ReadContext {
    cancel: Arc<Cancellation>,
    deadline: Instant,
    completion: Arc<Completion>,
}

impl ReadContext {
    fn note(
        &self,
        step: &str,
        result: &str,
        reason: &str,
        status: Option<i32>,
        hresult: Option<i32>,
    ) {
        gatt_note(format!(
            "remote_model phase=read step={step} result={result} reason={reason} status={} hresult={} attempt={} connection_generation={}",
            status.map_or_else(|| "none".to_owned(), |value| value.to_string()),
            hresult.map_or_else(|| "none".to_owned(), |value| format!("0x{:08X}", value as u32)),
            self.completion.attempt, self.completion.generation,
        ));
    }

    fn check(&self, step: &'static str) -> Result<(), ReadError> {
        if self.cancel.cancelled.load(Ordering::Acquire) {
            return Err(ReadError::reason(step, self.cancel.reason()));
        }
        if Instant::now() >= self.deadline {
            return Err(ReadError::reason(step, "timeout"));
        }
        Ok(())
    }

    fn wait<T: Send + 'static>(
        &self,
        step: &'static str,
        register: impl FnOnce(
            Box<dyn FnOnce(windows::core::Result<T>) + Send>,
        ) -> windows::core::Result<()>,
        cancel_operation: impl FnOnce() -> windows::core::Result<()>,
    ) -> Result<T, ReadError> {
        let result = Arc::new(Mutex::new(None));
        let callback_result = Arc::clone(&result);
        let cancellation = Arc::clone(&self.cancel);
        let registered = register(Box::new(move |value| {
            let _gate = lock(&cancellation.gate);
            *lock(&callback_result) = Some(value);
            cancellation.wake.notify_all();
        }));
        if let Err(error) = registered {
            let _ = cancel_operation();
            return Err(ReadError::windows(step, error));
        }
        let mut gate = lock(&self.cancel.gate);
        loop {
            if let Err(error) = self.check(step) {
                drop(gate);
                let cancelled = cancel_operation();
                self.note(
                    step,
                    if cancelled.is_ok() {
                        "passed"
                    } else {
                        "failed"
                    },
                    "cancel_requested",
                    None,
                    Some(cancelled.err().map_or(0, |e| e.code().0)),
                );
                return Err(error);
            }
            if let Some(result) = lock(&result).take() {
                return result.map_err(|error| ReadError::windows(step, error));
            }
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            gate = self
                .cancel
                .wake
                .wait_timeout(gate, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }
}

// Close every discovered Device Information service, including ambiguous or
// late results. This guard never owns the session's BluetoothLEDevice/ATVV service.
struct OwnedServices(
    windows::Devices::Bluetooth::GenericAttributeProfile::GattDeviceServicesResult,
);
impl Drop for OwnedServices {
    fn drop(&mut self) {
        if let Ok(services) = self.0.Services() {
            if let Ok(count) = services.Size() {
                for index in 0..count {
                    if let Ok(service) = services.GetAt(index) {
                        if let Err(error) = service.Close() {
                            gatt_note(format!(
                                "remote_model phase=cleanup result=failed step=service_close hresult=0x{:08X}",
                                error.code().0 as u32
                            ));
                        }
                    }
                }
            }
        }
    }
}

macro_rules! read_operation {
    ($context:expr, $step:literal, $operation:expr, $map:expr) => {{
        let context = $context;
        context.check($step)?;
        let operation = $operation.map_err(|error| ReadError::windows($step, error))?;
        let close_operation = operation.clone();
        let lease = Arc::clone(&context.completion);
        context.wait($step, |complete| operation.when(move |result| {
            let result = result.map($map);
            if let Err(error) = close_operation.Close() {
                gatt_note(format!(
                    "remote_model phase=cleanup result=failed step=operation_close hresult=0x{:08X}",
                    error.code().0 as u32
                ));
            }
            complete(result);
            drop(lease);
        }), || operation.Cancel())?
    }};
}

fn check_status(
    status: GattCommunicationStatus,
    step: &'static str,
    context: &ReadContext,
) -> Result<(), ReadError> {
    if status == GattCommunicationStatus::Success {
        context.note(step, "passed", "completed", Some(status.0), Some(0));
        Ok(())
    } else {
        Err(ReadError {
            step,
            reason: "gatt_status",
            status: Some(status.0),
            hresult: Some(0),
        })
    }
}

fn read_model(device: &BluetoothLEDevice, context: &ReadContext) -> Result<RemoteModel, ReadError> {
    context.check("initialize")?;
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|error| ReadError::windows("initialize", error))?;
    let _apartment = WinRtApartment;
    let services = read_operation!(
        context,
        "service_discovery",
        device.GetGattServicesForUuidWithCacheModeAsync(
            DEVICE_INFORMATION_SERVICE_UUID,
            BluetoothCacheMode::Uncached
        ),
        OwnedServices
    );
    check_status(
        services
            .0
            .Status()
            .map_err(|e| ReadError::windows("service_status", e))?,
        "service_discovery",
        context,
    )?;
    let list = services
        .0
        .Services()
        .map_err(|e| ReadError::windows("service_list", e))?;
    if list
        .Size()
        .map_err(|e| ReadError::windows("service_count", e))?
        != 1
    {
        return Err(ReadError::reason(
            "service_discovery",
            "missing_or_ambiguous",
        ));
    }
    let service = list
        .GetAt(0)
        .map_err(|e| ReadError::windows("service_get", e))?;
    let characteristics = read_operation!(
        context,
        "characteristic_discovery",
        service.GetCharacteristicsForUuidWithCacheModeAsync(
            MODEL_NUMBER_UUID,
            BluetoothCacheMode::Uncached
        ),
        |result| result
    );
    check_status(
        characteristics
            .Status()
            .map_err(|e| ReadError::windows("characteristic_status", e))?,
        "characteristic_discovery",
        context,
    )?;
    let list = characteristics
        .Characteristics()
        .map_err(|e| ReadError::windows("characteristic_list", e))?;
    if list
        .Size()
        .map_err(|e| ReadError::windows("characteristic_count", e))?
        != 1
    {
        return Err(ReadError::reason(
            "characteristic_discovery",
            "missing_or_ambiguous",
        ));
    }
    let characteristic = list
        .GetAt(0)
        .map_err(|e| ReadError::windows("characteristic_get", e))?;
    let value = read_operation!(
        context,
        "model_number_read",
        characteristic.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached),
        |result| result
    );
    check_status(
        value
            .Status()
            .map_err(|e| ReadError::windows("read_status", e))?,
        "model_number_read",
        context,
    )?;
    let buffer = value
        .Value()
        .map_err(|e| ReadError::windows("read_value", e))?;
    let bytes = buffer_to_vec(&buffer).map_err(|e| ReadError::windows("decode_buffer", e))?;
    let text =
        String::from_utf8(bytes).map_err(|_| ReadError::reason("model_number", "invalid_utf8"))?;
    crate::remote_model_from_model_number(&text)
        .ok_or_else(|| ReadError::reason("model_number", "unrecognized"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(generation: u64) -> ConnectionSnapshot {
        ConnectionSnapshot {
            phase: ConnectionPhase::Ready,
            connection_generation: generation,
            ..ConnectionSnapshot::default()
        }
    }

    #[test]
    fn cold_connection_waits_for_capabilities_then_first_failure_recovers_without_restart() {
        let mut recovery = Recovery::new();
        let mut snapshot = ready(7);
        let now = Instant::now();
        snapshot.phase = ConnectionPhase::AwaitingCapabilities;
        assert!(recovery.prepare(&snapshot, 7, false, now).is_none());
        snapshot.phase = ConnectionPhase::Ready;
        assert_eq!(recovery.prepare(&snapshot, 7, false, now).unwrap().0, 1);
        assert!(recovery.prepare(&snapshot, 7, false, now).is_none());
        recovery.complete(7, 1, None, &mut snapshot, now);
        assert!(recovery.prepare(&snapshot, 7, false, now).is_none());
        let retry = now + RECONNECT_BASE_DELAY;
        assert_eq!(recovery.prepare(&snapshot, 7, false, retry).unwrap().0, 2);
        recovery.complete(7, 2, Some(RemoteModel::Rc003), &mut snapshot, retry);
        assert_eq!(snapshot.remote_model, RemoteModel::Rc003);
        assert!(recovery.prepare(&snapshot, 7, false, retry).is_none());
    }

    #[test]
    fn three_failures_exhaust_budget_without_hot_loop_or_unbounded_retry() {
        let mut recovery = Recovery::new();
        let mut snapshot = ready(3);
        let mut now = Instant::now();
        for attempt in 1..=MAX_FAILURES {
            assert_eq!(
                recovery.prepare(&snapshot, 3, false, now).unwrap().0,
                attempt
            );
            recovery.complete(3, attempt, None, &mut snapshot, now);
            assert!(recovery.prepare(&snapshot, 3, false, now).is_none());
            now += RECONNECT_MAX_DELAY;
        }
        assert!(recovery.deadline().is_none());
        assert!(recovery
            .prepare(&snapshot, 3, false, now + REQUEST_TIMEOUT)
            .is_none());
        assert_eq!(snapshot.remote_model, RemoteModel::Unknown);
    }

    #[test]
    fn disconnect_cancels_and_serializes_next_connection_until_late_result_is_discarded() {
        let mut recovery = Recovery::new();
        let now = Instant::now();
        let mut snapshot = ready(4);
        let (_, cancel) = recovery.prepare(&snapshot, 4, false, now).unwrap();
        snapshot.phase = ConnectionPhase::Disconnected;
        assert!(recovery.prepare(&snapshot, 5, false, now).is_none());
        assert!(cancel.cancelled.load(Ordering::Acquire));
        assert_eq!(cancel.reason(), "connection_cancelled");
        snapshot = ready(6);
        assert!(recovery.prepare(&snapshot, 6, false, now).is_none());
        recovery.complete(4, 1, Some(RemoteModel::Rc003), &mut snapshot, now);
        assert_eq!(snapshot.remote_model, RemoteModel::Unknown);
        assert_eq!(recovery.prepare(&snapshot, 6, false, now).unwrap().0, 1);
    }

    #[test]
    fn voice_open_cancels_probe_and_waits_for_idle_without_using_voice_generation() {
        let mut recovery = Recovery::new();
        let now = Instant::now();
        let mut snapshot = ready(8);
        let (_, cancel) = recovery.prepare(&snapshot, 8, false, now).unwrap();
        assert!(recovery.prepare(&snapshot, 8, true, now).is_none());
        assert!(cancel.cancelled.load(Ordering::Acquire));
        assert_eq!(cancel.reason(), "voice_preempted");
        snapshot.phase = ConnectionPhase::Streaming;
        snapshot.voice_state = VoiceSessionState::Streaming;
        snapshot.generation = 42;
        recovery.complete(8, 1, Some(RemoteModel::Rc003), &mut snapshot, now);
        assert_eq!(snapshot.remote_model, RemoteModel::Unknown);
        assert!(recovery
            .prepare(&snapshot, 8, false, now + RECONNECT_MAX_DELAY)
            .is_none());
        assert!(recovery.deadline().is_none());
        snapshot.phase = ConnectionPhase::Ready;
        snapshot.voice_state = VoiceSessionState::Idle;
        assert_eq!(
            recovery
                .prepare(&snapshot, 8, false, now + RECONNECT_MAX_DELAY)
                .unwrap()
                .0,
            2
        );
    }

    #[test]
    fn three_voice_preemptions_do_not_exhaust_failure_budget() {
        let mut recovery = Recovery::new();
        let mut now = Instant::now();
        let mut snapshot = ready(11);
        for attempt in 1..=3 {
            let (id, cancel) = recovery.prepare(&snapshot, 11, false, now).unwrap();
            assert_eq!(id, attempt);
            cancel.cancel_for_voice();
            snapshot.phase = ConnectionPhase::Streaming;
            snapshot.voice_state = VoiceSessionState::Streaming;
            recovery.complete(11, attempt, None, &mut snapshot, now);
            assert_eq!(recovery.failures, 0);
            assert!(recovery.prepare(&snapshot, 11, true, now).is_none());
            now += RECONNECT_BASE_DELAY;
            snapshot.phase = ConnectionPhase::Ready;
            snapshot.voice_state = VoiceSessionState::Idle;
        }
        assert_eq!(recovery.prepare(&snapshot, 11, false, now).unwrap().0, 4);
        recovery.complete(11, 4, Some(RemoteModel::Rc003), &mut snapshot, now);
        assert_eq!(snapshot.remote_model, RemoteModel::Rc003);
    }

    #[test]
    fn generic_name_never_enables_rc003_and_confirmed_rc001_is_preserved() {
        assert_eq!(remote_model_from_name("MI RC"), RemoteModel::Unknown);
        let mut recovery = Recovery::new();
        let now = Instant::now();
        let mut snapshot = ready(9);
        recovery.prepare(&snapshot, 9, false, now).unwrap();
        recovery.complete(9, 1, Some(RemoteModel::Rc001), &mut snapshot, now);
        assert_eq!(snapshot.remote_model, RemoteModel::Rc001);
        assert!(recovery.prepare(&snapshot, 9, false, now).is_none());
    }

    #[test]
    fn cancelled_wait_is_bounded_and_late_result_cleanup_precedes_completion() {
        struct Resource(Arc<AtomicBool>);
        impl Drop for Resource {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let (sender, receiver) = mpsc::channel();
        let context = ReadContext {
            cancel: Arc::new(Cancellation::default()),
            deadline: Instant::now() + REQUEST_TIMEOUT,
            completion: Arc::new(Completion {
                sender,
                generation: 2,
                attempt: 1,
                model: Mutex::new(None),
            }),
        };
        context.cancel.cancel();
        let closed = Arc::new(AtomicBool::new(false));
        let os_cancelled = AtomicBool::new(false);
        let mut callback = None;
        let lease = Arc::clone(&context.completion);
        let result = context.wait::<Resource>(
            "fixture",
            |complete| {
                callback = Some(complete);
                Ok(())
            },
            || {
                os_cancelled.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(ReadError {
                reason: "connection_cancelled",
                ..
            })
        ));
        assert!(os_cancelled.load(Ordering::Acquire));
        drop(context);
        assert!(receiver.try_recv().is_err());
        callback.take().unwrap()(Ok(Resource(Arc::clone(&closed))));
        assert!(closed.load(Ordering::Acquire));
        drop(lease);
        assert!(matches!(
            receiver.try_recv(),
            Ok(WorkerMessage::ModelRead {
                connection_generation: 2,
                ..
            })
        ));
    }

    #[test]
    fn stalled_operation_uses_existing_request_budget_and_is_cancelled() {
        let (sender, _receiver) = mpsc::channel();
        let context = ReadContext {
            cancel: Arc::new(Cancellation::default()),
            deadline: Instant::now(),
            completion: Arc::new(Completion {
                sender,
                generation: 1,
                attempt: 1,
                model: Mutex::new(None),
            }),
        };
        let cancelled = AtomicBool::new(false);
        let result = context.wait::<()>(
            "fixture",
            |_complete| Ok(()),
            || {
                cancelled.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(ReadError {
                reason: "timeout",
                ..
            })
        ));
        assert!(cancelled.load(Ordering::Acquire));
    }
}
