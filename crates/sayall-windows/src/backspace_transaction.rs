//! Eager Backspace with an optional, precisely verified double-click transaction.
//!
//! Only public UIA and SendInput are used. Editor text stays in this worker's
//! memory and is never logged. An unsupported or slow provider still gets one
//! ordinary Backspace; it never becomes eligible for compensating input.

use crate::send_input::{KeyChord, KeyCode};
use crate::send_input_windows::SendInputRuntime;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use windows::core::Interface;
use windows::Win32::Foundation::{HWND, VARIANT_FALSE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::{VariantClear, VT_BOOL};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
    IUIAutomationTextEditPattern, IUIAutomationTextPattern, IUIAutomationTextRange,
    IUIAutomationValuePattern, SupportedTextSelection_Single, TextPatternRangeEndpoint_End as END,
    TextPatternRangeEndpoint_Start as START, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
    UIA_IsReadOnlyAttributeId, UIA_TextEditPatternId, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetLastInputInfo, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, LASTINPUTINFO, VK_BACK, VK_CONTROL, VK_LWIN, VK_MENU,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

pub type Report = Arc<dyn Fn(Result<(), String>) + Send + Sync>;
pub type PermitInput = Arc<dyn Fn() -> bool + Send + Sync>;
type EditResult<T> = Result<T, &'static str>;
// Initial ceiling informed by the recorded 155ms new-probe-process first run
// and 63–84ms warm fixture observations, including fixture lookup. These are
// not idle-first production latency measurements. A ready first click does not
// wait for this ceiling; the candidate still requires actual idle validation.
const FIRST_BUDGET: Duration = Duration::from_millis(200);
const VERIFY_BUDGET: Duration = Duration::from_secs(2);
const MAX_UNITS: usize = 2048;

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn note(phase: &str, result: &str, reason: &str, started: Instant) {
    crate::gatt_note(format!(
        "backspace_transaction phase={phase} terminal_result={result} reason={reason} elapsed_ms={}",
        started.elapsed().as_millis()
    ));
}

fn message(reason: &'static str) -> String {
    if reason == "selection_cleanup_unconfirmed" {
        return crate::text_edit::error_message(reason);
    }
    match reason {
        "cancelled" => "退格事务已取消。",
        "busy" => "上一次文本编辑仍在安全清理，请稍后重试。",
        "input_failed" => "退格按键未完整发送，请检查输入框。",
        _ => "当前输入状态无法安全完成双击删除。",
    }
    .to_owned()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ticket {
    Pending,
    Prepared,
    Fallback,
    Cancelled,
}

struct Busy(Arc<AtomicUsize>);
impl Busy {
    fn new(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Completion {
    report: Report,
    _busy: Busy,
}

struct State {
    ticket: Ticket,
    first_report: Option<Report>,
    complete: Option<Completion>,
    complete_requested: bool,
}

struct Transaction {
    started: Instant,
    foreground: usize,
    first_input_tick: Option<u32>,
    cancelled: AtomicBool,
    epoch: Arc<AtomicU64>,
    generation: u64,
    permit_input: PermitInput,
    state: Mutex<State>,
    changed: Condvar,
}

impl Transaction {
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
            || self.epoch.load(Ordering::SeqCst) != self.generation
    }

    fn cancel(&self) {
        // Signal before waiting for the short SendInput gate; no UIA operation
        // holds this mutex. An already accepted pair finishes before return.
        self.cancelled.store(true, Ordering::SeqCst);
        let (first, complete) = {
            let mut state = lock(&self.state);
            if state.ticket == Ticket::Pending {
                state.ticket = Ticket::Cancelled;
            }
            (state.first_report.take(), state.complete.take())
        };
        self.changed.notify_all();
        // A superseded, unsent first click is neither a successful deletion nor
        // a user-facing fault. Only a requested double receives cancellation.
        drop(first);
        if let Some(completion) = complete {
            let report = completion.report.clone();
            drop(completion);
            report(Err(message("cancelled")));
        }
    }

    fn check(&self, started: Instant) -> EditResult<()> {
        if self.cancelled() {
            Err("cancelled")
        } else if started.elapsed() >= VERIFY_BUDGET {
            Err("operation_timeout")
        } else {
            Ok(())
        }
    }

    fn await_complete(&self) -> Option<Completion> {
        let mut state = lock(&self.state);
        loop {
            if self.cancelled() {
                return None;
            }
            if let Some(completion) = state.complete.take() {
                return Some(completion);
            }
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}

pub struct Runtime {
    send: Arc<SendInputRuntime>,
    permit_input: PermitInput,
    active: Mutex<Option<Arc<Transaction>>>,
    epoch: Arc<AtomicU64>,
    busy: Arc<AtomicUsize>,
    stopped: AtomicBool,
}

impl Runtime {
    pub fn new(send: Arc<SendInputRuntime>, permit_input: PermitInput) -> Arc<Self> {
        Arc::new(Self {
            send,
            permit_input,
            active: Mutex::new(None),
            epoch: Arc::new(AtomicU64::new(0)),
            busy: Arc::new(AtomicUsize::new(0)),
            stopped: AtomicBool::new(false),
        })
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst) != 0
    }

    pub fn begin(&self, report: Report) -> Result<(), String> {
        let first_input_tick = last_input_tick();
        let foreground = unsafe { GetForegroundWindow() }.0 as usize;
        let transaction = self.register(foreground, first_input_tick, report)?;
        note("begin", "accepted", "first_requested", transaction.started);
        let fallback = transaction.clone();
        let send = self.send.clone();
        let busy = self.busy.clone();
        if std::thread::Builder::new()
            .name("sayall-backspace-deadline".to_owned())
            .spawn(move || fallback_worker(fallback, send, busy))
            .is_err()
        {
            self.abort(&transaction);
            return Err(message("worker_unavailable"));
        }
        let worker = transaction.clone();
        let send = self.send.clone();
        let busy = self.busy.clone();
        if std::thread::Builder::new()
            .name("sayall-backspace-transaction".to_owned())
            .spawn(move || unsafe { transaction_worker(worker, send, busy) })
            .is_err()
        {
            self.abort(&transaction);
            return Err(message("worker_unavailable"));
        }
        Ok(())
    }

    // Pure registration boundary: no UIA, window lookup, input or callbacks
    // while holding active. Epoch invalidation precedes exposing a successor.
    fn register(
        &self,
        foreground: usize,
        first_input_tick: Option<u32>,
        report: Report,
    ) -> Result<Arc<Transaction>, String> {
        let (previous, candidate) = {
            let mut active = lock(&self.active);
            let previous = active.take();
            let generation = self.epoch.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
            let candidate = if self.stopped.load(Ordering::SeqCst) {
                Err(message("cancelled"))
            } else if self.is_busy() {
                Err(message("busy"))
            } else if foreground == 0 {
                Err(message("foreground_unavailable"))
            } else {
                let transaction = Arc::new(Transaction {
                    started: Instant::now(),
                    foreground,
                    first_input_tick,
                    cancelled: AtomicBool::new(false),
                    epoch: self.epoch.clone(),
                    generation,
                    permit_input: self.permit_input.clone(),
                    state: Mutex::new(State {
                        ticket: Ticket::Pending,
                        first_report: Some(report),
                        complete: None,
                        complete_requested: false,
                    }),
                    changed: Condvar::new(),
                });
                *active = Some(transaction.clone());
                Ok(transaction)
            };
            (previous, candidate)
        };
        if let Some(previous) = previous {
            // Wait for any accepted pair and cancel callbacks outside active.
            // A concurrent successor can already invalidate our own candidate.
            previous.cancel();
            note(
                "cancel",
                "cancelled",
                "generation_superseded",
                previous.started,
            );
        }
        candidate.and_then(|transaction| {
            if transaction.cancelled() {
                Err(message("cancelled"))
            } else {
                Ok(transaction)
            }
        })
    }

    fn abort(&self, transaction: &Arc<Transaction>) {
        {
            let mut active = lock(&self.active);
            if active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, transaction))
            {
                self.epoch.fetch_add(1, Ordering::SeqCst);
                active.take();
            }
        }
        // A stale spawn failure must never cancel a newer registered first.
        transaction.cancel();
    }

    pub fn complete(&self, report: Report) -> Result<(), String> {
        let active = lock(&self.active);
        let transaction = active.clone().ok_or_else(|| message("no_transaction"))?;
        let mut state = lock(&transaction.state);
        if transaction.cancelled() || state.complete_requested {
            return Err(message("cancelled"));
        }
        state.complete_requested = true;
        state.complete = Some(Completion {
            report,
            _busy: Busy::new(&self.busy),
        });
        drop(state);
        drop(active);
        transaction.changed.notify_all();
        Ok(())
    }

    pub fn cancel(&self) {
        self.invalidate(false);
    }

    fn invalidate(&self, stop: bool) {
        let active = {
            let mut active = lock(&self.active);
            if stop {
                self.stopped.store(true, Ordering::SeqCst);
            }
            self.epoch.fetch_add(1, Ordering::SeqCst);
            active.take()
        };
        if let Some(transaction) = active {
            transaction.cancel();
            note(
                "cancel",
                "cancelled",
                "generation_invalidated",
                transaction.started,
            );
        }
    }

    pub fn shutdown(&self) {
        self.invalidate(true);
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

unsafe fn no_held_keys() -> EditResult<()> {
    if [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN, VK_BACK]
        .iter()
        .any(|key| GetAsyncKeyState(i32::from(key.0)) < 0)
    {
        Err("modifier_or_backspace_held")
    } else {
        Ok(())
    }
}

fn submit_first(
    transaction: &Transaction,
    send: &SendInputRuntime,
    busy: &AtomicUsize,
    ticket: Ticket,
) -> EditResult<bool> {
    submit_first_with_activity(transaction, busy, ticket, last_input_tick, || unsafe {
        if GetForegroundWindow().0 as usize != transaction.foreground {
            return Err("focus_changed");
        }
        no_held_keys()?;
        send.tap(KeyChord {
            keys: vec![KeyCode::Backspace],
        })
        .map(|_| ())
        .map_err(|_| "input_failed")
    })
}

// The injected closure includes the actual OS guards and paired submission in
// production. Tests supply a counter only, exercising this same ticket and
// cancellation gate without inspecting a window or sending keyboard input.
fn submit_first_with_activity(
    transaction: &Transaction,
    busy: &AtomicUsize,
    ticket: Ticket,
    current_input_tick: impl FnOnce() -> Option<u32>,
    submit: impl FnOnce() -> EditResult<()>,
) -> EditResult<bool> {
    let mut state = lock(&transaction.state);
    if transaction.cancelled() || state.ticket != Ticket::Pending {
        return Ok(false);
    }
    // A completion queued for this transaction is not a preceding selection.
    let own_busy = usize::from(state.complete.is_some());
    let result = if !(transaction.permit_input)() {
        Err("input_gate_unavailable")
    } else if busy.load(Ordering::SeqCst) > own_busy {
        Err("busy")
    } else if ticket == Ticket::Fallback {
        unchanged_input_activity(transaction.first_input_tick, current_input_tick())
            .and_then(|()| submit())
    } else {
        submit()
    };
    state.ticket = ticket;
    let report = state.first_report.take();
    let activity_invalidated = matches!(
        result,
        Err("input_activity_changed" | "input_activity_unknown")
    );
    let completion = if activity_invalidated {
        transaction.cancelled.store(true, Ordering::SeqCst);
        state.complete.take()
    } else {
        None
    };
    drop(state);
    transaction.changed.notify_all();
    note(
        "first_submit",
        if result.is_ok() {
            "submitted"
        } else {
            "failed"
        },
        result.err().unwrap_or(if ticket == Ticket::Prepared {
            "prepared"
        } else {
            "fallback"
        }),
        transaction.started,
    );
    if let Some(report) = report.filter(|_| !activity_invalidated) {
        report(result.map_err(message));
    }
    if let Some(completion) = completion {
        let report = completion.report.clone();
        drop(completion);
        report(result.map_err(message));
    }
    result.map(|()| true)
}

fn last_input_tick() -> Option<u32> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetLastInputInfo(&mut info) }
        .as_bool()
        .then_some(info.dwTime)
}

fn unchanged_input_activity(before: Option<u32>, current: Option<u32>) -> EditResult<()> {
    match (before, current) {
        (Some(before), Some(current)) if before == current => Ok(()),
        (Some(_), Some(_)) => Err("input_activity_changed"),
        _ => Err("input_activity_unknown"),
    }
}

#[cfg(test)]
fn submit_first_with(
    transaction: &Transaction,
    busy: &AtomicUsize,
    ticket: Ticket,
    submit: impl FnOnce() -> EditResult<()>,
) -> EditResult<bool> {
    submit_first_with_activity(
        transaction,
        busy,
        ticket,
        || transaction.first_input_tick,
        submit,
    )
}

fn cancel_for_prepare_failure(transaction: &Transaction, reason: &str) -> bool {
    let changed = matches!(
        reason,
        "cancelled"
            | "focus_changed"
            | "caret_changed"
            | "text_changed"
            | "selection_changed"
            | "text_or_caret_changed"
            | "caret_outside_document"
            | "modifier_or_backspace_held"
    ) || reason.starts_with("foreground_")
        || reason.starts_with("first_change_");
    if changed {
        note("prepare", "cancelled", reason, transaction.started);
        // Invalidate only this transaction; a late failure must not advance the
        // shared epoch or cancel a successor. This also wakes the deadline wait.
        transaction.cancel();
    }
    changed
}

fn fallback_worker(
    transaction: Arc<Transaction>,
    send: Arc<SendInputRuntime>,
    busy: Arc<AtomicUsize>,
) {
    let mut state = lock(&transaction.state);
    while state.ticket == Ticket::Pending && !transaction.cancelled() {
        let Some(remaining) = FIRST_BUDGET.checked_sub(transaction.started.elapsed()) else {
            break;
        };
        let waited = transaction
            .changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(|e| e.into_inner());
        state = waited.0;
    }
    drop(state);
    let _ = submit_first(&transaction, &send, &busy, Ticket::Fallback);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    prefix: String,
    suffix: String,
}

impl Snapshot {
    fn full(&self) -> String {
        format!("{}{}", self.prefix, self.suffix)
    }
}

fn empty_edit_placeholder(
    is_edit: bool,
    writable: bool,
    value: &[u16],
    document: &[u16],
    single_empty_caret_at_start: bool,
) -> bool {
    is_edit && writable && value.is_empty() && document == [0xfffc] && single_empty_caret_at_start
}

fn first_change(before: &Snapshot, after: &Snapshot) -> EditResult<()> {
    if before.suffix != after.suffix
        || before.prefix.len() <= after.prefix.len()
        || !before.prefix.starts_with(&after.prefix)
    {
        return Err("first_change_not_suffix_deletion");
    }
    let deleted = &before.prefix[after.prefix.len()..];
    let mut scalars = deleted.chars();
    let scalar = scalars.next().ok_or("first_change_not_suffix_deletion")?;
    if scalars.next().is_some() {
        return Err("first_change_not_single_scalar");
    }
    if scalar.is_control() || matches!(scalar, '\u{2028}' | '\u{2029}') {
        return Err("first_change_structure_unsupported");
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Compensation {
    Delete,
    Restore(String),
    Nothing,
}

fn compensation(before: &Snapshot, after: &Snapshot) -> EditResult<Compensation> {
    first_change(before, after)?;
    let retained = crate::text_edit::retained_prefix(&before.prefix)?;
    if retained == after.prefix {
        Ok(Compensation::Nothing)
    } else if after.prefix.starts_with(retained) {
        Ok(Compensation::Delete)
    } else if let Some(missing) = retained.strip_prefix(&after.prefix) {
        if missing
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err("restore_structure_unsupported");
        }
        Ok(Compensation::Restore(missing.to_owned()))
    } else {
        Err("boundary_changed")
    }
}

struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

struct Editor {
    uia: IUIAutomation,
    element: IUIAutomationElement,
    pattern: IUIAutomationTextPattern,
    composition: IUIAutomationTextEditPattern,
}

impl Editor {
    unsafe fn create(transaction: &Transaction) -> EditResult<Self> {
        transaction.check(transaction.started)?;
        let uia2: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "uia_unavailable")?;
        uia2.SetConnectionTimeout(200)
            .map_err(|_| "uia_timeout_unavailable")?;
        uia2.SetTransactionTimeout(200)
            .map_err(|_| "uia_timeout_unavailable")?;
        let uia: IUIAutomation = uia2.cast().map_err(|_| "uia_unavailable")?;
        let element = uia.GetFocusedElement().map_err(|_| "focus_unavailable")?;
        crate::text_edit::verify_focused_edit(
            &uia,
            &element,
            HWND(transaction.foreground as *mut _),
            transaction.started,
            &|| transaction.cancelled(),
        )?;
        let kind = element
            .CurrentControlType()
            .map_err(|_| "control_type_unknown")?;
        if kind != UIA_EditControlTypeId && kind != UIA_DocumentControlTypeId {
            return Err("not_text_editor");
        }
        let pattern: IUIAutomationTextPattern = element
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| "text_pattern_unsupported")?;
        if pattern
            .SupportedTextSelection()
            .map_err(|_| "selection_unsupported")?
            != SupportedTextSelection_Single
        {
            return Err("selection_mode_unsupported");
        }
        let composition = element
            .GetCurrentPatternAs(UIA_TextEditPatternId)
            .map_err(|_| "composition_unknown")?;
        Ok(Self {
            uia,
            element,
            pattern,
            composition,
        })
    }

    unsafe fn focus(&self, transaction: &Transaction, started: Instant) -> EditResult<()> {
        transaction.check(started)?;
        crate::text_edit::verify_focused_edit(
            &self.uia,
            &self.element,
            HWND(transaction.foreground as *mut _),
            started,
            &|| transaction.cancelled(),
        )
    }

    unsafe fn no_composition(&self) -> EditResult<()> {
        // windows-rs' convenience wrapper treats S_OK + null as an error. Here
        // that exact result is the required positive evidence of no composition.
        let table = self.composition.vtable();
        for query in [table.GetActiveComposition, table.GetConversionTarget] {
            let mut raw = std::ptr::null_mut();
            let status = query(self.composition.as_raw(), &mut raw);
            let absent = raw.is_null();
            if !absent {
                drop(IUIAutomationTextRange::from_raw(raw));
            }
            if status.0 != 0 {
                return Err("composition_unknown");
            }
            if !absent {
                return Err("composition_active");
            }
        }
        Ok(())
    }

    unsafe fn snapshot(&self, transaction: &Transaction, started: Instant) -> EditResult<Snapshot> {
        self.snapshot_for_expected_result(transaction, started, false)
    }

    unsafe fn snapshot_for_expected_result(
        &self,
        transaction: &Transaction,
        started: Instant,
        expected_empty: bool,
    ) -> EditResult<Snapshot> {
        self.focus(transaction, started)?;
        self.no_composition()?;
        let selected = self
            .pattern
            .GetSelection()
            .map_err(|_| "selection_unknown")?;
        if selected.Length().map_err(|_| "selection_unknown")? != 1 {
            return Err("selection_not_single");
        }
        let caret = selected.GetElement(0).map_err(|_| "selection_unknown")?;
        if caret
            .CompareEndpoints(START, &caret, END)
            .map_err(|_| "range_unknown")?
            != 0
        {
            return Err("existing_selection");
        }
        let mut read_only = caret
            .GetAttributeValue(UIA_IsReadOnlyAttributeId)
            .map_err(|_| "writable_unknown")?;
        let writable = read_only.Anonymous.Anonymous.vt == VT_BOOL
            && read_only.Anonymous.Anonymous.Anonymous.boolVal == VARIANT_FALSE;
        let _ = VariantClear(&mut read_only);
        if !writable {
            return Err("readonly_or_unknown");
        }
        let document = self.pattern.DocumentRange().map_err(|_| "range_unknown")?;
        if caret
            .CompareEndpoints(START, &document, START)
            .map_err(|_| "range_unknown")?
            < 0
            || caret
                .CompareEndpoints(END, &document, END)
                .map_err(|_| "range_unknown")?
                > 0
        {
            return Err("caret_outside_document");
        }
        // Observed in our own empty WebView <input>: TextPattern exposes one
        // object-replacement placeholder while writable ValuePattern is empty.
        // Interpret this only when verifying an explicitly empty result, never
        // when preparing a transaction or reading a general embedded object.
        if expected_empty {
            let candidate = document.GetText(2).map_err(|_| "text_unavailable")?;
            if &*candidate == [0xfffc] {
                return self.confirm_empty_placeholder(transaction, started);
            }
        }
        let full = read_text(&document)?;
        let prefix = document.Clone().map_err(|_| "range_unknown")?;
        prefix
            .MoveEndpointByRange(END, &caret, START)
            .map_err(|_| "range_unknown")?;
        let suffix = document.Clone().map_err(|_| "range_unknown")?;
        suffix
            .MoveEndpointByRange(START, &caret, END)
            .map_err(|_| "range_unknown")?;
        let value = Snapshot {
            prefix: read_text(&prefix)?,
            suffix: read_text(&suffix)?,
        };
        if value.full() != full || read_text(&document)? != full {
            return Err("text_changed");
        }
        let current = self
            .pattern
            .GetSelection()
            .map_err(|_| "selection_unknown")?;
        if current.Length().map_err(|_| "selection_unknown")? != 1
            || !current
                .GetElement(0)
                .map_err(|_| "selection_unknown")?
                .Compare(&caret)
                .map_err(|_| "range_unknown")?
                .as_bool()
        {
            return Err("caret_changed");
        }
        self.no_composition()?;
        self.focus(transaction, started)?;
        Ok(value)
    }

    unsafe fn confirm_empty_placeholder(
        &self,
        transaction: &Transaction,
        started: Instant,
    ) -> EditResult<Snapshot> {
        for _ in 0..2 {
            self.focus(transaction, started)?;
            self.no_composition()?;
            let is_edit = self
                .element
                .CurrentControlType()
                .map_err(|_| "control_type_unknown")?
                == UIA_EditControlTypeId;
            let value: IUIAutomationValuePattern = self
                .element
                .GetCurrentPatternAs(UIA_ValuePatternId)
                .map_err(|_| "empty_value_unconfirmed")?;
            let writable = !value
                .CurrentIsReadOnly()
                .map_err(|_| "writable_unknown")?
                .as_bool();
            let value = value
                .CurrentValue()
                .map_err(|_| "empty_value_unconfirmed")?;
            let document = self.pattern.DocumentRange().map_err(|_| "range_unknown")?;
            let document_text = document.GetText(2).map_err(|_| "text_unavailable")?;
            let selections = self
                .pattern
                .GetSelection()
                .map_err(|_| "selection_unknown")?;
            if selections.Length().map_err(|_| "selection_unknown")? != 1 {
                return Err("selection_not_single");
            }
            let caret = selections.GetElement(0).map_err(|_| "selection_unknown")?;
            let caret_empty_at_start = caret
                .CompareEndpoints(START, &caret, END)
                .map_err(|_| "range_unknown")?
                == 0
                && caret
                    .CompareEndpoints(START, &document, START)
                    .map_err(|_| "range_unknown")?
                    == 0
                && caret.GetText(1).map_err(|_| "text_unavailable")?.is_empty();
            if !empty_edit_placeholder(
                is_edit,
                writable,
                &value,
                &document_text,
                caret_empty_at_start,
            ) {
                return Err("empty_placeholder_unconfirmed");
            }
            self.no_composition()?;
            self.focus(transaction, started)?;
        }
        note(
            "empty_result_verify",
            "passed",
            "writable_empty_value_placeholder_confirmed",
            started,
        );
        Ok(Snapshot {
            prefix: String::new(),
            suffix: String::new(),
        })
    }
}

unsafe fn read_text(range: &IUIAutomationTextRange) -> EditResult<String> {
    let text = range
        .GetText((MAX_UNITS + 1) as i32)
        .map_err(|_| "text_unavailable")?;
    if text.len() > MAX_UNITS {
        return Err("context_limit");
    }
    let text = String::from_utf16(&text).map_err(|_| "unsupported_text_content")?;
    if text
        .chars()
        .any(|c| c == '\u{fffc}' || (c.is_control() && !matches!(c, '\t' | '\r' | '\n')))
    {
        return Err("unsupported_text_content");
    }
    Ok(text)
}

unsafe fn transaction_worker(
    transaction: Arc<Transaction>,
    send: Arc<SendInputRuntime>,
    busy: Arc<AtomicUsize>,
) {
    let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_ok();
    let _com = initialized.then_some(Com);
    let prepared = (|| {
        if !initialized {
            return Err("mta_required");
        }
        let editor = Editor::create(&transaction)?;
        let before = editor.snapshot(&transaction, transaction.started)?;
        if before.prefix.is_empty() {
            return Err("nothing_before_caret");
        }
        if transaction.started.elapsed() >= FIRST_BUDGET {
            return Err("first_deadline");
        }
        if !submit_first(&transaction, &send, &busy, Ticket::Prepared)? {
            return Err("first_not_prepared");
        }
        let started = Instant::now();
        let after = loop {
            let observed = editor.snapshot(&transaction, started)?;
            if observed != before {
                first_change(&before, &observed)?;
                break observed;
            }
            transaction.check(started)?;
            std::thread::yield_now();
        };
        note(
            "first_verify",
            "passed",
            "exact_suffix_deletion",
            transaction.started,
        );
        crate::gatt_note(format!(
            "backspace_transaction phase=first_change terminal_result=passed deleted_utf16_units={} right_context_unchanged=true elapsed_ms={}",
            before.prefix.encode_utf16().count() - after.prefix.encode_utf16().count(),
            transaction.started.elapsed().as_millis()
        ));
        Ok((editor, before, after))
    })();
    if let Err(reason) = &prepared {
        if !cancel_for_prepare_failure(&transaction, reason) {
            note("prepare", "unsupported", reason, transaction.started);
            let _ = submit_first(&transaction, &send, &busy, Ticket::Fallback);
        }
    }
    let Some(completion) = transaction.await_complete() else {
        return;
    };
    let started = Instant::now();
    let result = match prepared {
        Ok((editor, before, after)) => finish(&transaction, &editor, &before, &after, started),
        Err(reason) => Err(reason),
    };
    note(
        "double_complete",
        if result.is_ok() { "passed" } else { "refused" },
        result.err().unwrap_or("exact_result_verified"),
        started,
    );
    let report = completion.report.clone();
    drop(completion);
    report(result.map_err(message));
}

unsafe fn finish(
    transaction: &Transaction,
    editor: &Editor,
    before: &Snapshot,
    after: &Snapshot,
    started: Instant,
) -> EditResult<()> {
    if editor.snapshot(transaction, started)? != *after {
        return Err("text_or_caret_changed");
    }
    no_held_keys()?;
    let target = Snapshot {
        prefix: crate::text_edit::retained_prefix(&before.prefix)?.to_owned(),
        suffix: before.suffix.clone(),
    };
    let plan = compensation(before, after)?;
    note(
        "double_plan",
        "verified",
        match &plan {
            Compensation::Nothing => "boundary_already_reached",
            Compensation::Delete => "boundary_retained_delete_suffix",
            Compensation::Restore(_) => "restore_deleted_boundary",
        },
        started,
    );
    match plan {
        Compensation::Nothing => {}
        Compensation::Delete => {
            // The product operation owns selection cleanup. Keep the Busy lease
            // until it returns, including after cancellation and rollback.
            crate::text_edit::delete_to_previous_punctuation_checked(&|| {
                transaction.cancelled()
                    || !(transaction.permit_input)()
                    || editor.focus(transaction, started).is_err()
                    || editor.no_composition().is_err()
            })?;
        }
        Compensation::Restore(missing) => {
            // Re-read the exact caret/context immediately before the short input
            // gate. Never restore via undo, clipboard, or whole-value mutation.
            if editor.snapshot(transaction, started)? != *after {
                return Err("text_or_caret_changed");
            }
            let _gate = lock(&transaction.state);
            transaction.check(started)?;
            if !(transaction.permit_input)() {
                return Err("input_gate_unavailable");
            }
            if GetForegroundWindow().0 as usize != transaction.foreground {
                return Err("focus_changed");
            }
            no_held_keys()?;
            restore_unicode(&missing)?;
        }
    }
    loop {
        let observed = editor.snapshot_for_expected_result(
            transaction,
            started,
            target.prefix.is_empty() && target.suffix.is_empty(),
        )?;
        if observed == target {
            return Ok(());
        }
        if observed != *after {
            return Err("result_changed");
        }
        transaction.check(started)?;
        std::thread::yield_now();
    }
}

unsafe fn restore_unicode(text: &str) -> EditResult<()> {
    let inputs: Vec<_> = text
        .encode_utf16()
        .flat_map(|unit| {
            [false, true].map(|up| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: unit,
                        dwFlags: if up {
                            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                        } else {
                            KEYEVENTF_UNICODE
                        },
                        ..Default::default()
                    },
                },
            })
        })
        .collect();
    let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize;
    crate::gatt_note(format!(
        "backspace_transaction phase=unicode_submit terminal_result={} requested={} submitted={sent} target_result=unknown",
        if sent == inputs.len() { "submitted" } else { "failed" },
        inputs.len()
    ));
    if sent != inputs.len() {
        // A short batch can end on a DOWN. Release exactly that outstanding
        // Unicode key; do not retry characters or attempt to roll back user text.
        if sent % 2 == 1 {
            let released = SendInput(&inputs[sent..sent + 1], std::mem::size_of::<INPUT>() as i32);
            crate::gatt_note(format!(
                "backspace_transaction phase=unicode_release terminal_result={} requested=1 submitted={released} release_only=true",
                if released == 1 { "submitted" } else { "failed" }
            ));
        }
        return Err("input_failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(prefix: &str, suffix: &str) -> Snapshot {
        Snapshot {
            prefix: prefix.into(),
            suffix: suffix.into(),
        }
    }

    #[test]
    fn ordinary_suffix_deletion_keeps_original_boundary() {
        assert_eq!(
            compensation(
                &snapshot("alpha, bravo", "right"),
                &snapshot("alpha, brav", "right")
            ),
            Ok(Compensation::Delete)
        );
        assert_eq!(
            compensation(&snapshot("alpha, x", ""), &snapshot("alpha, ", "")),
            Ok(Compensation::Delete)
        );
    }

    #[test]
    fn punctuation_first_backspace_restores_only_missing_boundary() {
        assert_eq!(
            compensation(&snapshot("alpha,", "right"), &snapshot("alpha", "right")),
            Ok(Compensation::Restore(",".into()))
        );
        assert_eq!(
            compensation(&snapshot("文字。", ""), &snapshot("文字", "")),
            Ok(Compensation::Restore("。".into()))
        );
    }

    #[test]
    fn restoration_rejects_control_and_paragraph_structure() {
        for prefix in [
            "alpha\r",
            "alpha\n",
            "alpha\r\n",
            "alpha\u{2028}",
            "alpha\u{2029}",
        ] {
            assert!(compensation(&snapshot(prefix, "right"), &snapshot("alpha", "right")).is_err());
        }
        assert_eq!(
            compensation(&snapshot("\talpha,", "right"), &snapshot("", "right")),
            Err("first_change_not_single_scalar")
        );
    }

    #[test]
    fn exact_boundary_and_no_punctuation_are_explicit() {
        assert_eq!(
            compensation(&snapshot("alpha,x", ""), &snapshot("alpha,", "")),
            Ok(Compensation::Nothing)
        );
        assert_eq!(
            compensation(&snapshot("abc", ""), &snapshot("ab", "")),
            Ok(Compensation::Delete)
        );
        assert_eq!(
            compensation(&snapshot("x", ""), &snapshot("", "")),
            Ok(Compensation::Nothing)
        );
    }

    #[test]
    fn complex_changes_and_unchanged_text_are_rejected() {
        for after in [
            snapshot("abc", "right"),
            snapshot("ab", "wrong"),
            snapshot("xbc", "right"),
            snapshot("abcd", "right"),
        ] {
            assert!(compensation(&snapshot("abc", "right"), &after).is_err());
        }
    }

    #[test]
    fn busy_lease_survives_cancel_until_operation_cleanup_finishes() {
        let count = Arc::new(AtomicUsize::new(0));
        let lease = Busy::new(&count);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(lease);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    fn test_transaction(permit: bool, report: Report) -> Arc<Transaction> {
        Arc::new(Transaction {
            started: Instant::now(),
            foreground: 1,
            first_input_tick: Some(1),
            cancelled: AtomicBool::new(false),
            epoch: Arc::new(AtomicU64::new(1)),
            generation: 1,
            permit_input: Arc::new(move || permit),
            state: Mutex::new(State {
                ticket: Ticket::Pending,
                first_report: Some(report),
                complete: None,
                complete_requested: false,
            }),
            changed: Condvar::new(),
        })
    }

    #[test]
    fn prepare_and_fallback_race_claim_only_one_first_ticket() {
        let reports = Arc::new(AtomicUsize::new(0));
        let observed = reports.clone();
        let transaction = test_transaction(
            true,
            Arc::new(move |result| {
                assert!(result.is_ok());
                observed.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let sends = Arc::new(AtomicUsize::new(0));
        let busy = Arc::new(AtomicUsize::new(0));
        let mut workers = vec![];
        for ticket in [Ticket::Prepared, Ticket::Fallback] {
            let transaction = transaction.clone();
            let barrier = barrier.clone();
            let sends = sends.clone();
            let busy = busy.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                submit_first_with(&transaction, &busy, ticket, || {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }));
        }
        barrier.wait();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Ok(false))
                .count(),
            1
        );
        assert_eq!(
            results.iter().filter(|result| **result == Ok(true)).count(),
            1
        );
        assert_eq!(reports.load(Ordering::SeqCst), 1);
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancelled_generation_cannot_submit_late_or_complete_twice() {
        let reports = Arc::new(AtomicUsize::new(0));
        let observed = reports.clone();
        let transaction = test_transaction(
            true,
            Arc::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let sends = AtomicUsize::new(0);
        transaction.cancel();
        transaction.cancel();
        for ticket in [Ticket::Prepared, Ticket::Fallback] {
            assert_eq!(
                submit_first_with(&transaction, &AtomicUsize::new(0), ticket, || {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
                Ok(false)
            );
        }
        assert_eq!(reports.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
        assert!(transaction.await_complete().is_none());
    }

    #[test]
    fn cancel_drops_queued_completion_but_not_running_cleanup_lease() {
        let transaction = test_transaction(true, Arc::new(|_| {}));
        let busy = Arc::new(AtomicUsize::new(0));
        let running = Busy::new(&busy);
        lock(&transaction.state).complete = Some(Completion {
            report: Arc::new(|result| assert!(result.is_err())),
            _busy: Busy::new(&busy),
        });
        transaction.cancel();
        assert_eq!(busy.load(Ordering::SeqCst), 1);
        drop(running);
        assert_eq!(busy.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn successful_fallback_blocks_every_late_prepare() {
        let transaction = test_transaction(true, Arc::new(|result| assert!(result.is_ok())));
        let sends = AtomicUsize::new(0);
        let busy = AtomicUsize::new(0);
        for (ticket, expected) in [
            (Ticket::Fallback, true),
            (Ticket::Prepared, false),
            (Ticket::Prepared, false),
            (Ticket::Fallback, false),
        ] {
            assert_eq!(
                submit_first_with(&transaction, &busy, ticket, || {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
                Ok(expected)
            );
        }
        assert_eq!(sends.load(Ordering::SeqCst), 1);
        assert_eq!(lock(&transaction.state).ticket, Ticket::Fallback);
    }

    fn runtime_with_active(transaction: Arc<Transaction>) -> Arc<Runtime> {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        *lock(&runtime.active) = Some(transaction);
        runtime
    }

    #[test]
    fn complete_queued_before_first_does_not_block_its_own_send() {
        let transaction = test_transaction(true, Arc::new(|result| assert!(result.is_ok())));
        let runtime = runtime_with_active(transaction.clone());
        runtime.complete(Arc::new(|_| {})).unwrap();
        assert!(runtime.is_busy());
        let sends = AtomicUsize::new(0);
        assert_eq!(
            submit_first_with(&transaction, &runtime.busy, Ticket::Prepared, || {
                sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Ok(true)
        );
        assert_eq!(sends.load(Ordering::SeqCst), 1);
        assert!(runtime.is_busy());
        runtime.cancel();
        assert!(!runtime.is_busy());
    }

    #[test]
    fn older_cleanup_busy_blocks_send_even_with_own_completion_queued() {
        let transaction = test_transaction(true, Arc::new(|result| assert!(result.is_err())));
        let runtime = runtime_with_active(transaction.clone());
        let older_cleanup = Busy::new(&runtime.busy);
        runtime.complete(Arc::new(|_| {})).unwrap();
        assert_eq!(runtime.busy.load(Ordering::SeqCst), 2);
        let sends = AtomicUsize::new(0);
        assert_eq!(
            submit_first_with(&transaction, &runtime.busy, Ticket::Fallback, || {
                sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Err("busy")
        );
        drop(older_cleanup);
        // Becoming idle must not revive the denied/consumed first ticket.
        assert_eq!(
            submit_first_with(&transaction, &runtime.busy, Ticket::Prepared, || {
                sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Ok(false)
        );
        assert_eq!(sends.load(Ordering::SeqCst), 0);
        runtime.cancel();
        assert!(!runtime.is_busy());
    }

    #[test]
    fn permit_loss_consumes_first_without_sending_or_retrying() {
        let transaction = test_transaction(false, Arc::new(|result| assert!(result.is_err())));
        let sends = AtomicUsize::new(0);
        let busy = AtomicUsize::new(0);
        assert_eq!(
            submit_first_with(&transaction, &busy, Ticket::Prepared, || {
                sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Err("input_gate_unavailable")
        );
        assert_eq!(
            submit_first_with(&transaction, &busy, Ticket::Fallback, || {
                sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Ok(false)
        );
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancel_waits_for_an_accepted_pair_then_rejects_late_send() {
        use std::sync::mpsc;
        let transaction = test_transaction(true, Arc::new(|result| assert!(result.is_ok())));
        let sends = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let first = transaction.clone();
        let first_sends = sends.clone();
        let sending = std::thread::spawn(move || {
            submit_first_with(&first, &AtomicUsize::new(0), Ticket::Prepared, || {
                first_sends.fetch_add(1, Ordering::SeqCst);
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let cancelling = transaction.clone();
        let (cancelled_tx, cancelled_rx) = mpsc::sync_channel(1);
        let cancel = std::thread::spawn(move || {
            cancelling.cancel();
            cancelled_tx.send(()).unwrap();
        });
        let timeout = Instant::now();
        while !transaction.cancelled() {
            assert!(timeout.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        assert!(matches!(
            cancelled_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release_tx.send(()).unwrap();
        assert_eq!(sending.join().unwrap(), Ok(true));
        cancel.join().unwrap();
        cancelled_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            submit_first_with(&transaction, &AtomicUsize::new(0), Ticket::Fallback, || {
                sends.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Ok(false)
        );
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn empty_document_or_unchanged_first_has_no_compensation_plan() {
        assert_eq!(
            compensation(&snapshot("", ""), &snapshot("", "")),
            Err("first_change_not_suffix_deletion")
        );
        assert_eq!(
            compensation(&snapshot("", "right"), &snapshot("", "right")),
            Err("first_change_not_suffix_deletion")
        );
    }

    #[test]
    fn multi_scalar_deletion_is_never_assumed_to_be_one_backspace() {
        assert_eq!(
            compensation(&snapshot("abc,xyz", "right"), &snapshot("abc", "right")),
            Err("first_change_not_single_scalar")
        );
        assert_eq!(
            compensation(&snapshot("abc👨‍👩‍👧", ""), &snapshot("abc", "")),
            Err("first_change_not_single_scalar")
        );
        assert_eq!(
            compensation(&snapshot("abce\u{301}", ""), &snapshot("abc", "")),
            Err("first_change_not_single_scalar")
        );
    }

    #[test]
    fn one_scalar_emoji_deletion_is_valid_despite_two_utf16_units() {
        let before = snapshot("abc,😀", "right");
        let after = snapshot("abc,", "right");
        assert_eq!(first_change(&before, &after), Ok(()));
        assert_eq!(compensation(&before, &after), Ok(Compensation::Nothing));
        assert_eq!(
            compensation(&snapshot("abc😀", ""), &snapshot("abc", "")),
            Ok(Compensation::Delete)
        );
    }

    #[test]
    fn concurrent_runtime_registration_leaves_only_one_live_generation() {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let mut workers = vec![];
        for _ in 0..2 {
            let runtime = runtime.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                runtime.register(1, Some(1), Arc::new(|_| {}))
            }));
        }
        barrier.wait();
        let registered: Vec<_> = workers
            .into_iter()
            .filter_map(|worker| worker.join().unwrap().ok())
            .collect();
        assert_eq!(
            registered
                .iter()
                .filter(|transaction| !transaction.cancelled())
                .count(),
            1
        );
        let current = lock(&runtime.active).clone().unwrap();
        assert!(registered
            .iter()
            .any(|transaction| Arc::ptr_eq(transaction, &current)));
        let sends = AtomicUsize::new(0);
        for transaction in registered {
            let expected = !transaction.cancelled();
            assert_eq!(
                submit_first_with(&transaction, &runtime.busy, Ticket::Prepared, || {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
                Ok(expected)
            );
        }
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancelled_runtime_generation_cannot_send_after_restart_or_shutdown() {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        let first = runtime.register(1, Some(1), Arc::new(|_| {})).unwrap();
        runtime.cancel();
        let second = runtime.register(1, Some(1), Arc::new(|_| {})).unwrap();
        assert!(first.cancelled());
        assert!(!second.cancelled());
        // A late spawn failure belonging to the first may not cancel second.
        runtime.abort(&first);
        assert!(!second.cancelled());
        runtime.shutdown();
        assert!(second.cancelled());
        assert!(runtime.register(1, Some(1), Arc::new(|_| {})).is_err());
        let sends = AtomicUsize::new(0);
        for transaction in [first, second] {
            assert_eq!(
                submit_first_with(&transaction, &runtime.busy, Ticket::Fallback, || {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
                Ok(false)
            );
        }
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancellation_reports_run_outside_active_lock_and_first_is_silent() {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        let first_reports = Arc::new(AtomicUsize::new(0));
        let first_count = first_reports.clone();
        runtime
            .register(
                1,
                Some(1),
                Arc::new(move |_| {
                    first_count.fetch_add(1, Ordering::SeqCst);
                }),
            )
            .unwrap();
        let weak = Arc::downgrade(&runtime);
        let double_reports = Arc::new(AtomicUsize::new(0));
        let double_count = double_reports.clone();
        runtime
            .complete(Arc::new(move |result| {
                assert!(result.is_err());
                let runtime = weak.upgrade().unwrap();
                assert!(runtime.active.try_lock().is_ok());
                double_count.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        runtime.cancel();
        assert_eq!(first_reports.load(Ordering::SeqCst), 0);
        assert_eq!(double_reports.load(Ordering::SeqCst), 1);
        assert!(!runtime.is_busy());
    }

    #[test]
    fn unconfirmed_selection_cleanup_keeps_its_specific_warning() {
        let warning = message("selection_cleanup_unconfirmed");
        assert!(warning.contains("原光标"));
        assert!(warning.contains("当前选区"));
        assert_ne!(warning, message("text_or_caret_changed"));
        assert!(!message("foreground_unavailable").contains("已保留普通退格"));
    }

    #[test]
    fn empty_placeholder_requires_positive_empty_edit_evidence() {
        assert!(empty_edit_placeholder(true, true, &[], &[0xfffc], true));
        assert!(!empty_edit_placeholder(false, true, &[], &[0xfffc], true));
        assert!(!empty_edit_placeholder(true, false, &[], &[0xfffc], true));
        assert!(!empty_edit_placeholder(true, true, &[], &[0xfffc], false));
        assert!(!empty_edit_placeholder(
            true,
            true,
            &[b'x' as u16],
            &[0xfffc],
            true
        ));
        assert!(!empty_edit_placeholder(
            true,
            true,
            &[0xfffc],
            &[0xfffc],
            true
        ));
    }

    #[test]
    fn general_embedded_objects_or_changed_document_are_not_empty() {
        for document in [
            &[][..],
            &[0xfffc, 0xfffc][..],
            &[0xfffc, b'x' as u16][..],
            &[b'x' as u16, 0xfffc][..],
        ] {
            assert!(!empty_edit_placeholder(true, true, &[], document, true));
        }
    }

    #[test]
    fn fallback_requires_two_successful_identical_activity_ticks() {
        for (before, current, expected) in [
            (Some(42), Some(42), Ok(true)),
            (Some(42), Some(43), Err("input_activity_changed")),
            // Tick counters may move backwards or wrap; inequality is enough.
            (Some(42), Some(41), Err("input_activity_changed")),
            (Some(u32::MAX), Some(0), Err("input_activity_changed")),
            (None, Some(42), Err("input_activity_unknown")),
            (Some(42), None, Err("input_activity_unknown")),
            (None, None, Err("input_activity_unknown")),
        ] {
            let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
            let reports = Arc::new(AtomicUsize::new(0));
            let report_count = reports.clone();
            let transaction = runtime
                .register(
                    1,
                    before,
                    Arc::new(move |result| {
                        assert!(result.is_ok());
                        report_count.fetch_add(1, Ordering::SeqCst);
                    }),
                )
                .unwrap();
            let sends = AtomicUsize::new(0);
            assert_eq!(
                submit_first_with_activity(
                    &transaction,
                    &runtime.busy,
                    Ticket::Fallback,
                    || current,
                    || {
                        sends.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                ),
                expected
            );
            let expected_count = usize::from(expected.is_ok());
            assert_eq!(sends.load(Ordering::SeqCst), expected_count);
            assert_eq!(reports.load(Ordering::SeqCst), expected_count);
            assert_eq!(
                submit_first_with_activity(
                    &transaction,
                    &runtime.busy,
                    Ticket::Prepared,
                    || Some(42),
                    || {
                        sends.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                ),
                Ok(false)
            );
            assert_eq!(sends.load(Ordering::SeqCst), expected_count);
        }
    }

    #[test]
    fn precise_prepared_path_does_not_require_an_activity_tick() {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        let transaction = runtime
            .register(1, None, Arc::new(|result| assert!(result.is_ok())))
            .unwrap();
        let sends = AtomicUsize::new(0);
        assert_eq!(
            submit_first_with_activity(
                &transaction,
                &runtime.busy,
                Ticket::Prepared,
                || panic!("prepared must not read fallback activity"),
                || {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            ),
            Ok(true)
        );
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn prepare_context_change_prevents_deadline_send_without_cancelling_successor() {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        for reason in [
            "focus_changed",
            "caret_changed",
            "text_changed",
            "selection_changed",
            "foreground_ancestry_mismatch",
            "cancelled",
        ] {
            let transaction = runtime
                .register(
                    1,
                    Some(7),
                    Arc::new(|_| panic!("unsent first cancellation must be silent")),
                )
                .unwrap();
            assert!(cancel_for_prepare_failure(&transaction, reason));
            let sends = AtomicUsize::new(0);
            assert_eq!(
                submit_first_with_activity(
                    &transaction,
                    &runtime.busy,
                    Ticket::Fallback,
                    || Some(7),
                    || {
                        sends.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                ),
                Ok(false)
            );
            assert_eq!(sends.load(Ordering::SeqCst), 0);
            let successor = runtime.register(1, Some(8), Arc::new(|_| {})).unwrap();
            assert!(cancel_for_prepare_failure(&transaction, reason));
            assert!(!successor.cancelled());
        }
    }

    #[test]
    fn activity_change_silences_first_but_rejects_requested_double() {
        let runtime = Runtime::new(Arc::new(SendInputRuntime::new()), Arc::new(|| true));
        let transaction = runtime
            .register(
                1,
                Some(1),
                Arc::new(|_| panic!("unsent first cancellation must be silent")),
            )
            .unwrap();
        let double_reports = Arc::new(AtomicUsize::new(0));
        let report_count = double_reports.clone();
        runtime
            .complete(Arc::new(move |result| {
                assert!(result.is_err());
                report_count.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        assert_eq!(
            submit_first_with_activity(
                &transaction,
                &runtime.busy,
                Ticket::Fallback,
                || Some(2),
                || panic!("changed activity must not send")
            ),
            Err("input_activity_changed")
        );
        assert!(transaction.cancelled());
        assert!(!runtime.is_busy());
        assert_eq!(double_reports.load(Ordering::SeqCst), 1);
    }
}
