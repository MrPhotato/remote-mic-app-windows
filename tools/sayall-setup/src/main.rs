#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod native;
mod validation;

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Manager, State};
use validation::{
    directory_path, installer_arguments, reject_reparse_points, same_directory, validate_target,
};

const VERSION: &str = env!("SAYALL_SETUP_VERSION");
const PAYLOAD_HASH: &str = env!("SAYALL_SETUP_PAYLOAD_SHA256");
const APP_HASH: &str = env!("SAYALL_SETUP_APP_SHA256");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
struct SetupState {
    busy: AtomicBool,
    finished: AtomicBool,
    sequence: AtomicU64,
    installed: Mutex<Option<Installed>>,
}

struct Operation {
    state: Arc<SetupState>,
    id: u64,
    started: Instant,
    terminal: bool,
}

impl Operation {
    fn begin(state: &Arc<SetupState>) -> Result<Self, String> {
        if state.finished.load(Ordering::Acquire)
            || state
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            note("request_rejected", "busy");
            return Err("安装正在进行，请稍候。".into());
        }
        let operation = Self {
            state: state.clone(),
            id: state.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            started: Instant::now(),
            terminal: false,
        };
        operation.record("request", "accepted", "explicit_click", "");
        Ok(operation)
    }

    fn record(&self, event: &str, result: &str, reason: &str, extra: &str) {
        write_note(&format!(
            "operation_id={} event={event} result={result} reason={reason} took_ms={} {extra}",
            self.id,
            self.started.elapsed().as_millis()
        ));
    }

    fn failure(&self, event: &str, reason: &str, error: String) -> String {
        self.record(
            event,
            "failed",
            reason,
            &format!("error_domain=setup error_code={reason} retryable=true"),
        );
        error
    }

    fn finish(&mut self, result: &str) {
        self.record(
            "completed",
            result,
            "operation_finished",
            &format!("terminal_result={result}"),
        );
        self.terminal = true;
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if !self.terminal {
            self.finish("failed");
        }
        self.state.busy.store(false, Ordering::Release);
    }
}

#[derive(Serialize)]
struct SetupInfo {
    version: &'static str,
    directory: String,
    existing: bool,
}

#[derive(Clone, Serialize)]
struct Installed {
    directory: String,
    version: String,
}

/// Log only fixed event/reason labels: never arguments, directory names or
/// device/user information. Setup never opens the application's settings.
fn note(event: &str, result: &str) {
    write_note(&format!("event={event} result={result}"));
}

fn write_note(fields: &str) {
    let root = std::env::temp_dir().join("SayAllSetup");
    if std::fs::create_dir_all(&root).is_err() {
        return;
    }
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("setup.log"))
    {
        let time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let _ = writeln!(
            file,
            "{time} pid={} ver={VERSION} build=unknown feature=setup {fields}",
            std::process::id()
        );
    }
}

fn checked_file(path: &Path, expected: &str) -> Result<File, String> {
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("此安装包未完成校验准备，请重新下载安装包。".into());
    }
    reject_reparse_points(path)?;
    // Retain this read handle through execution. Denying write/delete sharing
    // prevents swapping a verified payload before the child opens it.
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .map_err(|_| "无法读取安装文件，请重新下载安装包。".to_owned())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "安装文件校验失败，请重新下载。".to_owned())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != expected {
        return Err("安装文件校验失败，请重新下载安装包。".into());
    }
    Ok(file)
}

fn payload_path() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("sayall-payload.exe"))
        })
        .ok_or_else(|| "无法确认安装文件的位置。".into())
}

fn verify_installed(directory: &Path) -> Result<Installed, String> {
    let registered = native::registration()?.ok_or("未能确认安装结果，请重试安装。")?;
    if registered.version != VERSION || !same_directory(&registered.directory, directory) {
        return Err("安装结果与预期不一致，请重试安装。".into());
    }
    let _file = checked_file(&directory.join("remote-coding.exe"), APP_HASH)?;
    Ok(Installed {
        directory: directory.to_string_lossy().into_owned(),
        version: VERSION.into(),
    })
}

#[tauri::command]
fn setup_info() -> Result<SetupInfo, String> {
    let current = native::registration()?;
    let directory = match &current {
        Some(current) => current.directory.clone(),
        None => native::default_directory()?,
    };
    note(
        "info",
        if current.is_some() {
            "existing"
        } else {
            "fresh"
        },
    );
    Ok(SetupInfo {
        version: VERSION,
        directory: directory.to_string_lossy().into_owned(),
        existing: current.is_some(),
    })
}

#[tauri::command]
async fn setup_browse_directory(
    directory: String,
    window: tauri::Window,
    state: State<'_, Arc<SetupState>>,
) -> Result<Option<String>, String> {
    let operation = Operation::begin(&state)?;
    if native::registration()?.is_some() {
        return Err("已有安装会沿用原来的位置。".into());
    }
    let (initial, used_default) = browse_initial_directory(&directory, native::default_directory)?;
    if used_default {
        operation.record(
            "browse_initial",
            "fallback",
            "invalid_input_using_default",
            "",
        );
    }
    let owner = window
        .hwnd()
        .map_err(|_| "无法打开文件夹选择器。".to_owned())?
        .0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        let mut operation = operation;
        let result = native::browse_directory(initial, owner);
        let outcome = match &result {
            Ok(Some(_)) => "passed",
            Ok(None) => "cancelled",
            Err(_) => "failed",
        };
        operation.record("browse", outcome, "folder_dialog", "");
        operation.finish(outcome);
        result
    })
    .await
    .map_err(|_| "文件夹选择器意外结束，请重试。".to_owned())?
}

fn browse_initial_directory(
    input: &str,
    default: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<(PathBuf, bool), String> {
    match directory_path(input) {
        Ok(path) => Ok((path, false)),
        Err(_) => default().map(|path| (path, true)),
    }
}

fn install_exit_message(code: Option<i32>, unregistered_partial_files: bool) -> &'static str {
    match code {
        Some(1638) => "电脑上已有更新版本，不能安装较早版本。",
        Some(1639 | 1640) => "无线麦尚未完成退出，本次安装已停止。请稍后重试。",
        _ if unregistered_partial_files => {
            "安装未完成，部分文件已写入。请选择另一个空文件夹后重试。"
        }
        _ => "安装未完成。请确认文件夹可写，然后重试。",
    }
}

fn install(
    directory: String,
    desktop: bool,
    state: Arc<SetupState>,
    mut operation: Operation,
) -> Result<Installed, String> {
    if state
        .installed
        .lock()
        .map_err(|_| "无法读取安装状态。")?
        .is_some()
    {
        return Err("安装已完成。".into());
    }
    let target = directory_path(&directory)
        .map_err(|error| operation.failure("target_validation", "invalid_path", error))?;
    let current = native::registration()
        .map_err(|error| operation.failure("target_validation", "registration_invalid", error))?;
    validate_target(
        &target,
        current
            .as_ref()
            .map(|registration| registration.directory.as_path()),
    )
    .map_err(|error| operation.failure("target_validation", "unsafe_target", error))?;
    native::check_local_writable_directory(&target)
        .map_err(|error| operation.failure("target_validation", "target_not_writable", error))?;
    let payload = payload_path()
        .map_err(|error| operation.failure("payload_validation", "payload_location", error))?;
    let _verified_payload = checked_file(&payload, PAYLOAD_HASH).map_err(|error| {
        operation.failure("payload_validation", "payload_hash_or_read_failed", error)
    })?;
    // A valid main-binary binding is also required before changing anything.
    if APP_HASH.len() != 64 || !APP_HASH.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(operation.failure(
            "payload_validation",
            "app_hash_unbound",
            "此安装包未完成校验准备，请重新下载安装包。".into(),
        ));
    }
    let arguments = installer_arguments(&target, current.is_some(), desktop);
    operation.record("payload_verified", "passed", "sha256_matches", "");
    let mut child = Command::new(&payload)
        .raw_arg(arguments)
        .current_dir(payload.parent().ok_or("无法确认安装文件的位置。")?)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| {
            operation.failure(
                "install_start",
                "process_start_failed",
                "无法开始安装，请重试。".into(),
            )
        })?;
    operation.record("install_started", "passed", "child_started", "");
    // Never kill a child on a timeout: it may be waiting for the installed app
    // to finish microphone/BLE cleanup. Close remains disabled until it exits.
    let mut wait_failure_logged = false;
    let status = loop {
        match child.wait() {
            Ok(status) => break status,
            Err(_) => {
                if !wait_failure_logged {
                    operation.record(
                        "install_wait",
                        "retrying",
                        "process_wait_failed",
                        "error_domain=process error_code=wait_failed retryable=true",
                    );
                    wait_failure_logged = true;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    };
    if !status.success() {
        operation.record(
            "install_exit",
            "failed",
            "nonzero_exit",
            &format!(
                "error_domain=nsis error_code={} retryable=true",
                status.code().unwrap_or(-1)
            ),
        );
        let unregistered_partial_files = current.is_none()
            && !matches!(status.code(), Some(1638 | 1639 | 1640))
            && matches!(native::registration(), Ok(None))
            && std::fs::read_dir(&target).is_ok_and(|mut entries| entries.next().is_some());
        if unregistered_partial_files {
            operation.record(
                "install_recovery",
                "required",
                "unregistered_partial_files",
                "recovery=choose_empty_directory existing_files_preserved=true",
            );
        }
        return Err(install_exit_message(status.code(), unregistered_partial_files).into());
    }
    operation.record("install_exit", "passed", "exit_zero", "exit_code=0");
    let result = verify_installed(&target).map_err(|error| {
        operation.failure(
            "install_verification",
            "installed_registration_or_hash_failed",
            error,
        )
    })?;
    *state.installed.lock().map_err(|_| "无法保存安装状态。")? = Some(result.clone());
    operation.record(
        "install_verified",
        "passed",
        "registration_and_sha256_match",
        "",
    );
    operation.finish("passed");
    Ok(result)
}

#[tauri::command]
async fn setup_install(
    directory: String,
    desktop_shortcut: bool,
    state: State<'_, Arc<SetupState>>,
) -> Result<Installed, String> {
    let state = state.inner().clone();
    let operation = Operation::begin(&state)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        install(directory, desktop_shortcut, state, operation)
    })
    .await
    .map_err(|_| "安装进程意外结束，请重新打开安装包检查。".to_owned())?;
    if result.is_err() {
        note("install", "failed");
    }
    result
}

#[tauri::command]
fn setup_finish(
    launch: bool,
    app: tauri::AppHandle,
    state: State<'_, Arc<SetupState>>,
) -> Result<(), String> {
    let mut operation = Operation::begin(&state)?;
    let installed = state
        .installed
        .lock()
        .map_err(|_| "无法读取安装状态。")?
        .clone()
        .ok_or("请先完成安装。")?;
    let directory = directory_path(&installed.directory)?;
    verify_installed(&directory)?;
    if launch {
        let application = directory.join("remote-coding.exe");
        let _verified_app = checked_file(&application, APP_HASH)?;
        Command::new(&application)
            .current_dir(&directory)
            .spawn()
            .map_err(|_| "安装已完成，但未能打开无线麦。请通过桌面或开始菜单打开。".to_owned())?;
        note("launch", "requested");
    }
    state.finished.store(true, Ordering::Release);
    operation.finish("passed");
    app.exit(0);
    Ok(())
}

fn main() {
    let state = Arc::new(SetupState::default());
    note("host_started", "passed");
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            setup_info,
            setup_browse_directory,
            setup_install,
            setup_finish
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<Arc<SetupState>>();
                if state.busy.load(Ordering::Acquire) {
                    api.prevent_close();
                    note("close", "blocked_busy");
                } else {
                    api.prevent_close();
                    let completed = state.finished.load(Ordering::Acquire)
                        || state.installed.lock().is_ok_and(|value| value.is_some());
                    note("close", if completed { "completed" } else { "cancelled" });
                    window.app_handle().exit(if completed { 0 } else { 1602 });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("SayAll setup could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_recovers_from_empty_or_invalid_input_without_weakening_install_validation() {
        let default = PathBuf::from(r"C:\Apps\SayAll");
        for input in ["", "not a path", r"C:\"] {
            assert!(directory_path(input).is_err());
            assert_eq!(
                browse_initial_directory(input, || Ok(default.clone())).unwrap(),
                (default.clone(), true)
            );
        }
        assert_eq!(
            browse_initial_directory(r"D:\应用\SayAll", || panic!(
                "valid input keeps its location"
            ))
            .unwrap(),
            (PathBuf::from(r"D:\应用\SayAll"), false)
        );
    }

    #[test]
    fn partial_fresh_install_guidance_preserves_known_failure_messages() {
        assert!(install_exit_message(Some(2), true).contains("另一个空文件夹"));
        assert!(!install_exit_message(Some(2), false).contains("另一个空文件夹"));
        assert!(install_exit_message(Some(1638), true).contains("已有更新版本"));
        for code in [1639, 1640] {
            assert!(install_exit_message(Some(code), true).contains("尚未完成退出"));
        }
    }

    #[test]
    fn repeat_operation_is_rejected_and_failure_releases_the_gate() {
        let state = Arc::new(SetupState::default());
        let operation = Operation::begin(&state).unwrap();
        assert!(Operation::begin(&state).is_err());
        drop(operation);
        assert!(Operation::begin(&state).is_ok());
    }

    #[test]
    fn completed_finish_cannot_launch_twice() {
        let state = Arc::new(SetupState::default());
        state.finished.store(true, Ordering::Release);
        assert!(Operation::begin(&state).is_err());
    }

    #[test]
    fn unbound_or_modified_payload_is_rejected_before_launch() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sayall-setup-payload-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::write(&path, b"test-only payload").unwrap();
        assert!(checked_file(&path, "unbound").is_err());
        assert!(checked_file(&path, &"0".repeat(64)).is_err());
        let correct = format!("{:x}", Sha256::digest(b"test-only payload"));
        assert!(checked_file(&path, &correct).is_ok());
        // Retain this inert fixture for inspection (no product executable).
    }
}
