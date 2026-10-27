//! Supply the large window icon that the Tauri/Tao default icon path omits.
//!
//! This only changes our own window's ICON_BIG. It does not touch shortcuts,
//! Shell caches, window grouping, or the small icon managed by Tauri.

use std::fmt;
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, LoadImageW, SendMessageW, HICON, ICON_BIG, ICON_SMALL, IMAGE_ICON,
    LR_DEFAULTSIZE, LR_SHARED, WM_GETICON, WM_SETICON,
};

// tauri-build embeds the configured Windows ICO under this resource identifier.
const APPLICATION_ICON_RESOURCE: usize = 32512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarIconUpdate {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarIconError {
    pub stage: &'static str,
    pub reason: &'static str,
    pub native_code: Option<i32>,
}

impl TaskbarIconError {
    fn new(stage: &'static str, reason: &'static str) -> Self {
        Self {
            stage,
            reason,
            native_code: None,
        }
    }

    fn native(stage: &'static str, reason: &'static str, error: windows::core::Error) -> Self {
        Self {
            stage,
            reason,
            native_code: Some(error.code().0),
        }
    }
}

impl fmt::Display for TaskbarIconError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.reason)?;
        if let Some(code) = self.native_code {
            write!(formatter, " (0x{:08x})", code as u32)?;
        }
        Ok(())
    }
}

impl std::error::Error for TaskbarIconError {}

/// Set the main window's large icon from the current executable's resource.
///
/// Must run on the window-owning thread (the Tauri setup callback does so).
/// Shared resource handles remain valid for the executable's lifetime and must
/// not be destroyed. A failure is diagnostic-only; the caller may keep running.
pub fn set_main_window_taskbar_icon(hwnd: HWND) -> Result<TaskbarIconUpdate, TaskbarIconError> {
    let started = Instant::now();
    crate::gatt_note(
        "window_icon feature=taskbar_icon action=initialize phase=started result=passed".to_owned(),
    );
    let result = apply_with_loader(hwnd, load_application_icon);
    match &result {
        Ok(update) => crate::gatt_note(format!(
            "window_icon feature=taskbar_icon action=initialize phase=completed terminal_result=passed reason={} source=executable_resource resource_id={} size=system_default lifetime=module_shared icon_big_verified=true icon_small_preserved=true elapsed_ms={}",
            match update {
                TaskbarIconUpdate::Applied => "applied",
                TaskbarIconUpdate::AlreadyApplied => "already_applied",
            },
            APPLICATION_ICON_RESOURCE,
            started.elapsed().as_millis(),
        )),
        Err(error) => crate::gatt_note(format!(
            "window_icon feature=taskbar_icon action=initialize phase=completed terminal_result=failed stage={} reason={} error_domain=windows error_code={} retryable=false elapsed_ms={}",
            error.stage,
            error.reason,
            error.native_code.map(|code| format!("0x{:08x}", code as u32)).unwrap_or_else(|| "none".to_owned()),
            started.elapsed().as_millis(),
        )),
    }
    result
}

fn load_application_icon() -> Result<HICON, TaskbarIconError> {
    let module = unsafe { GetModuleHandleW(None) }.map_err(|error| {
        TaskbarIconError::native("load_module", "executable_module_unavailable", error)
    })?;
    // Never pass a null module: 32512 would select the generic system icon.
    // DEFAULTSIZE avoids taking the ICO's first (smallest) frame. LR_SHARED is
    // deliberately used with one fixed system-default size, not per-DPI sizes.
    let image = unsafe {
        LoadImageW(
            Some(HINSTANCE(module.0)),
            PCWSTR(APPLICATION_ICON_RESOURCE as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
    }
    .map_err(|error| {
        TaskbarIconError::native("load_resource", "application_icon_unavailable", error)
    })?;
    Ok(HICON(image.0))
}

fn apply_with_loader(
    hwnd: HWND,
    load_icon: impl FnOnce() -> Result<HICON, TaskbarIconError>,
) -> Result<TaskbarIconUpdate, TaskbarIconError> {
    let mut owner_process = 0;
    let owner_thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_process)) };
    if hwnd.is_invalid() || owner_thread == 0 {
        return Err(TaskbarIconError::new("validate_window", "invalid_window"));
    }
    if owner_process != unsafe { GetCurrentProcessId() } {
        return Err(TaskbarIconError::new("validate_window", "foreign_process"));
    }
    if owner_thread != unsafe { GetCurrentThreadId() } {
        return Err(TaskbarIconError::new("validate_window", "wrong_thread"));
    }

    let icon = load_icon()?;
    if icon.is_invalid() {
        return Err(TaskbarIconError::new("load_resource", "invalid_icon"));
    }
    let small_before = read_icon(hwnd, ICON_SMALL);
    if read_icon(hwnd, ICON_BIG) == icon {
        return Ok(TaskbarIconUpdate::AlreadyApplied);
    }
    // A zero return is the previous icon, not an error. Do not destroy it:
    // its ownership belongs to whichever component originally installed it.
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
    }
    if read_icon(hwnd, ICON_BIG) != icon {
        return Err(TaskbarIconError::new("verify_icon", "large_icon_mismatch"));
    }
    if read_icon(hwnd, ICON_SMALL) != small_before {
        return Err(TaskbarIconError::new("verify_icon", "small_icon_changed"));
    }
    Ok(TaskbarIconUpdate::Applied)
}

fn read_icon(hwnd: HWND, kind: u32) -> HICON {
    let result = unsafe { SendMessageW(hwnd, WM_GETICON, Some(WPARAM(kind as usize)), None) };
    HICON(result.0 as *mut core::ffi::c_void)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows::core::w;
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetDesktopWindow, LoadIconW,
        RegisterClassW, UnregisterClassW, IDI_INFORMATION, IDI_WARNING, WNDCLASSW, WNDPROC,
    };

    static CLASS_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    struct TestWindow {
        hwnd: HWND,
        class_name: Vec<u16>,
        instance: HINSTANCE,
    }

    impl TestWindow {
        fn new(procedure: WNDPROC) -> Self {
            let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }.unwrap().0);
            let class_name: Vec<u16> = format!(
                "SayAllIconTest-{}-{}",
                std::process::id(),
                CLASS_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let class = WNDCLASSW {
                lpfnWndProc: procedure,
                hInstance: instance,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            assert_ne!(unsafe { RegisterClassW(&class) }, 0);
            // No WS_VISIBLE: the fixture never shows or joins the taskbar.
            let hwnd = unsafe {
                CreateWindowExW(
                    Default::default(),
                    PCWSTR(class_name.as_ptr()),
                    w!("SayAll icon test"),
                    Default::default(),
                    0,
                    0,
                    32,
                    32,
                    None,
                    None,
                    Some(instance),
                    None,
                )
            }
            .unwrap();
            Self {
                hwnd,
                class_name,
                instance,
            }
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
                let _ = UnregisterClassW(PCWSTR(self.class_name.as_ptr()), Some(self.instance));
            }
        }
    }

    // Test executables do not contain Tauri's resource 32512. Use shared system
    // icons only as fixtures for the exact same application/validation path.
    fn fixture_icon() -> Result<HICON, TaskbarIconError> {
        Ok(unsafe { LoadIconW(None, IDI_INFORMATION) }.unwrap())
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    #[test]
    fn window_icon_sets_large_preserves_small_and_reuses_shared_handle() {
        let window = TestWindow::new(Some(window_proc));
        let small = unsafe { LoadIconW(None, IDI_WARNING) }.unwrap();
        unsafe {
            SendMessageW(
                window.hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(small.0 as isize)),
            );
        }
        assert!(read_icon(window.hwnd, ICON_BIG).is_invalid());
        assert_eq!(
            apply_with_loader(window.hwnd, fixture_icon),
            Ok(TaskbarIconUpdate::Applied)
        );
        let large = read_icon(window.hwnd, ICON_BIG);
        assert_eq!(large, fixture_icon().unwrap());
        assert_eq!(read_icon(window.hwnd, ICON_SMALL), small);
        assert_eq!(
            apply_with_loader(window.hwnd, fixture_icon),
            Ok(TaskbarIconUpdate::AlreadyApplied)
        );
        assert_eq!(read_icon(window.hwnd, ICON_BIG), large);
        assert_eq!(read_icon(window.hwnd, ICON_SMALL), small);
    }

    #[test]
    fn window_icon_rejects_invalid_window_before_loading() {
        let error = apply_with_loader(HWND::default(), || panic!("must not load")).unwrap_err();
        assert_eq!(error.reason, "invalid_window");
    }

    #[test]
    fn window_icon_rejects_foreign_process_before_loading() {
        let desktop = unsafe { GetDesktopWindow() };
        let error = apply_with_loader(desktop, || panic!("must not load")).unwrap_err();
        assert_eq!(error.reason, "foreign_process");
    }

    #[test]
    fn window_icon_rejects_wrong_thread_without_changing_window() {
        let window = TestWindow::new(Some(window_proc));
        let raw = window.hwnd.0 as usize;
        let error = std::thread::spawn(move || {
            apply_with_loader(HWND(raw as *mut core::ffi::c_void), || {
                panic!("must not load")
            })
            .unwrap_err()
        })
        .join()
        .unwrap();
        assert_eq!(error.reason, "wrong_thread");
        assert!(read_icon(window.hwnd, ICON_BIG).is_invalid());
    }

    #[test]
    fn window_icon_load_failure_keeps_existing_icons() {
        let window = TestWindow::new(Some(window_proc));
        let expected = TaskbarIconError::new("load_resource", "fixture_missing");
        assert_eq!(
            apply_with_loader(window.hwnd, || Err(expected)),
            Err(expected)
        );
        assert!(read_icon(window.hwnd, ICON_BIG).is_invalid());
        assert!(read_icon(window.hwnd, ICON_SMALL).is_invalid());
    }

    unsafe extern "system" fn ignore_icon_changes(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_SETICON {
            LRESULT(0)
        } else {
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }

    #[test]
    fn window_icon_reports_failed_readback() {
        let window = TestWindow::new(Some(ignore_icon_changes));
        let error = apply_with_loader(window.hwnd, fixture_icon).unwrap_err();
        assert_eq!(error.stage, "verify_icon");
        assert_eq!(error.reason, "large_icon_mismatch");
    }
}
