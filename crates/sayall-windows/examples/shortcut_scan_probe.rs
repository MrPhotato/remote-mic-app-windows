//! Opt-in shortcut verification in a window owned by this probe only.
//! `cargo run -p sayall-windows --example shortcut_scan_probe -- --run`
//! Optional `--idle-seconds=60` waits before the first test. No input is sent
//! unless this exact window owns foreground and keyboard focus. All four edges
//! of every shortcut are submitted together; a partial batch is released.

#[cfg(not(windows))]
fn main() {
    eprintln!("This probe requires Windows.");
}

#[cfg(windows)]
fn main() {
    if !std::env::args().any(|arg| arg == "--run") {
        println!("Pass --run to create a temporary isolated shortcut-test window.");
        return;
    }
    let idle = std::env::args()
        .find_map(|arg| arg.strip_prefix("--idle-seconds=").map(str::to_owned))
        .map(|arg| arg.parse::<u32>().expect("idle seconds must be an integer"))
        .unwrap_or(1)
        .clamp(1, 120);
    match probe::run(idle) {
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
    use sayall_windows::send_input::{KeyChord, KeyCode};
    use sayall_windows::send_input_windows::SendInputRuntime;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;
    use windows::core::w;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    static PHASE: AtomicUsize = AtomicUsize::new(0);
    static INJECTION_OK: AtomicBool = AtomicBool::new(true);
    static OBSERVATIONS: Mutex<Vec<(usize, u32, u16, bool, bool, bool)>> = Mutex::new(Vec::new());
    const CASES: [&str; 4] = [
        "runtime_ctrl_page_up",
        "runtime_ctrl_page_down",
        "physical_ctrl_page_up",
        "physical_ctrl_page_down",
    ];

    fn physical_input(scan: u16, extended: bool, up: bool) -> INPUT {
        let mut flags = KEYEVENTF_SCANCODE;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        }
    }

    unsafe fn send_physical(page_scan: u16) -> bool {
        let inputs = [
            physical_input(0x1d, false, false),
            physical_input(page_scan, true, false),
            physical_input(page_scan, true, true),
            physical_input(0x1d, false, true),
        ];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            // Release only keys whose down edges may have been submitted.
            let releases = match sent {
                1 => vec![physical_input(0x1d, false, true)],
                2 => vec![
                    physical_input(page_scan, true, true),
                    physical_input(0x1d, false, true),
                ],
                3 => vec![physical_input(0x1d, false, true)],
                _ => Vec::new(),
            };
            if !releases.is_empty() {
                let _ = SendInput(&releases, std::mem::size_of::<INPUT>() as i32);
            }
        }
        sent == inputs.len() as u32
    }

    unsafe fn own_focus(hwnd: HWND) -> bool {
        GetForegroundWindow() == hwnd && GetFocus() == hwnd
    }

    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_TIMER => {
                let phase = PHASE.load(Ordering::SeqCst);
                if phase >= CASES.len() {
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                if !own_focus(hwnd) {
                    println!(
                        "deferred case={} reason=own_window_not_focused",
                        CASES[phase]
                    );
                    INJECTION_OK.store(false, Ordering::SeqCst);
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                // Never overwrite a physically held modifier during a probe.
                if [0x11, 0x10, 0x12, 0x5b, 0x5c]
                    .into_iter()
                    .any(|key| GetAsyncKeyState(key) < 0)
                {
                    println!(
                        "deferred case={} reason=modifier_already_held",
                        CASES[phase]
                    );
                    INJECTION_OK.store(false, Ordering::SeqCst);
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                PHASE.store(phase + 1, Ordering::SeqCst);
                let ok = if phase < 2 {
                    SendInputRuntime::new()
                        .tap(KeyChord {
                            keys: vec![
                                KeyCode::Control,
                                if phase == 0 {
                                    KeyCode::PageUp
                                } else {
                                    KeyCode::PageDown
                                },
                            ],
                        })
                        .is_ok()
                } else {
                    send_physical(if phase == 2 { 0x49 } else { 0x51 })
                };
                println!("submit case={} complete={ok}", CASES[phase]);
                if !ok {
                    INJECTION_OK.store(false, Ordering::SeqCst);
                }
                let _ = SetTimer(Some(hwnd), 1, 750, None);
                LRESULT(0)
            }
            WM_KEYDOWN | WM_KEYUP => {
                let phase = PHASE.load(Ordering::SeqCst);
                let vk = wparam.0 as u32;
                if phase > 0 && matches!(vk, 0x11 | 0x21 | 0x22) {
                    let scan = ((lparam.0 >> 16) & 0xff) as u16;
                    let extended = lparam.0 & (1 << 24) != 0;
                    let control = GetKeyState(0x11) < 0;
                    let key_up = message == WM_KEYUP;
                    OBSERVATIONS.lock().unwrap().push((
                        phase - 1,
                        vk,
                        scan,
                        extended,
                        control,
                        key_up,
                    ));
                    println!(
                        "event case={} edge={} vk={vk:02x} scan={scan:02x} extended={extended} ctrl={control}",
                        CASES[phase - 1],
                        if key_up { "up" } else { "down" }
                    );
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                println!(
                    "closed completed_submissions={}",
                    PHASE.load(Ordering::SeqCst)
                );
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    pub(super) fn run(idle: u32) -> windows::core::Result<bool> {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("RemoteCodingShortcutScanProbe");
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
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!("Remote Coding: isolated shortcut scan verification"),
                WS_OVERLAPPEDWINDOW,
                180,
                180,
                720,
                220,
                None,
                None,
                Some(instance),
                None,
            )?;
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            println!("ready own_focus={} idle_seconds={idle}", own_focus(hwnd));
            if SetTimer(Some(hwnd), 1, idle * 1000, None) == 0 {
                let _ = DestroyWindow(hwnd);
                return Err(windows::core::Error::from_thread());
            }
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let observations = OBSERVATIONS.lock().unwrap();
            let mut all_passed = INJECTION_OK.load(Ordering::SeqCst);
            for (phase, name) in CASES.into_iter().enumerate() {
                let expected_scan = if phase % 2 == 0 { 0x49 } else { 0x51 };
                let events: Vec<_> = observations
                    .iter()
                    .filter(|event| event.0 == phase)
                    .collect();
                if phase >= PHASE.load(Ordering::SeqCst) {
                    println!("deferred case={name} reason=not_submitted");
                    all_passed = false;
                    continue;
                }
                let passed = events.len() == 4
                    && events.iter().any(|event| {
                        matches!(event.1, 0x21 | 0x22)
                            && event.2 == expected_scan
                            && event.3
                            && event.4
                            && !event.5
                    })
                    && events
                        .last()
                        .is_some_and(|event| event.1 == 0x11 && event.5 && !event.4);
                all_passed &= passed;
                println!(
                    "{} case={name} edges={}",
                    if passed { "passed" } else { "failed" },
                    events.len()
                );
            }
            Ok(all_passed)
        }
    }
}
