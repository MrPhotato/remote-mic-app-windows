//! Bounded, foreground-only deletion through public UI Automation text ranges.
//!
//! TextPattern is read-only: after validating and selecting an exact range we
//! submit one Backspace. There is no clipboard, whole-value replacement, or
//! word-deletion fallback. UIA and SendInput are not an atomic transaction;
//! focus, selection and text are checked again immediately before submission.
//! Call this from a dedicated worker, never the hook, UI, or BLE thread.

const MAX_CONTEXT_UNITS: usize = 2048;

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
        windows_impl::run(cancelled)
    }
    #[cfg(not(windows))]
    {
        let _ = cancelled;
        Err("按标点删除仅支持 Windows。".to_owned())
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{plan_delete, MAX_CONTEXT_UNITS};
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
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    type EditResult<T> = Result<T, &'static str>;
    const OPERATION_BUDGET: Duration = Duration::from_secs(2);

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

    pub(super) fn run(cancelled: &dyn Fn() -> bool) -> Result<(), String> {
        let budget = Budget {
            started: Instant::now(),
            cancelled,
        };
        note("requested", "unknown", "user_requested", budget.started);
        let result = unsafe { execute(&budget) };
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
                Err(match reason {
                    "cancelled" => "按标点删除已取消，未发送删除键。",
                    "operation_timeout" => "读取文本超时，未发送删除键。",
                    "focus_changed" | "caret_changed" | "text_changed" | "selection_changed" => {
                        "输入焦点或文本已改变，已停止按标点删除。"
                    }
                    "context_limit" | "boundary_outside_context" => {
                        "当前段落过长，无法安全确定删除边界。"
                    }
                    "input_failed" => "删除按键未完整发送，请检查输入框。",
                    _ => "当前输入框不支持安全的按标点删除，未发送删除键。",
                }
                .to_owned())
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

    unsafe fn validate_focus(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        foreground: HWND,
        budget: &Budget<'_>,
    ) -> EditResult<()> {
        budget.check()?;
        if GetForegroundWindow() != foreground {
            return Err("focus_changed");
        }
        let focused = uia.GetFocusedElement().map_err(|_| "focus_changed")?;
        if !uia
            .CompareElements(element, &focused)
            .map_err(|_| "focus_changed")?
            .as_bool()
            || !element
                .CurrentHasKeyboardFocus()
                .map_err(|_| "focus_changed")?
                .as_bool()
        {
            return Err("focus_changed");
        }
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
        if pid == 0 || element.CurrentProcessId().map_err(|_| "process_unknown")? as u32 != pid {
            return Err("foreground_process_mismatch");
        }
        budget.check()
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

    struct SelectionRollback<'a> {
        committed: bool,
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
    }

    impl SelectionRollback<'_> {
        unsafe fn restore_if_unchanged(&self) -> EditResult<()> {
            // Cleanup deliberately ignores the operation's cancellation/budget:
            // leaving our wide selection could turn the next ordinary Backspace
            // into a bulk deletion. Each UIA call retains its 300ms RPC timeout.
            if GetForegroundWindow() != self.foreground {
                return Err("cleanup_focus_changed");
            }
            let actual = selected(self.pattern)?;
            if !actual
                .Compare(self.deletion)
                .map_err(|_| "cleanup_range_unknown")?
                .as_bool()
                || text(&actual)? != self.suffix
                || text(self.prefix)? != self.before
            {
                return Err("cleanup_selection_or_text_changed");
            }
            // The focus comparison is the final COM read before changing the
            // selection. A changed input inside the same window is also rejected.
            let focused = self
                .uia
                .GetFocusedElement()
                .map_err(|_| "cleanup_focus_changed")?;
            if !self
                .uia
                .CompareElements(self.element, &focused)
                .map_err(|_| "cleanup_focus_changed")?
                .as_bool()
                || GetForegroundWindow() != self.foreground
            {
                return Err("cleanup_focus_changed");
            }
            self.caret.Select().map_err(|_| "cleanup_restore_failed")
        }
    }

    impl Drop for SelectionRollback<'_> {
        fn drop(&mut self) {
            if self.committed {
                return;
            }
            let outcome = unsafe { self.restore_if_unchanged() };
            note(
                "selection_cleanup",
                if outcome.is_ok() {
                    "submitted"
                } else {
                    "skipped"
                },
                outcome.err().unwrap_or("original_caret_restored"),
                self.started,
            );
        }
    }

    unsafe fn execute(budget: &Budget<'_>) -> EditResult<&'static str> {
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
        let prefix = caret.Clone().map_err(|_| "range_unsupported")?;
        prefix
            .MoveEndpointByUnit(START, TextUnit_Character, -(MAX_CONTEXT_UNITS as i32))
            .map_err(|_| "range_unsupported")?;
        if prefix
            .CompareEndpoints(END, &caret, END)
            .map_err(|_| "range_unsupported")?
            != 0
        {
            return Err("range_unsupported");
        }
        let document = pattern.DocumentRange().map_err(|_| "range_unsupported")?;
        let at_start = prefix
            .CompareEndpoints(START, &document, START)
            .map_err(|_| "range_unsupported")?
            == 0;
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
        };
        deletion.Select().map_err(|_| "selection_failed")?;
        let actual = selected(&pattern)?;
        if !actual
            .Compare(&deletion)
            .map_err(|_| "selection_changed")?
            .as_bool()
            || text(&actual)? != plan.suffix
            || text(&prefix)? != before
        {
            return Err("selection_changed");
        }
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
