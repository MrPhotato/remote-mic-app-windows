//! Bounded, foreground-only deletion through public UI Automation text ranges.
//!
//! TextPattern is read-only: after validating and selecting an exact range we
//! submit one Backspace. There is no clipboard, whole-value replacement, or
//! word-deletion fallback. UIA and SendInput are not an atomic transaction;
//! focus, selection and text are checked again immediately before submission.
//! Call this from a dedicated worker, never the hook, UI, or BLE thread.

const MAX_CONTEXT_UNITS: usize = 2048;

#[cfg(windows)]
pub(crate) use windows_impl::{
    run_checked as delete_to_previous_punctuation_checked, verify_focused_edit,
};

#[derive(Debug, PartialEq, Eq)]
struct DeletePlan {
    /// Last punctuation or paragraph break, retained by the deletion.
    boundary: Option<char>,
    suffix: String,
}

fn is_paragraph_break(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\u{2028}' | '\u{2029}')
}

fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '，' | '。'
                | '！'
                | '？'
                | '；'
                | '：'
                | '、'
                | '…'
                | '—'
                | '·'
                | '“'
                | '”'
                | '‘'
                | '’'
                | '「'
                | '」'
                | '『'
                | '』'
                | '（'
                | '）'
                | '【'
                | '】'
                | '《'
                | '》'
                | '〈'
                | '〉'
                | '〔'
                | '〕'
                | '［'
                | '］'
                | '｛'
                | '｝'
                | '．'
                | '｡'
                | '､'
        )
}

/// The supplied text is the entire bounded range immediately before the caret.
/// A truncated prefix without a known boundary must not become "delete all".
fn plan_delete(prefix: &str, reaches_document_start: bool) -> Result<DeletePlan, &'static str> {
    if prefix.encode_utf16().count() > MAX_CONTEXT_UNITS {
        return Err("context_limit");
    }
    let boundary = prefix
        .char_indices()
        .rev()
        .find(|(_, c)| is_paragraph_break(*c) || is_punctuation(*c));
    let (boundary, suffix) = match boundary {
        Some((offset, c)) => (Some(c), &prefix[offset + c.len_utf8()..]),
        None if reaches_document_start => (None, prefix),
        None => return Err("boundary_outside_context"),
    };
    if suffix
        .chars()
        .any(|c| c == '\u{fffc}' || (c.is_control() && c != '\t'))
    {
        return Err("unsupported_text_content");
    }
    Ok(DeletePlan {
        boundary,
        suffix: suffix.to_owned(),
    })
}

/// Retained portion of a complete, bounded prefix before the caret.
pub(crate) fn retained_prefix(prefix: &str) -> Result<&str, &'static str> {
    let plan = plan_delete(prefix, true)?;
    Ok(&prefix[..prefix.len() - plan.suffix.len()])
}

/// Delete back to the closest punctuation or current paragraph start, retaining
/// punctuation. An empty suffix succeeds without injecting any input.
pub fn delete_to_previous_punctuation() -> Result<(), String> {
    delete_to_previous_punctuation_with_cancel(&|| false)
}

/// `cancelled` returns true when the caller's generation is stale or cancelled.
/// It is checked throughout planning and immediately before selection/input.
/// A cancelled operation restores its own selection only if focus, selection,
/// and text are all unchanged. Newer user selection/focus is never overwritten.
pub fn delete_to_previous_punctuation_with_cancel(
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows_impl::run_checked(cancelled).map_err(error_message)
    }
    #[cfg(not(windows))]
    {
        let _ = cancelled;
        Err("按标点删除仅支持 Windows。".to_owned())
    }
}

#[cfg(windows)]
pub(crate) fn error_message(reason: &str) -> String {
    match reason {
        "cancelled" => "按标点删除已取消。",
        "selection_cleanup_unconfirmed" => "未能确认原光标已恢复，请检查当前选区后再继续编辑。",
        "operation_timeout" => "读取文本超时，未发送删除键。",
        "focus_changed" | "caret_changed" | "text_changed" | "selection_changed" => {
            "输入焦点或文本已改变，已停止按标点删除。"
        }
        "context_limit" | "boundary_outside_context" => "当前段落过长，无法安全确定删除边界。",
        "input_failed" => "删除按键未完整发送，请检查输入框。",
        _ => "当前输入框不支持安全的按标点删除，未发送删除键。",
    }
    .to_owned()
}

#[cfg(windows)]
mod windows_impl {
    use super::{plan_delete, MAX_CONTEXT_UNITS};
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    use windows::core::{Interface, BSTR};
    use windows::Win32::Foundation::{HWND, VARIANT_FALSE};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::{VariantClear, VT_BOOL};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
        IUIAutomationTextPattern, IUIAutomationTextPattern2, IUIAutomationTextRange,
        SupportedTextSelection_Single, TextPatternRangeEndpoint_End as END,
        TextPatternRangeEndpoint_Start as START, TextUnit_Character, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId, UIA_IsReadOnlyAttributeId, UIA_TextPattern2Id, UIA_TextPatternId,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_BACK, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, GA_ROOT,
    };

    type EditResult<T> = Result<T, &'static str>;
    const OPERATION_BUDGET: Duration = Duration::from_secs(2);
    const MAX_FOCUS_ANCESTORS: usize = 32;

    struct Budget<'a> {
        started: Instant,
        cancelled: &'a dyn Fn() -> bool,
    }
    impl Budget<'_> {
        fn check(&self) -> EditResult<()> {
            if (self.cancelled)() {
                return Err("cancelled");
            }
            if self.started.elapsed() > OPERATION_BUDGET {
                return Err("operation_timeout");
            }
            Ok(())
        }
    }

    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    fn note(phase: &str, result: &str, reason: &str, started: Instant) {
        crate::gatt_note(format!("text_edit action=delete_to_previous_punctuation phase={phase} terminal_result={result} reason={reason} elapsed_ms={}", started.elapsed().as_millis()));
    }

    pub(crate) fn run_checked(cancelled: &dyn Fn() -> bool) -> Result<(), &'static str> {
        let budget = Budget {
            started: Instant::now(),
            cancelled,
        };
        note("requested", "unknown", "user_requested", budget.started);
        let cleanup_unconfirmed = Cell::new(false);
        let result = unsafe { execute(&budget, &cleanup_unconfirmed) };
        let result = if cleanup_unconfirmed.get() {
            Err("selection_cleanup_unconfirmed")
        } else {
            result
        };
        match result {
            Ok(reason) => {
                note(
                    "completed",
                    if reason == "nothing_to_delete" {
                        "passed"
                    } else {
                        "submitted"
                    },
                    reason,
                    budget.started,
                );
                Ok(())
            }
            Err(reason) => {
                note("completed", "failed", reason, budget.started);
                Err(reason)
            }
        }
    }

    unsafe fn selected(pattern: &IUIAutomationTextPattern) -> EditResult<IUIAutomationTextRange> {
        let ranges = pattern
            .GetSelection()
            .map_err(|_| "selection_unsupported")?;
        if ranges.Length().map_err(|_| "selection_unsupported")? != 1 {
            return Err("selection_not_single");
        }
        ranges.GetElement(0).map_err(|_| "selection_unsupported")
    }

    unsafe fn empty(range: &IUIAutomationTextRange) -> EditResult<bool> {
        Ok(range
            .CompareEndpoints(START, range, END)
            .map_err(|_| "range_unsupported")?
            == 0)
    }

    unsafe fn editable(range: &IUIAutomationTextRange) -> EditResult<()> {
        let mut value = range
            .GetAttributeValue(UIA_IsReadOnlyAttributeId)
            .map_err(|_| "read_only_unknown")?;
        let inner = &value.Anonymous.Anonymous;
        // Do not coerce mixed/not-supported IUnknown tokens to false.
        let writable = inner.vt == VT_BOOL && inner.Anonymous.boolVal == VARIANT_FALSE;
        let _ = VariantClear(&mut value);
        if writable {
            Ok(())
        } else {
            Err("read_only_or_unknown")
        }
    }

    unsafe fn text(range: &IUIAutomationTextRange) -> EditResult<String> {
        let value = range
            .GetText((MAX_CONTEXT_UNITS + 1) as i32)
            .map_err(|_| "text_unsupported")?;
        if value.len() > MAX_CONTEXT_UNITS {
            return Err("context_limit");
        }
        String::from_utf16(&value).map_err(|_| "invalid_unicode")
    }

    unsafe fn same_focused_element(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        foreground: HWND,
    ) -> EditResult<()> {
        if GetForegroundWindow() != foreground {
            return Err("focus_changed");
        }
        let focused = uia.GetFocusedElement().map_err(|_| "focus_unavailable")?;
        if !uia
            .CompareElements(element, &focused)
            .map_err(|_| "focus_unavailable")?
            .as_bool()
            || !element
                .CurrentHasKeyboardFocus()
                .map_err(|_| "focus_unavailable")?
                .as_bool()
        {
            return Err("focus_changed");
        }
        Ok(())
    }

    struct NativeHost {
        root: HWND,
        process_matches: bool,
    }

    // Stop at the nearest HWND-bearing UIA element. An unrelated host must not
    // become acceptable merely because a later ancestor reports another HWND.
    fn verify_first_native_host(
        foreground: HWND,
        budget: &Budget<'_>,
        mut next: impl FnMut() -> EditResult<Option<NativeHost>>,
    ) -> EditResult<usize> {
        for depth in 0..MAX_FOCUS_ANCESTORS {
            budget.check()?;
            let host = next()?;
            budget.check()?;
            if let Some(host) = host {
                if !host.process_matches {
                    return Err("foreground_host_process_mismatch");
                }
                if host.root.0.is_null() {
                    return Err("foreground_host_unavailable");
                }
                if host.root != foreground {
                    return Err("foreground_ancestry_mismatch");
                }
                return Ok(depth);
            }
        }
        Err("foreground_ancestry_limit")
    }

    unsafe fn validate_foreground_ancestry(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        foreground: HWND,
        budget: &Budget<'_>,
    ) -> EditResult<usize> {
        let walker = uia
            .RawViewWalker()
            .map_err(|_| "foreground_ancestry_unavailable")?;
        let mut ancestor = element.clone();
        let mut first = true;
        verify_first_native_host(foreground, budget, || {
            if !first {
                ancestor = walker
                    .GetParentElement(&ancestor)
                    .map_err(|_| "foreground_ancestry_unavailable")?;
                budget.check()?;
            }
            first = false;
            let window = ancestor
                .CurrentNativeWindowHandle()
                .map_err(|_| "foreground_host_unavailable")?;
            if window.0.is_null() {
                return Ok(None);
            }
            if !IsWindow(Some(window)).as_bool() {
                return Err("foreground_host_unavailable");
            }
            let mut native_pid = 0;
            GetWindowThreadProcessId(window, Some(&mut native_pid));
            let provider_pid = ancestor.CurrentProcessId().map_err(|_| "process_unknown")?;
            Ok(Some(NativeHost {
                // GA_ROOT deliberately excludes owner relationships: an owned
                // popup or another top-level window is not the foreground host.
                root: GetAncestor(window, GA_ROOT),
                process_matches: native_pid != 0
                    && provider_pid > 0
                    && provider_pid as u32 == native_pid,
            }))
        })
    }

    unsafe fn validate_focus(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        foreground: HWND,
        budget: &Budget<'_>,
    ) -> EditResult<()> {
        budget.check()?;
        same_focused_element(uia, element, foreground)?;
        if !element
            .CurrentIsEnabled()
            .map_err(|_| "element_unavailable")?
            .as_bool()
        {
            return Err("element_disabled");
        }
        if element
            .CurrentIsPassword()
            .map_err(|_| "password_unknown")?
            .as_bool()
        {
            return Err("password_field");
        }
        let mut pid = 0;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        let element_pid = element.CurrentProcessId().map_err(|_| "process_unknown")?;
        if pid == 0 || element_pid <= 0 {
            return Err("process_unknown");
        }
        if element_pid as u32 != pid {
            // Hosted editors (for example WebView2) can expose focused UIA
            // elements from a different process. Prove their window ancestry
            // instead of trusting arbitrary providers or process parentage.
            let depth = validate_foreground_ancestry(uia, element, foreground, budget)?;
            same_focused_element(uia, element, foreground)?;
            let mut current_pid = 0;
            GetWindowThreadProcessId(foreground, Some(&mut current_pid));
            if current_pid != pid {
                return Err("focus_changed");
            }
            budget.check()?;
            crate::gatt_note(format!(
                "text_edit action=delete_to_previous_punctuation phase=focus_binding terminal_result=passed reason=foreground_ancestor_verified ancestor_depth={depth} elapsed_ms={}",
                budget.started.elapsed().as_millis()
            ));
        }
        budget.check()
    }

    /// Share the same hosted-editor identity checks and caller's elapsed budget.
    pub(crate) unsafe fn verify_focused_edit(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        foreground: HWND,
        started: Instant,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), &'static str> {
        validate_focus(uia, element, foreground, &Budget { started, cancelled })
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SelectionState {
        Caret,
        Deletion,
        Other,
    }

    fn wait_for_selection(
        expected: SelectionState,
        budget: &Budget<'_>,
        phase: &str,
        mut observe: impl FnMut(bool) -> EditResult<SelectionState>,
    ) -> EditResult<()> {
        let started = Instant::now();
        let mut pending_count = 0;
        let result = (|| loop {
            budget.check()?;
            let state = observe(pending_count == 0)?;
            budget.check()?;
            if state == SelectionState::Other {
                return Err("selection_changed");
            }
            if state == expected {
                return Ok(());
            }
            pending_count += 1;
            // Each iteration observes the provider again; no guessed settling
            // delay or unchecked sleep replaces an exact selection check.
            std::thread::yield_now();
        })();
        crate::gatt_note(format!(
            "text_edit action=delete_to_previous_punctuation phase={phase} terminal_result={} reason={} target={expected:?} pending_count={pending_count} observed_ms={} elapsed_ms={}",
            if result.is_ok() { "passed" } else { "failed" },
            result.as_ref().err().copied().unwrap_or("selection_observed"),
            started.elapsed().as_millis(),
            budget.started.elapsed().as_millis()
        ));
        result
    }

    struct SelectionRollback<'a> {
        committed: bool,
        selection_observed: bool,
        cleanup_unconfirmed: &'a Cell<bool>,
        uia: &'a IUIAutomation,
        element: &'a IUIAutomationElement,
        pattern: &'a IUIAutomationTextPattern,
        foreground: HWND,
        caret: &'a IUIAutomationTextRange,
        deletion: &'a IUIAutomationTextRange,
        prefix: &'a IUIAutomationTextRange,
        before: &'a str,
        suffix: &'a str,
        started: Instant,
        select_started: Instant,
        select_call_us: u128,
    }

    impl SelectionRollback<'_> {
        unsafe fn observe_selection(
            &self,
            budget: &Budget<'_>,
            log_snapshot: bool,
        ) -> EditResult<SelectionState> {
            budget.check()?;
            let actual = selected(self.pattern)?;
            let selection_read_us = self.select_started.elapsed().as_micros();
            budget.check()?;
            same_focused_element(self.uia, self.element, self.foreground)?;
            budget.check()?;
            let range_matches = actual
                .Compare(self.deletion)
                .map_err(|_| "selection_changed")?
                .as_bool();
            let caret_matches = actual
                .Compare(self.caret)
                .map_err(|_| "selection_changed")?
                .as_bool();
            budget.check()?;
            let actual_text = text(&actual)?;
            let current_prefix = text(self.prefix)?;
            budget.check()?;
            let text_matches = actual_text == self.suffix;
            let context_matches = current_prefix == self.before;
            let selection_empty = empty(&actual)?;
            let deletion_unchanged = text(self.deletion)? == self.suffix;
            budget.check()?;
            let state = if range_matches && text_matches {
                SelectionState::Deletion
            } else if caret_matches && selection_empty && actual_text.is_empty() {
                SelectionState::Caret
            } else {
                SelectionState::Other
            };
            if log_snapshot {
                crate::gatt_note(format!(
                    "text_edit action=delete_to_previous_punctuation phase=selection_verify terminal_result=observed state={state:?} range_matches={range_matches} caret_matches={caret_matches} actual_utf16_units={} suffix_matches={text_matches} prefix_utf16_units={} context_matches={context_matches} actual_empty={selection_empty} deletion_unchanged={deletion_unchanged} select_call_us={} selection_read_us={selection_read_us} elapsed_ms={}",
                    actual_text.encode_utf16().count(),
                    current_prefix.encode_utf16().count(),
                    self.select_call_us,
                    self.started.elapsed().as_millis()
                ));
            }
            if !context_matches || !deletion_unchanged {
                return Err("text_changed");
            }
            // Selection, context and focus must describe one unchanged editor.
            same_focused_element(self.uia, self.element, self.foreground)?;
            budget.check()?;
            Ok(state)
        }

        unsafe fn restore_if_unchanged(&self) -> EditResult<&'static str> {
            // A cancelled Select can still arrive later. Cleanup has its own
            // finite budget and ignores cancellation, but keeps every focus,
            // context and exact-range guard. Each RPC retains its 300ms timeout.
            let cleanup = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            let initial = self.observe_selection(&cleanup, true)?;
            if self.selection_observed && initial == SelectionState::Caret {
                return Ok("original_caret_already_present");
            }
            let mut first = Some(initial);
            wait_for_selection(
                SelectionState::Deletion,
                &cleanup,
                "cleanup_wait_selection",
                |log| match first.take() {
                    Some(state) => Ok(state),
                    None => self.observe_selection(&cleanup, log),
                },
            )?;
            // A newer user selection must not be overwritten while the request
            // settles. Only our still-exact deletion range may be restored.
            match self.observe_selection(&cleanup, false)? {
                SelectionState::Caret => return Ok("original_caret_already_present"),
                SelectionState::Other => return Err("selection_changed"),
                SelectionState::Deletion => {}
            }
            cleanup.check()?;
            self.caret.Select().map_err(|_| "cleanup_restore_failed")?;
            wait_for_selection(
                SelectionState::Caret,
                &cleanup,
                "cleanup_wait_caret",
                |log| self.observe_selection(&cleanup, log),
            )?;
            Ok("original_caret_restored")
        }
    }

    impl Drop for SelectionRollback<'_> {
        fn drop(&mut self) {
            if self.committed {
                return;
            }
            let outcome = unsafe { self.restore_if_unchanged() };
            if let Err(reason) = outcome {
                // Known newer user state is deliberately preserved. Failure to
                // observe the provider (including timeout) is not a confirmed
                // restoration and must reach the caller, not only the log.
                if !matches!(
                    reason,
                    "focus_changed" | "text_changed" | "selection_changed"
                ) {
                    self.cleanup_unconfirmed.set(true);
                }
            }
            note(
                "selection_cleanup",
                if outcome.is_ok() { "passed" } else { "skipped" },
                outcome.unwrap_or_else(|reason| reason),
                self.started,
            );
        }
    }

    unsafe fn execute(
        budget: &Budget<'_>,
        cleanup_unconfirmed: &Cell<bool>,
    ) -> EditResult<&'static str> {
        budget.check()?;
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|_| "mta_required")?;
        let _com = Com;
        let uia2: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "uia_unavailable")?;
        uia2.SetConnectionTimeout(300)
            .map_err(|_| "uia_timeout_unavailable")?;
        uia2.SetTransactionTimeout(300)
            .map_err(|_| "uia_timeout_unavailable")?;
        let uia: IUIAutomation = uia2.cast().map_err(|_| "uia_unavailable")?;
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return Err("foreground_unavailable");
        }
        let element = uia.GetFocusedElement().map_err(|_| "focus_unavailable")?;
        validate_focus(&uia, &element, foreground, budget)?;
        no_held_keys()?;
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
        let caret = selected(&pattern)?;
        if !empty(&caret)? {
            return Err("existing_selection");
        }
        editable(&caret)?;
        if let Ok(pattern2) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
        {
            let mut active = windows::core::BOOL(0);
            let active_caret = pattern2
                .GetCaretRange(&mut active)
                .map_err(|_| "caret_unknown")?;
            if !active.as_bool()
                || !active_caret
                    .Compare(&caret)
                    .map_err(|_| "caret_unknown")?
                    .as_bool()
            {
                return Err("caret_changed");
            }
        }
        budget.check()?;
        let document = pattern.DocumentRange().map_err(|_| "range_unsupported")?;
        if caret
            .CompareEndpoints(START, &document, START)
            .map_err(|_| "range_unsupported")?
            < 0
            || caret
                .CompareEndpoints(END, &document, END)
                .map_err(|_| "range_unsupported")?
                > 0
        {
            return Err("caret_outside_document");
        }
        let prefix = caret.Clone().map_err(|_| "range_unsupported")?;
        let moved = prefix
            .MoveEndpointByUnit(START, TextUnit_Character, -(MAX_CONTEXT_UNITS as i32))
            .map_err(|_| "range_unsupported")?;
        // Some hosted providers navigate beyond this editor into the page's AX
        // text. Bound the cloned range to this TextPattern's own DocumentRange
        // before reading text or searching for punctuation.
        let clamped = prefix
            .CompareEndpoints(START, &document, START)
            .map_err(|_| "range_unsupported")?
            < 0;
        if clamped {
            prefix
                .MoveEndpointByRange(START, &document, START)
                .map_err(|_| "prefix_clamp_failed")?;
        }
        if prefix
            .CompareEndpoints(END, &caret, END)
            .map_err(|_| "range_unsupported")?
            != 0
        {
            return Err("prefix_caret_mismatch");
        }
        let start_order = prefix
            .CompareEndpoints(START, &document, START)
            .map_err(|_| "range_unsupported")?;
        if start_order < 0
            || prefix
                .CompareEndpoints(END, &document, END)
                .map_err(|_| "range_unsupported")?
                > 0
            || prefix
                .CompareEndpoints(START, &prefix, END)
                .map_err(|_| "range_unsupported")?
                > 0
        {
            return Err("prefix_outside_document");
        }
        budget.check()?;
        crate::gatt_note(format!(
            "text_edit action=delete_to_previous_punctuation phase=context_bound terminal_result=passed reason=document_range_verified clamped={clamped} moved_units={moved} elapsed_ms={}",
            budget.started.elapsed().as_millis()
        ));
        let at_start = start_order == 0;
        let before = text(&prefix)?;
        let plan = plan_delete(&before, at_start)?;
        if plan.suffix.is_empty() {
            return Ok("nothing_to_delete");
        }
        let deletion = prefix.Clone().map_err(|_| "range_unsupported")?;
        if let Some(boundary) = plan.boundary {
            let boundary = prefix
                .FindText(&BSTR::from(boundary.to_string()), true, false)
                .map_err(|_| "boundary_lookup_failed")?;
            deletion
                .MoveEndpointByRange(START, &boundary, END)
                .map_err(|_| "range_unsupported")?;
        }
        if empty(&deletion)? || text(&deletion)? != plan.suffix {
            return Err("boundary_mismatch");
        }
        editable(&deletion)?;
        if deletion
            .GetChildren()
            .map_err(|_| "embedded_objects_unknown")?
            .Length()
            .map_err(|_| "embedded_objects_unknown")?
            != 0
        {
            return Err("embedded_objects_present");
        }
        note(
            "planned",
            "unknown",
            "bounded_range_validated",
            budget.started,
        );
        validate_focus(&uia, &element, foreground, budget)?;
        let current = selected(&pattern)?;
        if !empty(&current)?
            || !current
                .Compare(&caret)
                .map_err(|_| "caret_changed")?
                .as_bool()
        {
            return Err("caret_changed");
        }
        if text(&prefix)? != before || text(&deletion)? != plan.suffix {
            return Err("text_changed");
        }
        no_held_keys()?;
        budget.check()?;
        let mut rollback = SelectionRollback {
            committed: false,
            selection_observed: false,
            cleanup_unconfirmed,
            uia: &uia,
            element: &element,
            pattern: &pattern,
            foreground,
            caret: &caret,
            deletion: &deletion,
            prefix: &prefix,
            before: &before,
            suffix: &plan.suffix,
            started: budget.started,
            select_started: Instant::now(),
            select_call_us: 0,
        };
        deletion.Select().map_err(|_| "selection_failed")?;
        rollback.select_call_us = rollback.select_started.elapsed().as_micros();
        wait_for_selection(SelectionState::Deletion, budget, "selection_wait", |log| {
            rollback.observe_selection(budget, log)
        })?;
        rollback.selection_observed = true;
        let actual = selected(&pattern)?;
        editable(&actual)?;
        validate_focus(&uia, &element, foreground, budget)?;
        no_held_keys()?;
        let final_selection = selected(&pattern)?;
        if !final_selection
            .Compare(&deletion)
            .map_err(|_| "selection_changed")?
            .as_bool()
            || text(&final_selection)? != plan.suffix
        {
            return Err("selection_changed");
        }
        // Make the final COM read a focus check, including same-window moves to
        // another editor. Do not perform more provider calls after this fence.
        let final_focus = uia.GetFocusedElement().map_err(|_| "focus_changed")?;
        if !uia
            .CompareElements(&element, &final_focus)
            .map_err(|_| "focus_changed")?
            .as_bool()
        {
            return Err("focus_changed");
        }
        // This is the final cancellation fence before the sole input side effect.
        budget.check()?;
        no_held_keys()?;
        if GetForegroundWindow() != foreground {
            return Err("focus_changed");
        }
        crate::send_input_windows::SendInputRuntime::new()
            .tap(crate::send_input::KeyChord {
                keys: vec![crate::send_input::KeyCode::Backspace],
            })
            .map_err(|_| "input_failed")?;
        rollback.committed = true;
        Ok("backspace_submitted")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;

        fn window(value: usize) -> HWND {
            HWND(value as *mut core::ffi::c_void)
        }

        fn host(root: usize, process_matches: bool) -> Option<NativeHost> {
            Some(NativeHost {
                root: window(root),
                process_matches,
            })
        }

        #[test]
        fn pending_selection_waits_for_the_exact_target() {
            let budget = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            let mut states = [
                SelectionState::Caret,
                SelectionState::Caret,
                SelectionState::Deletion,
            ]
            .into_iter();
            assert_eq!(
                wait_for_selection(
                    SelectionState::Deletion,
                    &budget,
                    "test_selection_wait",
                    |_| Ok(states.next().unwrap())
                ),
                Ok(())
            );
            assert_eq!(states.len(), 0);
        }

        #[test]
        fn pending_selection_rejects_newer_selection_focus_or_text() {
            let budget = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            for rejected in [
                Ok(SelectionState::Other),
                Err("focus_changed"),
                Err("text_changed"),
            ] {
                let expected = rejected.err().unwrap_or("selection_changed");
                let mut states = [
                    Ok(SelectionState::Caret),
                    rejected,
                    Ok(SelectionState::Deletion),
                ]
                .into_iter();
                assert_eq!(
                    wait_for_selection(
                        SelectionState::Deletion,
                        &budget,
                        "test_selection_wait",
                        |_| states.next().unwrap()
                    ),
                    Err(expected)
                );
                assert_eq!(
                    states.len(),
                    1,
                    "a newer user state must never be waited away"
                );
            }
        }

        #[test]
        fn cancelled_pending_select_can_settle_and_confirm_cleanup_independently() {
            let cancelled = Cell::new(false);
            let check = || cancelled.get();
            let budget = Budget {
                started: Instant::now(),
                cancelled: &check,
            };
            assert_eq!(
                wait_for_selection(
                    SelectionState::Deletion,
                    &budget,
                    "test_selection_wait",
                    |_| {
                        cancelled.set(true);
                        Ok(SelectionState::Caret)
                    }
                ),
                Err("cancelled")
            );

            let cleanup = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            let mut late_selection = [SelectionState::Caret, SelectionState::Deletion].into_iter();
            assert_eq!(
                wait_for_selection(
                    SelectionState::Deletion,
                    &cleanup,
                    "test_cleanup_wait_selection",
                    |_| Ok(late_selection.next().unwrap())
                ),
                Ok(())
            );
            assert_eq!(late_selection.len(), 0);
            // A successful caret.Select submission is not yet a restoration.
            let mut restoration = [
                SelectionState::Deletion,
                SelectionState::Deletion,
                SelectionState::Caret,
            ]
            .into_iter();
            assert_eq!(
                wait_for_selection(
                    SelectionState::Caret,
                    &cleanup,
                    "test_cleanup_wait_caret",
                    |_| Ok(restoration.next().unwrap())
                ),
                Ok(())
            );
            assert_eq!(restoration.len(), 0);
        }

        #[test]
        fn pending_selection_and_cleanup_have_finite_budgets() {
            let budget = Budget {
                started: Instant::now() - OPERATION_BUDGET - Duration::from_millis(1),
                cancelled: &|| false,
            };
            for expected in [SelectionState::Caret, SelectionState::Deletion] {
                assert_eq!(
                    wait_for_selection(expected, &budget, "test_selection_wait", |_| panic!(
                        "expired budget must not query provider"
                    )),
                    Err("operation_timeout")
                );
            }
        }

        #[test]
        fn cross_process_focus_requires_a_matching_native_ancestor() {
            let budget = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            let mut ancestors = [None, None, host(1, true)].into_iter();
            assert_eq!(
                verify_first_native_host(window(1), &budget, || Ok(ancestors.next().unwrap())),
                Ok(2)
            );
        }

        #[test]
        fn first_foreign_or_invalid_native_host_cannot_be_bypassed() {
            let budget = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            for (root, process_matches, reason) in [
                (2, true, "foreground_ancestry_mismatch"),
                (0, true, "foreground_host_unavailable"),
                (1, false, "foreground_host_process_mismatch"),
            ] {
                let mut ancestors = [None, host(root, process_matches), host(1, true)].into_iter();
                assert_eq!(
                    verify_first_native_host(window(1), &budget, || Ok(ancestors.next().unwrap())),
                    Err(reason)
                );
                assert_eq!(
                    ancestors.len(),
                    1,
                    "must not search past the nearest native host"
                );
            }
        }

        #[test]
        fn missing_or_cyclic_native_ancestry_is_bounded() {
            let budget = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            let mut visits = 0;
            assert_eq!(
                verify_first_native_host(window(1), &budget, || {
                    visits += 1;
                    Ok(None)
                }),
                Err("foreground_ancestry_limit")
            );
            assert_eq!(visits, MAX_FOCUS_ANCESTORS);
        }

        #[test]
        fn native_ancestry_api_failure_is_not_an_identity_fallback() {
            let budget = Budget {
                started: Instant::now(),
                cancelled: &|| false,
            };
            assert_eq!(
                verify_first_native_host(window(1), &budget, || Err(
                    "foreground_ancestry_unavailable"
                )),
                Err("foreground_ancestry_unavailable")
            );
        }

        #[test]
        fn native_ancestry_cancelled_during_read_cannot_pass() {
            let cancelled = Cell::new(false);
            let check = || cancelled.get();
            let budget = Budget {
                started: Instant::now(),
                cancelled: &check,
            };
            assert_eq!(
                verify_first_native_host(window(1), &budget, || {
                    cancelled.set(true);
                    Ok(host(1, true))
                }),
                Err("cancelled")
            );
            assert_eq!(
                verify_first_native_host(window(1), &budget, || panic!("already cancelled")),
                Err("cancelled")
            );
        }

        #[test]
        fn native_ancestry_budget_is_shared_with_the_text_operation() {
            let budget = Budget {
                started: Instant::now() - OPERATION_BUDGET - Duration::from_millis(1),
                cancelled: &|| false,
            };
            assert_eq!(
                verify_first_native_host(window(1), &budget, || panic!("budget already expired")),
                Err("operation_timeout")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_nearest_chinese_and_ascii_punctuation() {
        assert_eq!(
            plan_delete("你好，今天天气很好", true).unwrap(),
            DeletePlan {
                boundary: Some('，'),
                suffix: "今天天气很好".into(),
            }
        );
        assert_eq!(
            plan_delete("first; last part", true).unwrap().suffix,
            " last part"
        );
        assert_eq!(plan_delete("你好，", true).unwrap().suffix, "");
    }

    #[test]
    fn stops_at_paragraph_start_and_never_deletes_line_breaks() {
        for (input, expected) in [
            ("earlier，text\r\ncurrent", "current"),
            ("first\u{2028}second", "second"),
            ("first\u{2029}", ""),
            ("single paragraph", "single paragraph"),
            ("", ""),
        ] {
            assert_eq!(plan_delete(input, true).unwrap().suffix, expected);
        }
    }

    #[test]
    fn truncated_context_requires_a_visible_boundary() {
        assert_eq!(
            plan_delete("unknown prefix", false),
            Err("boundary_outside_context")
        );
        assert_eq!(
            plan_delete("partial，suffix", false).unwrap().suffix,
            "suffix"
        );
        assert_eq!(
            plan_delete("partial\nparagraph", false).unwrap().suffix,
            "paragraph"
        );
    }

    #[test]
    fn preserves_unicode_without_byte_or_utf16_offset_assumptions() {
        assert_eq!(
            plan_delete("你好，😀e\u{301}天气", true).unwrap().suffix,
            "😀e\u{301}天气"
        );
        assert_eq!(plan_delete("a：𠮷野家", true).unwrap().suffix, "𠮷野家");
    }

    #[test]
    fn rejects_oversized_or_embedded_content() {
        assert_eq!(
            plan_delete(&"a".repeat(MAX_CONTEXT_UNITS + 1), true),
            Err("context_limit")
        );
        assert_eq!(
            plan_delete(&"😀".repeat(MAX_CONTEXT_UNITS / 2 + 1), true),
            Err("context_limit")
        );
        assert_eq!(
            plan_delete("a\u{fffc}b", true),
            Err("unsupported_text_content")
        );
        assert_eq!(plan_delete("a\0b", true), Err("unsupported_text_content"));
    }

    #[test]
    fn punctuation_immediately_before_caret_is_noop() {
        for text in ["。", "你好，", "last!", "line\n", "abc—"] {
            assert!(plan_delete(text, true).unwrap().suffix.is_empty());
        }
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_before_start_never_initializes_uia_or_sends_input() {
        assert!(delete_to_previous_punctuation_with_cancel(&|| true)
            .unwrap_err()
            .contains("取消"));
    }
}
