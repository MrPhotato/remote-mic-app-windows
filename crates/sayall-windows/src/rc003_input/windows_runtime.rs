use super::Rc003InputStatus;
use crate::button_mapping::EngineMessage;
use crate::gatt_note;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows::core::{w, GUID, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const PAYLOAD_MANIFEST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/rc003-helper-manifest.json"));
const FRAME_LIMIT: usize = 4096;
type Availability = dyn Fn() -> (bool, u64, u64) + Send + Sync;

struct Worker {
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
    handle: JoinHandle<()>,
}

pub(crate) struct Rc003InputRuntime {
    sender: mpsc::Sender<EngineMessage>,
    status: Mutex<Rc003InputStatus>,
    selected: Mutex<Option<String>>,
    available: Arc<Availability>,
    worker: Mutex<Option<Worker>>,
    // Serializes terminal cancellation with generation changes and engine delivery only.
    // No network/process wait or diagnostic write may run while this gate is held.
    input_gate: Mutex<()>,
    next_generation: AtomicU64,
}

impl Rc003InputRuntime {
    pub fn new(sender: mpsc::Sender<EngineMessage>, available: Arc<Availability>) -> Self {
        Self {
            sender,
            status: Mutex::new(Rc003InputStatus::stopped()),
            selected: Mutex::new(None),
            available,
            worker: Mutex::new(None),
            input_gate: Mutex::new(()),
            next_generation: AtomicU64::new(1),
        }
    }

    pub fn snapshot(&self) -> Rc003InputStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn select_remote(&self, selected: Option<String>) {
        *self.selected.lock().unwrap() = selected;
    }

    fn set_phase(&self, phase: &str, reason: Option<&str>) {
        let message = {
            let mut status = self.status.lock().unwrap();
            let reason = reason.map(str::to_owned);
            if status.phase == phase && status.last_error == reason {
                return;
            }
            status.phase = phase.to_owned();
            status.last_error = reason;
            format!(
                "rc003_input phase={phase} reason={} generation={} report_count={} edge_count={}",
                status.last_error.as_deref().unwrap_or("none"),
                status.generation,
                status.report_count,
                status.edge_count
            )
        };
        gatt_note(message);
    }

    pub fn start(self: &Arc<Self>, payload: PathBuf) -> Result<Rc003InputStatus, String> {
        let mut worker = self.worker.lock().unwrap();
        if let Some(current) = worker.as_ref() {
            if !current.handle.is_finished() {
                return Ok(self.snapshot());
            }
        }
        if let Some(previous) = worker.take() {
            let _ = previous.handle.join();
        }
        if !(self.available)().0 || self.selected.lock().unwrap().is_none() {
            return Err("请先连接 RC003 遥控器并启动按键监听。".into());
        }
        let manifest: Value =
            serde_json::from_str(PAYLOAD_MANIFEST).map_err(|_| "Helper 清单无效")?;
        if manifest["files"]
            .as_array()
            .is_none_or(|files| files.is_empty())
            || !payload.join("SayAllKeyHelper.exe").is_file()
        {
            return Err("本地测试包缺少三键 Helper，请使用包含增强组件的完整安装包。".into());
        }
        let manifest_matches = std::fs::read(payload.join("manifest.json"))
            .is_ok_and(|bytes| bytes == PAYLOAD_MANIFEST.as_bytes());
        gatt_note(format!(
            "rc003_input payload_preflight phase=completed manifest_match={manifest_matches}"
        ));
        if !manifest_matches {
            return Err("三键增强组件清单与主程序不匹配，请安装完整测试包。".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (done_tx, done) = mpsc::channel();
        self.set_phase("starting", None);
        let runtime = Arc::clone(self);
        let stop_thread = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("sayall-rc003-input".into())
            .spawn(move || {
                let result = runtime.run(&payload, &stop_thread);
                let generation = runtime.snapshot().generation;
                let _ = runtime
                    .sender
                    .send(EngineMessage::Rc003TapLost { generation });
                match result {
                    Ok(()) => runtime.set_phase("stopped", None),
                    Err(code) if stop_thread.load(Ordering::SeqCst) => {
                        gatt_note(format!(
                            "rc003_input cleanup reason={code} generation={generation}"
                        ));
                        runtime.set_phase("stopped", None);
                    }
                    Err(code) => runtime.set_phase("failed", Some(code)),
                }
                let _ = done_tx.send(());
            })
            .map_err(|_| {
                self.set_phase("failed", Some("worker_start_failed"));
                "无法启动三键增强线程"
            })?;
        *worker = Some(Worker { stop, done, handle });
        Ok(self.snapshot())
    }

    pub fn stop(&self, timeout: Duration) -> Rc003InputStatus {
        let mut worker = self.worker.lock().unwrap();
        if let Some(current) = worker.as_ref() {
            self.request_stop(&current.stop);
            let _ = current.done.recv_timeout(timeout);
            if current.handle.is_finished() {
                if let Some(current) = worker.take() {
                    let _ = current.handle.join();
                }
            }
        }
        self.snapshot()
    }

    fn request_stop(&self, stop: &AtomicBool) {
        let _gate = self.input_gate.lock().unwrap();
        stop.store(true, Ordering::SeqCst);
        let generation = self.status.lock().unwrap().generation;
        // Any earlier send is before this Lost; all later starts/states see the stop token.
        let _ = self.sender.send(EngineMessage::Rc003TapLost { generation });
    }

    fn begin_generation(&self, stop: &AtomicBool) -> Option<u64> {
        let _gate = self.input_gate.lock().unwrap();
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        let previous = self.snapshot().generation;
        let _ = self.sender.send(EngineMessage::Rc003TapLost {
            generation: previous,
        });
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        self.status.lock().unwrap().generation = generation;
        let _ = self
            .sender
            .send(EngineMessage::Rc003TapStart { generation });
        Some(generation)
    }

    fn forward_state(
        &self,
        stop: &AtomicBool,
        generation: u64,
        sequence: u64,
        pressed_mask: u8,
        previous_mask: u8,
    ) -> Result<bool, &'static str> {
        let _gate = self.input_gate.lock().unwrap();
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        self.sender
            .send(EngineMessage::Rc003TapState {
                generation,
                sequence,
                pressed_mask,
            })
            .map_err(|_| "mapping_engine_unavailable")?;
        let mut status = self.status.lock().unwrap();
        status.report_count += 1;
        status.edge_count += (previous_mask ^ pressed_mask).count_ones() as u64;
        Ok(true)
    }

    fn run(&self, payload: &Path, stop: &AtomicBool) -> Result<(), &'static str> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "ipc_bind_failed")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "ipc_setup_failed")?;
        let port = listener
            .local_addr()
            .map_err(|_| "ipc_setup_failed")?
            .port();
        let token = format!(
            "{:032x}{:032x}",
            GUID::new().map_err(|_| "nonce_failed")?.to_u128(),
            GUID::new().map_err(|_| "nonce_failed")?.to_u128()
        );
        let bootstrap = launch_staged_helper(payload, port, &token)?;
        let accepted = (|| {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(90) && !stop.load(Ordering::SeqCst) {
                if bootstrap.exited() {
                    bootstrap.log_exit("startup_exited");
                    return Err("helper_start_failed");
                }
                match listener.accept() {
                    Ok((mut stream, peer)) => {
                        if !peer.ip().is_loopback() {
                            continue;
                        }
                        stream
                            .set_read_timeout(Some(Duration::from_millis(100)))
                            .map_err(|_| "ipc_setup_failed")?;
                        stream
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .map_err(|_| "ipc_setup_failed")?;
                        stream.set_nodelay(true).map_err(|_| "ipc_setup_failed")?;
                        let mut frames = Frames::default();
                        let hello_deadline = Instant::now();
                        while hello_deadline.elapsed() < Duration::from_secs(2)
                            && !stop.load(Ordering::SeqCst)
                        {
                            for hello in frames.read(&mut stream)? {
                                if hello["type"] == "hello"
                                    && hello["protocol"] == 1
                                    && hello["token"] == token
                                    && hello["pid"]
                                        .as_u64()
                                        .is_some_and(|pid| pid > 0 && pid <= u32::MAX as u64)
                                {
                                    gatt_note(
                                        "rc003_input ipc phase=authenticated protocol=1".into(),
                                    );
                                    return Ok((stream, frames));
                                }
                                return Err("ipc_authentication_failed");
                            }
                        }
                        return Err("ipc_authentication_timeout");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50))
                    }
                    Err(_) => return Err("ipc_accept_failed"),
                }
            }
            Err("helper_start_timeout")
        })();
        let (mut stream, mut frames) = accepted?;
        let mut generation = self.snapshot().generation;
        let mut last_selection = None;
        let mut last_lease = Instant::now() - Duration::from_secs(2);
        let mut last_contact = Instant::now();
        let mut last_sequence = None;
        let mut previous_mask = 0_u8;
        let mut armed = false;
        let result = (|| {
            loop {
                if stop.load(Ordering::SeqCst) {
                    return Ok(());
                }
                if bootstrap.exited() {
                    bootstrap.log_exit("runtime_exited");
                    return Err("helper_exited");
                }
                if last_contact.elapsed() > Duration::from_secs(5) {
                    return Err("helper_heartbeat_timeout");
                }
                let (available, connection_generation, input_epoch) = (self.available)();
                let selected = self.selected.lock().unwrap().clone();
                let selection = (
                    available,
                    connection_generation,
                    input_epoch,
                    selected.clone(),
                );
                if last_selection.as_ref() != Some(&selection) {
                    let Some(next_generation) = self.begin_generation(stop) else {
                        return Ok(());
                    };
                    generation = next_generation;
                    previous_mask = 0;
                    last_sequence = None;
                    armed = false;
                    last_selection = Some(selection);
                    last_lease = Instant::now() - Duration::from_secs(2);
                    self.set_phase(
                        "waiting",
                        Some(if available {
                            "awaiting_neutral"
                        } else {
                            "waiting_for_connection"
                        }),
                    );
                }
                if last_lease.elapsed() >= Duration::from_secs(1) {
                    send(
                        &mut stream,
                        &json!({"type":"lease", "generation":generation,
                        "enabled":available && selected.is_some(), "device_id":selected}),
                    )?;
                    last_lease = Instant::now();
                }
                for message in frames.read(&mut stream)? {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    let received_generation = message["generation"].as_u64();
                    if received_generation != Some(generation) {
                        continue;
                    }
                    last_contact = Instant::now();
                    match message["type"].as_str() {
                        Some("heartbeat") => {}
                        Some("state") if available && selected.is_some() => {
                            let sequence =
                                message["sequence"].as_u64().ok_or("ipc_invalid_state")?;
                            let mask = message["pressed_mask"]
                                .as_u64()
                                .filter(|mask| *mask <= 7)
                                .ok_or("ipc_invalid_state")?
                                as u8;
                            if last_sequence.is_some_and(|previous| sequence <= previous) {
                                continue;
                            }
                            last_sequence = Some(sequence);
                            if !armed {
                                if mask != 0 {
                                    continue;
                                }
                            }
                            if !self.forward_state(
                                stop,
                                generation,
                                sequence,
                                mask,
                                previous_mask,
                            )? {
                                return Ok(());
                            }
                            if !armed {
                                armed = true;
                                self.set_phase("ready", None);
                            }
                            previous_mask = mask;
                        }
                        Some("status") => {
                            let reason = safe_code(message["reason"].as_str())
                                .ok_or("ipc_invalid_status")?;
                            match message["phase"].as_str() {
                                Some("ready") => {} // A real neutral state, not a status label, arms input.
                                Some("waiting" | "failed") => {
                                    if armed {
                                        let Some(next_generation) = self.begin_generation(stop)
                                        else {
                                            return Ok(());
                                        };
                                        generation = next_generation;
                                        armed = false;
                                        previous_mask = 0;
                                        last_sequence = None;
                                        last_lease = Instant::now() - Duration::from_secs(2);
                                    }
                                    self.set_phase("waiting", Some(reason));
                                }
                                _ => return Err("ipc_invalid_status"),
                            }
                        }
                        Some("diagnostic") => {
                            if let Some(code) = safe_code(message["code"].as_str()) {
                                gatt_note(format!("rc003_input helper_diagnostic reason={code} generation={generation}"));
                            }
                        }
                        _ => return Err("ipc_unexpected_message"),
                    }
                }
            }
        })();
        let _ = self.sender.send(EngineMessage::Rc003TapLost { generation });
        let _ = send(&mut stream, &json!({"type":"stop"}));
        let _ = stream.shutdown(Shutdown::Both);
        let cleanup = bootstrap.wait(Duration::from_secs(3));
        gatt_note(format!(
            "rc003_input cleanup phase=completed helper_exited={cleanup} generation={generation}"
        ));
        result
    }
}

fn safe_code(code: Option<&str>) -> Option<&str> {
    code.filter(|code| {
        !code.is_empty()
            && code.len() <= 80
            && code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}

fn send(stream: &mut TcpStream, message: &Value) -> Result<(), &'static str> {
    let mut data = serde_json::to_vec(message).map_err(|_| "ipc_encode_failed")?;
    if data.len() > FRAME_LIMIT {
        return Err("ipc_frame_too_large");
    }
    data.push(b'\n');
    stream.write_all(&data).map_err(|_| "ipc_write_failed")
}

#[derive(Default)]
struct Frames {
    pending: Vec<u8>,
}
impl Frames {
    fn read(&mut self, stream: &mut TcpStream) -> Result<Vec<Value>, &'static str> {
        let mut buffer = [0_u8; 4096];
        match stream.read(&mut buffer) {
            Ok(0) => return Err("helper_disconnected"),
            Ok(count) => self.pending.extend_from_slice(&buffer[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err("ipc_read_failed"),
        }
        let mut result = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            if end > FRAME_LIMIT {
                return Err("ipc_frame_too_large");
            }
            let row = self.pending.drain(..=end).collect::<Vec<_>>();
            result.push(serde_json::from_slice(&row).map_err(|_| "ipc_invalid_json")?);
        }
        if self.pending.len() > FRAME_LIMIT {
            return Err("ipc_frame_too_large");
        }
        Ok(result)
    }
}

struct Bootstrap(HANDLE);
impl Bootstrap {
    fn exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
    fn wait(&self, timeout: Duration) -> bool {
        unsafe { WaitForSingleObject(self.0, timeout.as_millis() as u32) == WAIT_OBJECT_0 }
    }
    fn log_exit(&self, phase: &str) {
        let mut code = 0_u32;
        if unsafe { GetExitCodeProcess(self.0, &mut code) }.is_ok() {
            gatt_note(format!(
                "rc003_input helper_process phase={phase} exit_code={code}"
            ));
        } else {
            gatt_note(format!(
                "rc003_input helper_process phase={phase} exit_code=unknown"
            ));
        }
    }
}
impl Drop for Bootstrap {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn powershell_source_literal(payload: &Path) -> Result<String, &'static str> {
    // resource_dir can be a verbatim Windows path. Windows PowerShell 5.1's
    // filesystem provider rejects that prefix in Join-Path (unlike PowerShell 7).
    // Match Tauri's public path resolver: simplify only when semantics are safe.
    dunce::simplified(payload)
        .to_str()
        .map(|source| source.replace('\'', "''"))
        .ok_or("payload_path_invalid")
}

fn launch_staged_helper(payload: &Path, port: u16, token: &str) -> Result<Bootstrap, &'static str> {
    let digest = format!("{:x}", Sha256::digest(PAYLOAD_MANIFEST.as_bytes()));
    let source = powershell_source_literal(payload)?;
    gatt_note(format!(
        "rc003_input bootstrap_path phase=prepared simplified={}",
        dunce::simplified(payload) != payload
    ));
    let script = include_str!("stage-helper.ps1")
        .replace("__SOURCE__", &source)
        .replace("__DIGEST__", &digest)
        .replace("__PORT__", &port.to_string())
        .replace("__TOKEN__", token)
        .replace("__PARENT__", &std::process::id().to_string());
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let arguments: Vec<u16> =
        format!("-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand {encoded}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
    let mut system = [0_u16; 32768];
    let count = unsafe { GetSystemDirectoryW(Some(&mut system)) } as usize;
    if count == 0 || count >= system.len() {
        return Err("system_directory_failed");
    }
    let file: Vec<u16> = format!(
        "{}\\WindowsPowerShell\\v1.0\\powershell.exe",
        String::from_utf16_lossy(&system[..count])
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(arguments.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|error| {
        if error.code().0 as u32 == 0x800704c7 {
            "elevation_cancelled"
        } else {
            "helper_launch_failed"
        }
    })?;
    if info.hProcess.is_invalid() {
        return Err("helper_process_handle_missing");
    }
    gatt_note(
        "rc003_input helper_launch phase=completed result=started main_elevated=false".into(),
    );
    Ok(Bootstrap(info.hProcess))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_source_uses_compatible_paths_and_quotes_literals() {
        for (source, expected) in [
            (
                r"\\?\C:\Program Files\SayAll\rc003-helper",
                r"C:\Program Files\SayAll\rc003-helper",
            ),
            (
                r"\\?\UNC\server\share\SayAll\rc003-helper",
                r"\\?\UNC\server\share\SayAll\rc003-helper",
            ),
            (
                r"C:\User's Apps\SayAll\rc003-helper",
                r"C:\User''s Apps\SayAll\rc003-helper",
            ),
        ] {
            assert_eq!(
                powershell_source_literal(Path::new(source)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn stop_after_loop_check_rejects_late_generation_and_state() {
        let (sender, events) = mpsc::channel();
        let runtime = Arc::new(Rc003InputRuntime::new(sender, Arc::new(|| (true, 1, 0))));
        let stop = Arc::new(AtomicBool::new(false));
        let generation = runtime.begin_generation(&stop).unwrap();
        let _ = events.try_iter().collect::<Vec<_>>();
        let (checked, checked_rx) = mpsc::channel();
        let (resume, resume_rx) = mpsc::channel();
        let input = Arc::clone(&runtime);
        let input_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            // Pause at the old race window: the loop check passed, but no send occurred.
            assert!(!input_stop.load(Ordering::SeqCst));
            checked.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(input.begin_generation(&input_stop), None);
            assert!(!input
                .forward_state(&input_stop, generation, 1, 1, 0)
                .unwrap());
        });
        checked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        runtime.request_stop(&stop);
        resume.send(()).unwrap();
        worker.join().unwrap();
        let messages = events.try_iter().collect::<Vec<_>>();
        assert!(
            matches!(messages.as_slice(), [EngineMessage::Rc003TapLost { generation: lost }] if *lost == generation)
        );
        assert_eq!(runtime.snapshot().report_count, 0);
        assert_eq!(runtime.snapshot().generation, generation);
    }

    #[test]
    fn stop_cancels_the_generation_that_won_the_gate_before_it() {
        let (sender, events) = mpsc::channel();
        let runtime = Arc::new(Rc003InputRuntime::new(sender, Arc::new(|| (true, 1, 0))));
        let stop = Arc::new(AtomicBool::new(false));
        let first = runtime.begin_generation(&stop).unwrap();
        let _ = events.try_iter().collect::<Vec<_>>();
        let (forwarded, forwarded_rx) = mpsc::channel();
        let (resume, resume_rx) = mpsc::channel();
        let input = Arc::clone(&runtime);
        let input_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let generation = input.begin_generation(&input_stop).unwrap();
            assert!(input
                .forward_state(&input_stop, generation, 1, 0, 0)
                .unwrap());
            assert!(input
                .forward_state(&input_stop, generation, 2, 1, 0)
                .unwrap());
            forwarded.send(generation).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(!input
                .forward_state(&input_stop, generation, 3, 0, 1)
                .unwrap());
        });
        let current = forwarded_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        runtime.request_stop(&stop);
        resume.send(()).unwrap();
        worker.join().unwrap();
        let messages = events.try_iter().collect::<Vec<_>>();
        assert!(matches!(messages.as_slice(), [
            EngineMessage::Rc003TapLost { generation: replaced },
            EngineMessage::Rc003TapStart { generation: started },
            EngineMessage::Rc003TapState { generation: neutral, pressed_mask: 0, .. },
            EngineMessage::Rc003TapState { generation: pressed, pressed_mask: 1, .. },
            EngineMessage::Rc003TapLost { generation: stopped },
        ] if *replaced == first && *started == current && *neutral == current && *pressed == current && *stopped == current));
        assert_eq!(runtime.snapshot().report_count, 2);
        assert_eq!(runtime.snapshot().edge_count, 1);
    }

    #[test]
    fn protocol_diagnostics_reject_identity_and_paths() {
        assert_eq!(
            safe_code(Some("awaiting_neutral")),
            Some("awaiting_neutral")
        );
        for value in ["", "C:\\private", "a-b", "{identity}", "bad\nline"] {
            assert_eq!(safe_code(Some(value)), None);
        }
    }
    #[test]
    fn fragmented_frames_and_oversized_input_are_bounded() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut writer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut reader, _) = listener.accept().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut frames = Frames::default();
        writer.write_all(b"{\"type\":").unwrap();
        assert!(frames.read(&mut reader).unwrap().is_empty());
        writer.write_all(b"\"heartbeat\"}\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let messages = frames.read(&mut reader).unwrap();
            if !messages.is_empty() {
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0]["type"], "heartbeat");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fragmented frame did not complete"
            );
        }
        writer.write_all(&vec![b'x'; FRAME_LIMIT + 1]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match frames.read(&mut reader) {
                Ok(messages) => assert!(messages.is_empty()),
                Err(error) => {
                    assert_eq!(error, "ipc_frame_too_large");
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "oversized frame was not rejected"
            );
        }
    }
}
