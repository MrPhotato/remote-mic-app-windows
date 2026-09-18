//! Opt-in UIA verification against native controls created by this process only.
//! Run with `cargo run -p sayall-windows --example text_edit_probe -- --run`.
//! No third-party window is queried for text or receives injected input. Losing
//! focus cancels the action. The temporary test window closes after the checks.

#[cfg(not(windows))]
fn main() {
    eprintln!("This probe requires Windows.");
}

#[cfg(windows)]
fn main() {
    if !std::env::args().any(|arg| arg == "--run") {
        println!("Explicit opt-in required: --run (creates a temporary native test window).");
        return;
    }
    match probe::run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(2),
        Err(error) => {
            eprintln!("probe setup failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
mod probe {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const FOCUS_OWN_EDIT: u32 = WM_APP + 71;
    // Public Edit control messages, avoiding a Controls module dependency.
    const EM_SETSEL: u32 = 0x00B1;
    const EM_GETSEL: u32 = 0x00B0;
    const EM_SETREADONLY: u32 = 0x00CF;

    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            FOCUS_OWN_EDIT => {
                let child = HWND(wparam.0 as *mut _);
                if GetParent(child).ok() == Some(hwnd) {
                    let _ = SetFocus(Some(child));
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe fn own_focus(parent: HWND, edit: HWND) -> bool {
        if GetForegroundWindow() != parent
            || !IsWindow(Some(parent)).as_bool()
            || !IsWindow(Some(edit)).as_bool()
        {
            return false;
        }
        let thread = GetWindowThreadProcessId(parent, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread, &mut info).is_ok() && info.hwndFocus == edit
    }

    unsafe fn read_own_text(edit: HWND) -> String {
        let mut buffer = [0u16; 4096];
        let length = GetWindowTextW(edit, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..length])
    }

    unsafe fn own_selection(edit: HWND) -> (u32, u32) {
        let (mut start, mut end) = (0u32, 0u32);
        let _ = SendMessageW(
            edit,
            EM_GETSEL,
            Some(WPARAM(&mut start as *mut u32 as usize)),
            Some(LPARAM(&mut end as *mut u32 as isize)),
        );
        (start, end)
    }

    unsafe fn prepare_own_edit(parent: HWND, edit: HWND, value: &str) -> bool {
        let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let _ = SendMessageW(edit, EM_SETREADONLY, Some(WPARAM(0)), None);
        if SetWindowTextW(edit, PCWSTR(wide.as_ptr())).is_err() {
            return false;
        }
        let count = value.encode_utf16().count();
        let _ = SendMessageW(
            edit,
            EM_SETSEL,
            Some(WPARAM(count)),
            Some(LPARAM(count as isize)),
        );
        let _ = SendMessageW(parent, FOCUS_OWN_EDIT, Some(WPARAM(edit.0 as usize)), None);
        own_focus(parent, edit) && read_own_text(edit) == value
    }

    unsafe fn cancellation_case(parent: HWND, edit: HWND, move_selection: bool) -> bool {
        let initial = "测试，必须保留这些文字";
        if !prepare_own_edit(parent, edit, initial) {
            println!("deferred cancellation: own focus not verified");
            return false;
        }
        let cancelled = std::cell::Cell::new(false);
        let action = sayall_windows::text_edit::delete_to_previous_punctuation_with_cancel(&|| {
            if !own_focus(parent, edit) {
                return true;
            }
            let (start, end) = own_selection(edit);
            if start != end && !cancelled.get() {
                cancelled.set(true);
                if move_selection {
                    // Emulate a newer user caret only inside our own control.
                    let _ = SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(0)));
                }
            }
            cancelled.get()
        });
        let count = initial.encode_utf16().count() as u32;
        let expected = if move_selection {
            (0, 0)
        } else {
            (count, count)
        };
        let passed = cancelled.get()
            && action.is_err()
            && read_own_text(edit) == initial
            && own_selection(edit) == expected;
        println!(
            "{} {} result=refused",
            if passed { "passed" } else { "failed" },
            if move_selection {
                "cancel_preserves_newer_caret"
            } else {
                "cancel_restores_original_caret"
            }
        );
        passed
    }

    struct Case {
        name: &'static str,
        text: &'static str,
        expected: &'static str,
        control: usize,
        reject: bool,
        read_only: bool,
    }

    pub(super) fn run() -> windows::core::Result<bool> {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("RemoteCodingTextEditProbe");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                return Err(windows::core::Error::from_thread());
            }
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!("Remote Coding: isolated text-edit verification"),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                660,
                260,
                None,
                None,
                Some(instance),
                None,
            )?;
            let make_edit = |style: i32, y: i32| {
                CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    w!("EDIT"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(style as u32),
                    20,
                    y,
                    595,
                    45,
                    Some(parent),
                    None,
                    Some(instance),
                    None,
                )
            };
            let normal = make_edit(ES_AUTOHSCROLL, 20)?;
            let multiline = make_edit(ES_MULTILINE, 77)?;
            let password = make_edit(ES_PASSWORD | ES_AUTOHSCROLL, 134)?;
            let _ = ShowWindow(parent, SW_SHOW);
            let _ = SetForegroundWindow(parent);
            let _ = SetFocus(Some(normal));
            let handles = [
                parent.0 as usize,
                normal.0 as usize,
                multiline.0 as usize,
                password.0 as usize,
            ];
            let (tx, rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let [parent, normal, multiline, password] = handles.map(|h| HWND(h as *mut _));
                let controls = [normal, multiline, password];
                let result = (|| {
                    let cases = [
                        Case {
                            name: "keep_chinese_punctuation",
                            text: "你好，今天天气很好",
                            expected: "你好，",
                            control: 0,
                            reject: false,
                            read_only: false,
                        },
                        Case {
                            name: "keep_ascii_punctuation",
                            text: "first; final words",
                            expected: "first;",
                            control: 0,
                            reject: false,
                            read_only: false,
                        },
                        Case {
                            name: "current_paragraph_only",
                            text: "上一行\r\n本段文字",
                            expected: "上一行\r\n",
                            control: 1,
                            reject: false,
                            read_only: false,
                        },
                        Case {
                            name: "no_boundary_to_start",
                            text: "本段文字",
                            expected: "",
                            control: 0,
                            reject: false,
                            read_only: false,
                        },
                        Case {
                            name: "at_punctuation_noop",
                            text: "你好，",
                            expected: "你好，",
                            control: 0,
                            reject: false,
                            read_only: false,
                        },
                        Case {
                            name: "empty_noop",
                            text: "",
                            expected: "",
                            control: 0,
                            reject: false,
                            read_only: false,
                        },
                        Case {
                            name: "readonly_refused",
                            text: "只读，不可删除",
                            expected: "只读，不可删除",
                            control: 0,
                            reject: true,
                            read_only: true,
                        },
                        Case {
                            name: "password_refused",
                            text: "fixed,test-secret",
                            expected: "fixed,test-secret",
                            control: 2,
                            reject: true,
                            read_only: false,
                        },
                    ];
                    let mut all_passed = true;
                    for case in cases {
                        let edit = controls[case.control];
                        if !IsWindow(Some(parent)).as_bool() {
                            println!("deferred: probe window was closed");
                            return false;
                        }
                        let wide: Vec<u16> = case.text.encode_utf16().chain(Some(0)).collect();
                        let _ = SendMessageW(edit, EM_SETREADONLY, Some(WPARAM(0)), None);
                        if SetWindowTextW(edit, PCWSTR(wide.as_ptr())).is_err() {
                            println!("failed {} setup", case.name);
                            return false;
                        }
                        let _ = SendMessageW(
                            edit,
                            EM_SETSEL,
                            Some(WPARAM(case.text.encode_utf16().count())),
                            Some(LPARAM(case.text.encode_utf16().count() as isize)),
                        );
                        let _ = SendMessageW(
                            edit,
                            EM_SETREADONLY,
                            Some(WPARAM(case.read_only as usize)),
                            None,
                        );
                        let _ = SendMessageW(
                            parent,
                            FOCUS_OWN_EDIT,
                            Some(WPARAM(edit.0 as usize)),
                            None,
                        );
                        if !own_focus(parent, edit) || read_own_text(edit) != case.text {
                            println!("deferred {}: own foreground/focus/text not verified; no UIA operation", case.name);
                            return false;
                        }
                        let started = Instant::now();
                        let action =
                            sayall_windows::text_edit::delete_to_previous_punctuation_with_cancel(
                                &|| !own_focus(parent, edit),
                            );
                        // SendInput submission is asynchronous. Poll only our own
                        // control until its expected text arrives or 500ms elapses.
                        let until = Instant::now() + Duration::from_millis(500);
                        while read_own_text(edit) != case.expected && Instant::now() < until {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        let passed =
                            action.is_err() == case.reject && read_own_text(edit) == case.expected;
                        println!(
                            "{} {} result={} elapsed_ms={}",
                            if passed { "passed" } else { "failed" },
                            case.name,
                            if action.is_ok() {
                                "accepted"
                            } else {
                                "refused"
                            },
                            started.elapsed().as_millis()
                        );
                        if !passed {
                            all_passed = false;
                        }
                        if !own_focus(parent, edit) {
                            println!("deferred: focus lost; remaining cases cancelled");
                            return false;
                        }
                    }
                    all_passed &= cancellation_case(parent, normal, false);
                    all_passed &= cancellation_case(parent, normal, true);
                    if std::env::args().any(|arg| arg == "--idle") {
                        let initial = "闲置，首次删除测试";
                        if !prepare_own_edit(parent, normal, initial) {
                            return false;
                        }
                        println!("idle: waiting 60 seconds in the isolated test window");
                        let until = Instant::now() + Duration::from_secs(60);
                        while Instant::now() < until {
                            if !own_focus(parent, normal) || read_own_text(normal) != initial {
                                println!("deferred idle: own focus/text changed; no deletion");
                                return false;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        let action =
                            sayall_windows::text_edit::delete_to_previous_punctuation_with_cancel(
                                &|| !own_focus(parent, normal),
                            );
                        let deadline = Instant::now() + Duration::from_millis(500);
                        while read_own_text(normal) != "闲置，" && Instant::now() < deadline {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        let passed = action.is_ok() && read_own_text(normal) == "闲置，";
                        println!(
                            "{} first_after_60s_idle",
                            if passed { "passed" } else { "failed" }
                        );
                        if !passed {
                            let observed = read_own_text(normal);
                            let selection = own_selection(normal);
                            println!("idle_evidence result_ok={} original_unchanged={} observed_utf16_units={} expected_utf16_units={} selection_start={} selection_end={} own_focus={}",
                                action.is_ok(), observed == initial, observed.encode_utf16().count(),
                                "闲置，".encode_utf16().count(), selection.0, selection.1, own_focus(parent, normal));
                            // Read-only observation, not an input retry: distinguish
                            // delayed native delivery from an ineffective action.
                            std::thread::sleep(Duration::from_secs(2));
                            println!("idle_evidence after_2s_expected={} original_unchanged={} own_focus={}",
                                read_own_text(normal) == "闲置，", read_own_text(normal) == initial,
                                own_focus(parent, normal));
                        }
                        all_passed &= passed;
                    }
                    all_passed
                })();
                let _ = tx.send(result);
                let _ = PostMessageW(Some(parent), WM_CLOSE, WPARAM(0), LPARAM(0));
            });
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let _ = worker.join();
            let result = rx.try_recv().unwrap_or(false);
            let _ = UnregisterClassW(class, Some(instance));
            Ok(result)
        }
    }
}
