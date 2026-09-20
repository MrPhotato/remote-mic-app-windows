//! Read-only UIA timing probe for the explicitly identified SayAll fixture only.
//! Usage: text_edit_preflight_probe --pid <pid> --hwnd <decimal-or-0x-handle>
//! Never changes focus, selection or text; outputs metadata and timings only.

#[cfg(not(windows))]
fn main() {
    eprintln!("windows_required");
}

#[cfg(windows)]
fn main() {
    if let Err(code) = probe::run() {
        println!("{}", serde_json::json!({"type":"probe_error", "code":code}));
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod probe {
    use serde_json::{json, Map, Value};
    use std::ffi::c_void;
    use std::time::{Duration, Instant};
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, VARIANT_FALSE};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BOOL};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
        IUIAutomationTextEditPattern, IUIAutomationTextPattern, IUIAutomationTextRange,
        SupportedTextSelection_Single, TextPatternRangeEndpoint_End as END,
        TextPatternRangeEndpoint_Start as START, TextUnit_Character, TreeScope_Descendants,
        UIA_AutomationIdPropertyId, UIA_EditControlTypeId, UIA_IsReadOnlyAttributeId,
        UIA_TextEditPatternId, UIA_TextPatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, GA_ROOT,
    };

    type Result<T> = std::result::Result<T, &'static str>;
    const FIXTURE_ID: &str = "sayall-verification-input";
    const FIXTURE_TEXT: &str = "alpha, bravo";
    const CONTEXT_LIMIT: i32 = 2048;
    const SAMPLE_BUDGET: Duration = Duration::from_secs(2);

    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    fn check_budget(start: Instant) -> Result<()> {
        if start.elapsed() > SAMPLE_BUDGET {
            Err("sample_budget_exceeded")
        } else {
            Ok(())
        }
    }
    fn ms(start: Instant) -> f64 {
        start.elapsed().as_secs_f64() * 1000.0
    }
    fn record(timings: &mut Map<String, Value>, name: &str, start: Instant) {
        timings.insert(name.into(), json!(ms(start)));
    }

    unsafe fn native_focus(pid: u32, hwnd: HWND) -> Result<()> {
        let mut actual_pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut actual_pid));
        if !IsWindow(Some(hwnd)).as_bool()
            || actual_pid != pid
            || GetAncestor(hwnd, GA_ROOT) != hwnd
            || GetForegroundWindow() != hwnd
        {
            return Err("own_foreground_changed");
        }
        Ok(())
    }

    unsafe fn verify_process(pid: u32) -> Result<Process> {
        let process = Process(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map_err(|_| "own_process_unavailable")?,
        );
        let mut path = [0u16; 32768];
        let mut length = path.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .map_err(|_| "own_process_unavailable")?;
        let path =
            String::from_utf16(&path[..length as usize]).map_err(|_| "own_process_unavailable")?;
        if !std::path::Path::new(&path).file_name().is_some_and(|name| {
            name.to_string_lossy()
                .eq_ignore_ascii_case("remote-coding.exe")
        }) {
            return Err("not_own_application");
        }
        Ok(process)
    }

    unsafe fn focused_fixture(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        pid: u32,
        hwnd: HWND,
    ) -> Result<()> {
        native_focus(pid, hwnd)?;
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
            return Err("fixture_not_focused");
        }
        native_focus(pid, hwnd)
    }

    unsafe fn host_depth(
        uia: &IUIAutomation,
        element: &IUIAutomationElement,
        hwnd: HWND,
        start: Instant,
    ) -> Result<usize> {
        let walker = uia.RawViewWalker().map_err(|_| "ancestry_unavailable")?;
        let mut current = element.clone();
        for depth in 0..32 {
            check_budget(start)?;
            let native = current
                .CurrentNativeWindowHandle()
                .map_err(|_| "host_unavailable")?;
            if !native.0.is_null() {
                let mut actual_pid = 0;
                GetWindowThreadProcessId(native, Some(&mut actual_pid));
                let provider_pid = current.CurrentProcessId().map_err(|_| "host_unavailable")?;
                if !IsWindow(Some(native)).as_bool()
                    || actual_pid == 0
                    || provider_pid <= 0
                    || actual_pid != provider_pid as u32
                    || GetAncestor(native, GA_ROOT) != hwnd
                {
                    return Err("fixture_host_mismatch");
                }
                check_budget(start)?;
                return Ok(depth);
            }
            current = walker
                .GetParentElement(&current)
                .map_err(|_| "ancestry_unavailable")?;
        }
        Err("ancestry_limit")
    }

    unsafe fn selection(pattern: &IUIAutomationTextPattern) -> Result<IUIAutomationTextRange> {
        let ranges = pattern
            .GetSelection()
            .map_err(|_| "selection_unavailable")?;
        if ranges.Length().map_err(|_| "selection_unavailable")? != 1 {
            return Err("selection_not_single");
        }
        let range = ranges.GetElement(0).map_err(|_| "selection_unavailable")?;
        if range
            .CompareEndpoints(START, &range, END)
            .map_err(|_| "selection_unavailable")?
            != 0
        {
            return Err("selection_not_empty");
        }
        Ok(range)
    }

    unsafe fn expected_text(range: &IUIAutomationTextRange) -> Result<()> {
        // This range belongs only to the exact fixture. Never print its value.
        let value = range
            .GetText((FIXTURE_TEXT.len() + 1) as i32)
            .map_err(|_| "fixture_text_unavailable")?;
        if &*value != FIXTURE_TEXT.encode_utf16().collect::<Vec<_>>().as_slice() {
            return Err("fixture_text_not_expected");
        }
        Ok(())
    }

    unsafe fn sample(pid: u32, hwnd: HWND, index: usize) -> Value {
        let started = Instant::now();
        let mut timings = Map::new();
        let mut facts = Map::new();
        let result = (|| -> Result<()> {
            let stage = Instant::now();
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|_| "mta_failed")?;
            let _com = Com;
            record(&mut timings, "mta_init_ms", stage);
            let stage = Instant::now();
            let uia2: IUIAutomation2 =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| "uia_unavailable")?;
            uia2.SetConnectionTimeout(300)
                .map_err(|_| "uia_timeout_unavailable")?;
            uia2.SetTransactionTimeout(300)
                .map_err(|_| "uia_timeout_unavailable")?;
            let uia: IUIAutomation = uia2.cast().map_err(|_| "uia_unavailable")?;
            record(&mut timings, "uia_init_ms", stage);
            check_budget(started)?;

            let stage = Instant::now();
            native_focus(pid, hwnd)?;
            let root = uia
                .ElementFromHandle(hwnd)
                .map_err(|_| "own_root_unavailable")?;
            let condition = uia
                .CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(FIXTURE_ID))
                .map_err(|_| "fixture_query_failed")?;
            let matches = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|_| "fixture_query_failed")?;
            if matches.Length().map_err(|_| "fixture_query_failed")? != 1 {
                return Err("fixture_not_unique");
            }
            let element = matches.GetElement(0).map_err(|_| "fixture_query_failed")?;
            if element
                .CurrentAutomationId()
                .map_err(|_| "fixture_query_failed")?
                != FIXTURE_ID
                || element
                    .CurrentControlType()
                    .map_err(|_| "fixture_query_failed")?
                    != UIA_EditControlTypeId
                || element
                    .CurrentIsPassword()
                    .map_err(|_| "fixture_query_failed")?
                    .as_bool()
                || !element
                    .CurrentIsEnabled()
                    .map_err(|_| "fixture_query_failed")?
                    .as_bool()
            {
                return Err("fixture_properties_mismatch");
            }
            record(&mut timings, "fixture_lookup_ms", stage);
            let stage = Instant::now();
            focused_fixture(&uia, &element, pid, hwnd)?;
            let depth = host_depth(&uia, &element, hwnd, started)?;
            focused_fixture(&uia, &element, pid, hwnd)?;
            facts.insert("verified_host_depth".into(), json!(depth));
            record(&mut timings, "focus_host_ms", stage);
            check_budget(started)?;

            let stage = Instant::now();
            let pattern: IUIAutomationTextPattern = element
                .GetCurrentPatternAs(UIA_TextPatternId)
                .map_err(|_| "text_pattern_unavailable")?;
            if pattern
                .SupportedTextSelection()
                .map_err(|_| "selection_unavailable")?
                != SupportedTextSelection_Single
            {
                return Err("selection_mode_unsupported");
            }
            let caret = selection(&pattern)?;
            let mut writable = caret
                .GetAttributeValue(UIA_IsReadOnlyAttributeId)
                .map_err(|_| "writable_unknown")?;
            let value = &writable.Anonymous.Anonymous;
            let is_writable = value.vt == VT_BOOL && value.Anonymous.boolVal == VARIANT_FALSE;
            let _ = VariantClear(&mut writable);
            if !is_writable {
                return Err("writable_unknown");
            }
            facts.insert("single_empty_selection".into(), json!(true));
            facts.insert("writable_confirmed".into(), json!(true));
            record(&mut timings, "selection_writable_ms", stage);
            check_budget(started)?;

            let stage = Instant::now();
            let document = pattern
                .DocumentRange()
                .map_err(|_| "document_unavailable")?;
            expected_text(&document)?;
            if caret
                .CompareEndpoints(END, &document, END)
                .map_err(|_| "caret_unavailable")?
                != 0
            {
                return Err("fixture_caret_not_at_end");
            }
            let prefix = caret.Clone().map_err(|_| "range_unavailable")?;
            prefix
                .MoveEndpointByUnit(START, TextUnit_Character, -CONTEXT_LIMIT)
                .map_err(|_| "range_unavailable")?;
            let outside = prefix
                .CompareEndpoints(START, &document, START)
                .map_err(|_| "range_unavailable")?
                < 0;
            if outside {
                prefix
                    .MoveEndpointByRange(START, &document, START)
                    .map_err(|_| "range_unavailable")?;
            }
            if prefix
                .CompareEndpoints(END, &caret, END)
                .map_err(|_| "range_unavailable")?
                != 0
                || prefix
                    .CompareEndpoints(START, &document, START)
                    .map_err(|_| "range_unavailable")?
                    < 0
                || prefix
                    .CompareEndpoints(END, &document, END)
                    .map_err(|_| "range_unavailable")?
                    > 0
            {
                return Err("prefix_outside_fixture");
            }
            expected_text(&prefix)?;
            facts.insert("prefix_needed_clamp".into(), json!(outside));
            facts.insert("fixture_text_matches".into(), json!(true));
            record(&mut timings, "context_clamp_read_ms", stage);
            check_budget(started)?;

            let stage = Instant::now();
            let composition = match element
                .GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId)
            {
                Ok(edit) => {
                    // Public COM ABI preserves S_OK + null, which the generated
                    // non-optional convenience return cannot distinguish.
                    let mut raw: *mut c_void = std::ptr::null_mut();
                    let hr = (edit.vtable().GetActiveComposition)(edit.as_raw(), &mut raw);
                    let present = !raw.is_null();
                    if present {
                        drop(IUIAutomationTextRange::from_raw(raw));
                    }
                    json!({"pattern_available":true, "active_composition_hresult":hr.0,
                           "range_present":present, "inactive_confirmed":hr.0 == 0 && !present})
                }
                Err(error) => json!({"pattern_available":false, "pattern_hresult":error.code().0,
                                     "inactive_confirmed":false}),
            };
            facts.insert("composition".into(), composition);
            record(&mut timings, "composition_ms", stage);
            check_budget(started)?;

            let stage = Instant::now();
            focused_fixture(&uia, &element, pid, hwnd)?;
            let final_caret = selection(&pattern)?;
            if !final_caret
                .Compare(&caret)
                .map_err(|_| "selection_unavailable")?
                .as_bool()
            {
                return Err("fixture_selection_changed");
            }
            expected_text(&document)?;
            check_budget(started)?;
            facts.insert("fixture_unchanged".into(), json!(true));
            record(&mut timings, "final_validation_ms", stage);
            Ok(())
        })();
        json!({"type":"sample", "index":index,
               "mode":if index == 0 {"process_first"} else {"warm"},
               "passed":result.is_ok(), "reason":result.err().unwrap_or("read_only_checks_complete"),
               "total_ms":ms(started), "timings":timings, "facts":facts})
    }

    pub(super) fn run() -> Result<()> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if args.len() != 4 || args[0] != "--pid" || args[2] != "--hwnd" {
            return Err("expected_pid_and_hwnd");
        }
        let pid: u32 = args[1].parse().map_err(|_| "invalid_pid")?;
        let handle = if let Some(value) = args[3].strip_prefix("0x") {
            usize::from_str_radix(value, 16)
        } else {
            args[3].parse()
        }
        .map_err(|_| "invalid_hwnd")?;
        if pid == 0 || handle == 0 {
            return Err("invalid_target");
        }
        let hwnd = HWND(handle as *mut c_void);
        let _process = unsafe { verify_process(pid)? };
        unsafe { native_focus(pid, hwnd)? };
        println!(
            "{}",
            json!({"type":"probe", "read_only":true, "warm_samples":10,
                             "rpc_timeout_ms":300, "sample_budget_ms":2000,
                             "real_idle_claimed":false})
        );
        for index in 0..=10 {
            let result = unsafe { sample(pid, hwnd, index) };
            let passed = result["passed"] == true;
            println!("{result}");
            if !passed {
                return Err("sample_failed_closed");
            }
        }
        Ok(())
    }
}
