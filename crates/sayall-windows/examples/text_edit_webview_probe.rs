//! Opt-in verification of the caller-selected SayAll WebView fixture only.
//! Supply an observed PID and main HWND: --run --pid <pid> --hwnd <hwnd>
//! --case <keep_ascii|keep_boundary|single>. Decimal or 0x HWND is accepted.
//! The controller prepares the fixed value and end caret; this probe never sets
//! a whole value. Output contains only fixed classifications, booleans, lengths,
//! and timings. No process enumeration, arbitrary text, or third-party targets.

#[cfg(not(windows))]
fn main() {
    println!("result=deferred reason=windows_required");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let code = match probe::run() {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(reason) => {
            println!("result=deferred reason={reason}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(windows)]
mod probe {
    use std::path::Path;
    use std::time::{Duration, Instant};
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, WAIT_TIMEOUT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
        IUIAutomationTextPattern, IUIAutomationValuePattern, TextPatternRangeEndpoint_End as END,
        TextPatternRangeEndpoint_Start as START, TreeScope_Descendants, UIA_AutomationIdPropertyId,
        UIA_TextPatternId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_BACK, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
        GA_ROOT,
    };

    type ProbeResult<T> = Result<T, &'static str>;
    const FIXTURE_ID: &str = "sayall-verification-input";

    struct Arguments {
        pid: u32,
        hwnd: HWND,
        case: String,
    }

    fn arguments() -> ProbeResult<Arguments> {
        let mut args = std::env::args().skip(1);
        let (mut run, mut pid, mut hwnd, mut case) = (false, None, None, None);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--run" if !run => run = true,
                "--pid" if pid.is_none() => {
                    pid = Some(
                        args.next()
                            .ok_or("invalid_arguments")?
                            .parse::<u32>()
                            .map_err(|_| "invalid_arguments")?,
                    );
                }
                "--hwnd" if hwnd.is_none() => {
                    let value = args.next().ok_or("invalid_arguments")?;
                    let parsed = if let Some(hex) = value.strip_prefix("0x") {
                        usize::from_str_radix(hex, 16)
                    } else {
                        value.parse::<usize>()
                    }
                    .map_err(|_| "invalid_arguments")?;
                    hwnd = Some(HWND(parsed as *mut _));
                }
                "--case" if case.is_none() => {
                    let value = args.next().ok_or("invalid_arguments")?;
                    if !matches!(
                        value.as_str(),
                        "keep_ascii"
                            | "keep_boundary"
                            | "single"
                            | "cancel_selection"
                            | "fast_single"
                            | "fast_double"
                            | "fast_boundary"
                            | "fast_zh"
                            | "fast_emoji"
                            | "fast_no_punctuation"
                            | "fast_cancel"
                    ) {
                        return Err("invalid_case");
                    }
                    case = Some(value);
                }
                _ => return Err("invalid_arguments"),
            }
        }
        if !run || pid == Some(0) || hwnd.is_some_and(|window| window.0.is_null()) {
            return Err("explicit_target_and_run_required");
        }
        Ok(Arguments {
            pid: pid.ok_or("explicit_target_and_run_required")?,
            hwnd: hwnd.ok_or("explicit_target_and_run_required")?,
            case: case.ok_or("invalid_arguments")?,
        })
    }

    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    unsafe fn owns_window(args: &Arguments, process: &Process) -> bool {
        if WaitForSingleObject(process.0, 0) != WAIT_TIMEOUT
            || !IsWindow(Some(args.hwnd)).as_bool()
            || GetAncestor(args.hwnd, GA_ROOT) != args.hwnd
        {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(args.hwnd, Some(&mut pid));
        pid == args.pid
    }

    unsafe fn own_focus(
        args: &Arguments,
        process: &Process,
        uia: &IUIAutomation,
        fixture: &IUIAutomationElement,
    ) -> bool {
        if !owns_window(args, process) || GetForegroundWindow() != args.hwnd {
            return false;
        }
        let Ok(focused) = uia.GetFocusedElement() else {
            return false;
        };
        uia.CompareElements(fixture, &focused)
            .is_ok_and(|same| same.as_bool())
            && fixture
                .CurrentHasKeyboardFocus()
                .is_ok_and(|focused| focused.as_bool())
            && GetForegroundWindow() == args.hwnd
    }

    unsafe fn fixture_in_main(
        uia: &IUIAutomation,
        main: &IUIAutomationElement,
        fixture: &IUIAutomationElement,
    ) -> ProbeResult<()> {
        let walker = uia
            .RawViewWalker()
            .map_err(|_| "fixture_tree_unavailable")?;
        let mut ancestor = fixture.clone();
        for _ in 0..32 {
            if uia
                .CompareElements(main, &ancestor)
                .map_err(|_| "fixture_tree_unavailable")?
                .as_bool()
            {
                return Ok(());
            }
            ancestor = walker
                .GetParentElement(&ancestor)
                .map_err(|_| "fixture_not_in_main")?;
        }
        Err("fixture_tree_limit")
    }

    unsafe fn end_caret(fixture: &IUIAutomationElement) -> ProbeResult<bool> {
        let pattern: IUIAutomationTextPattern = fixture
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| "text_pattern_unavailable")?;
        let selections = pattern
            .GetSelection()
            .map_err(|_| "selection_unavailable")?;
        if selections.Length().map_err(|_| "selection_unavailable")? != 1 {
            return Ok(false);
        }
        let range = selections
            .GetElement(0)
            .map_err(|_| "selection_unavailable")?;
        let document = pattern
            .DocumentRange()
            .map_err(|_| "document_range_unavailable")?;
        Ok(range
            .CompareEndpoints(START, &range, END)
            .map_err(|_| "caret_unavailable")?
            == 0
            && range
                .CompareEndpoints(END, &document, END)
                .map_err(|_| "caret_unavailable")?
                == 0)
    }

    pub(super) fn run() -> ProbeResult<bool> {
        let args = arguments()?;
        let started = Instant::now();
        unsafe {
            let process = Process(
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    args.pid,
                )
                .map_err(|_| "target_unavailable")?,
            );
            if !owns_window(&args, &process) {
                return Err("target_window_unverified");
            }
            let mut image = vec![0u16; 32768];
            let mut size = image.len() as u32;
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                PWSTR(image.as_mut_ptr()),
                &mut size,
            )
            .map_err(|_| "target_image_unavailable")?;
            let image = String::from_utf16(&image[..size as usize])
                .map_err(|_| "target_image_unavailable")?;
            if !Path::new(&image)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("remote-coding.exe"))
            {
                return Err("target_image_mismatch");
            }
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|_| "com_unavailable")?;
            let _com = Com;
            let uia2: IUIAutomation2 =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| "uia_unavailable")?;
            uia2.SetConnectionTimeout(300)
                .map_err(|_| "uia_timeout_unavailable")?;
            uia2.SetTransactionTimeout(300)
                .map_err(|_| "uia_timeout_unavailable")?;
            let uia: IUIAutomation = uia2.cast().map_err(|_| "uia_unavailable")?;
            let main = uia
                .ElementFromHandle(args.hwnd)
                .map_err(|_| "main_unavailable")?;
            let condition = uia
                .CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(FIXTURE_ID))
                .map_err(|_| "fixture_condition_unavailable")?;
            let matches = main
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|_| "fixture_unavailable")?;
            if matches.Length().map_err(|_| "fixture_unavailable")? != 1 {
                return Err("fixture_not_unique");
            }
            let fixture = matches.GetElement(0).map_err(|_| "fixture_unavailable")?;
            fixture_in_main(&uia, &main, &fixture)?;
            if fixture
                .CurrentIsPassword()
                .map_err(|_| "fixture_state_unavailable")?
                .as_bool()
                || !fixture
                    .CurrentIsEnabled()
                    .map_err(|_| "fixture_state_unavailable")?
                    .as_bool()
            {
                return Err("fixture_not_editable");
            }
            let value: IUIAutomationValuePattern = fixture
                .GetCurrentPatternAs(UIA_ValuePatternId)
                .map_err(|_| "value_pattern_unavailable")?;
            if value
                .CurrentIsReadOnly()
                .map_err(|_| "fixture_state_unavailable")?
                .as_bool()
            {
                return Err("fixture_read_only");
            }
            let (initial, expected) = match args.case.as_str() {
                "keep_ascii" => ("alpha, bravo", "alpha,"),
                "keep_boundary" => ("alpha,", "alpha,"),
                "single" | "fast_single" | "fast_cancel" => ("alpha, bravo", "alpha, brav"),
                "cancel_selection" => ("alpha, bravo", "alpha, bravo"),
                "fast_double" => ("alpha, bravo", "alpha,"),
                "fast_boundary" => ("alpha,", "alpha,"),
                "fast_zh" => ("文字。", "文字。"),
                "fast_emoji" => ("alpha,😀", "alpha,"),
                "fast_no_punctuation" => ("bravo", ""),
                _ => unreachable!(),
            };
            if value
                .CurrentValue()
                .map_err(|_| "fixture_value_unavailable")?
                .to_string()
                != initial
            {
                return Err("fixture_value_mismatch");
            }
            let focus_started = Instant::now();
            let foreground_accepted = SetForegroundWindow(args.hwnd).as_bool();
            while GetForegroundWindow() != args.hwnd
                && focus_started.elapsed() < Duration::from_millis(500)
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            println!(
                "phase=foreground accepted={foreground_accepted} matched={} elapsed_ms={}",
                GetForegroundWindow() == args.hwnd,
                focus_started.elapsed().as_millis()
            );
            if GetForegroundWindow() != args.hwnd || !owns_window(&args, &process) {
                return Err("own_foreground_unverified");
            }
            fixture.SetFocus().map_err(|_| "fixture_focus_refused")?;
            let fixture_focus_started = Instant::now();
            while !own_focus(&args, &process, &uia, &fixture)
                && fixture_focus_started.elapsed() < Duration::from_millis(500)
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            if !own_focus(&args, &process, &uia, &fixture) {
                return Err("own_fixture_focus_unverified");
            }
            if value
                .CurrentValue()
                .map_err(|_| "fixture_value_unavailable")?
                .to_string()
                != initial
                || !end_caret(&fixture)?
            {
                return Err("fixture_value_or_end_caret_changed");
            }
            if [VK_BACK, VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
                .iter()
                .any(|key| GetAsyncKeyState(i32::from(key.0)) < 0)
                || !own_focus(&args, &process, &uia, &fixture)
            {
                return Err("input_precondition_changed");
            }
            let action_started = Instant::now();
            let api_ok = if args.case.starts_with("fast_") {
                use std::sync::{mpsc, Arc};
                let window = args.hwnd.0 as usize;
                let expected_pid = args.pid;
                let process_handle = process.0 .0 as usize;
                let permit = Arc::new(move || {
                    let hwnd = HWND(window as *mut _);
                    let mut pid = 0;
                    GetWindowThreadProcessId(hwnd, Some(&mut pid));
                    GetForegroundWindow() == hwnd
                        && pid == expected_pid
                        && WaitForSingleObject(HANDLE(process_handle as *mut _), 0) == WAIT_TIMEOUT
                });
                let runtime = sayall_windows::backspace_transaction::Runtime::new(
                    Arc::new(sayall_windows::send_input_windows::SendInputRuntime::new()),
                    permit,
                );
                let (first_tx, first_rx) = mpsc::channel();
                runtime
                    .begin(Arc::new(move |result| {
                        let _ = first_tx.send(result);
                    }))
                    .map_err(|_| "first_begin_refused")?;
                let first_ok = first_rx
                    .recv_timeout(Duration::from_secs(3))
                    .is_ok_and(|result| result.is_ok());
                println!(
                    "phase=first_submit ok={first_ok} elapsed_ms={}",
                    action_started.elapsed().as_millis()
                );
                let completed =
                    if first_ok && !matches!(args.case.as_str(), "fast_single" | "fast_cancel") {
                        let (double_tx, double_rx) = mpsc::channel();
                        runtime
                            .complete(Arc::new(move |result| {
                                let _ = double_tx.send(result);
                            }))
                            .map_err(|_| "double_begin_refused")?;
                        double_rx
                            .recv_timeout(Duration::from_secs(5))
                            .is_ok_and(|result| result.is_ok())
                    } else if first_ok && args.case == "fast_cancel" {
                        runtime.cancel();
                        runtime.complete(Arc::new(|_| {})).is_err()
                    } else {
                        first_ok
                    };
                runtime.shutdown();
                let drain = Instant::now();
                while runtime.is_busy() && drain.elapsed() < Duration::from_secs(3) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                first_ok && completed && !runtime.is_busy()
            } else if args.case == "cancel_selection" {
                let cancelled = std::sync::atomic::AtomicBool::new(false);
                let action =
                    sayall_windows::text_edit::delete_to_previous_punctuation_with_cancel(&|| {
                        if !own_focus(&args, &process, &uia, &fixture) {
                            return true;
                        }
                        let selected = fixture
                            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                            .and_then(|pattern| pattern.GetSelection())
                            .and_then(|ranges| ranges.GetElement(0))
                            .and_then(|range| range.CompareEndpoints(START, &range, END));
                        if selected.is_ok_and(|distance| distance != 0) {
                            cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        cancelled.load(std::sync::atomic::Ordering::SeqCst)
                    });
                let cancellation_observed = cancelled.load(std::sync::atomic::Ordering::SeqCst);
                let caret_restored = end_caret(&fixture)?;
                println!("phase=cancel expected_rejection={} cancellation_observed={cancellation_observed} caret_restored={caret_restored}", action.is_err());
                action.is_err() && cancellation_observed && caret_restored
            } else if args.case == "single" {
                sayall_windows::send_input_windows::SendInputRuntime::new()
                    .tap(sayall_windows::send_input::KeyChord {
                        keys: vec![sayall_windows::send_input::KeyCode::Backspace],
                    })
                    .is_ok()
            } else {
                sayall_windows::text_edit::delete_to_previous_punctuation_with_cancel(&|| {
                    !own_focus(&args, &process, &uia, &fixture)
                })
                .is_ok()
            };
            let action_ms = action_started.elapsed().as_millis();
            let observe_started = Instant::now();
            let mut after;
            loop {
                if !own_focus(&args, &process, &uia, &fixture) {
                    println!("phase=verification api_ok={api_ok} focus_match=false action_ms={action_ms}");
                    return Err("own_focus_lost_during_action");
                }
                after = value
                    .CurrentValue()
                    .map_err(|_| "fixture_value_unavailable")?
                    .to_string();
                if after == expected || observe_started.elapsed() >= Duration::from_millis(500) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let focus_match = own_focus(&args, &process, &uia, &fixture);
            let expected_match = after == expected;
            let passed = api_ok && expected_match && focus_match;
            println!(
                "result={} case={} api_ok={api_ok} focus_match={focus_match} expected_match={expected_match} before_len={} after_len={} action_ms={action_ms} elapsed_ms={}",
                if passed { "passed" } else { "failed" }, args.case,
                initial.encode_utf16().count(), after.encode_utf16().count(), started.elapsed().as_millis()
            );
            Ok(passed)
        }
    }
}
