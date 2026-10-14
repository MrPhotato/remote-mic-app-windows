//! Recoverable diagnostic file output. Error status never includes paths or records.
//!
//! The caller owns record formatting. Recovery formatting must not call the logger:
//! it runs while the sink lock is held. Failed records are counted, never buffered
//! or replayed (a failed write/flush may already have written part of a record).

use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const RETRY_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticLogHealth {
    pub configured: bool,
    pub ready: bool,
    pub dropped_records: u64,
    pub open_failures: u64,
    pub write_failures: u64,
    pub flush_failures: u64,
    pub recoveries: u64,
    pub last_error_code: Option<&'static str>,
}

/// Cumulative counters at the next successful write after an unavailable period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticRecovery {
    pub dropped_records: u64,
    pub open_failures: u64,
    pub write_failures: u64,
    pub flush_failures: u64,
}

pub struct DiagnosticSink {
    state: Mutex<SinkState<File>>,
}

impl Default for DiagnosticSink {
    fn default() -> Self {
        Self {
            state: Mutex::new(SinkState::default()),
        }
    }
}

impl DiagnosticSink {
    /// The first configured path remains authoritative for this sink's lifetime.
    /// An earlier record without a configured path does not prevent initialization.
    pub fn configure(&self, path: PathBuf) -> bool {
        self.lock().configure(path, Instant::now(), &mut open_log)
    }

    /// Append one record and flush it. False means delivery was not confirmed.
    /// A recovery marker uses the same timestamp/version prefix as other records.
    pub fn write_record(
        &self,
        record: &str,
        recovery_record: impl FnOnce(DiagnosticRecovery) -> String,
    ) -> bool {
        self.lock()
            .write_record(record, recovery_record, Instant::now(), &mut open_log)
    }

    /// A memory-only read: querying health never opens or writes a file.
    pub fn health(&self) -> DiagnosticLogHealth {
        self.lock().health()
    }

    fn lock(&self) -> MutexGuard<'_, SinkState<File>> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

fn open_log(path: &Path) -> io::Result<File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

struct SinkState<W> {
    path: Option<PathBuf>,
    writer: Option<W>,
    retry_after: Option<Instant>,
    pending_recovery: bool,
    pending_partial_record: bool,
    health: DiagnosticLogHealth,
}

impl<W> Default for SinkState<W> {
    fn default() -> Self {
        Self {
            path: None,
            writer: None,
            retry_after: None,
            pending_recovery: false,
            pending_partial_record: false,
            health: DiagnosticLogHealth::default(),
        }
    }
}

impl<W: Write> SinkState<W> {
    fn configure(
        &mut self,
        path: PathBuf,
        now: Instant,
        open: &mut impl FnMut(&Path) -> io::Result<W>,
    ) -> bool {
        if self.path.is_none() {
            self.path = Some(path);
        }
        self.ensure_open(now, open)
    }

    fn health(&self) -> DiagnosticLogHealth {
        DiagnosticLogHealth {
            configured: self.path.is_some(),
            ready: self.writer.is_some(),
            ..self.health.clone()
        }
    }

    fn ensure_open(&mut self, now: Instant, open: &mut impl FnMut(&Path) -> io::Result<W>) -> bool {
        if self.writer.is_some() {
            return true;
        }
        let Some(path) = self.path.as_deref() else {
            self.health.last_error_code = Some("not_configured");
            self.pending_recovery = true;
            return false;
        };
        if self.retry_after.is_some_and(|deadline| now < deadline) {
            return false;
        }
        match open(path) {
            Ok(writer) => {
                self.writer = Some(writer);
                self.retry_after = None;
                self.health.last_error_code = None;
                true
            }
            Err(_) => {
                self.health.open_failures = self.health.open_failures.saturating_add(1);
                self.mark_unavailable("open_failed", now);
                false
            }
        }
    }

    fn write_record(
        &mut self,
        record: &str,
        recovery_record: impl FnOnce(DiagnosticRecovery) -> String,
        now: Instant,
        open: &mut impl FnMut(&Path) -> io::Result<W>,
    ) -> bool {
        if !self.ensure_open(now, open) {
            self.health.dropped_records = self.health.dropped_records.saturating_add(1);
            return false;
        }
        let recovering = self.pending_recovery;
        let recovery = recovering.then(|| {
            recovery_record(DiagnosticRecovery {
                dropped_records: self.health.dropped_records,
                open_failures: self.health.open_failures,
                write_failures: self.health.write_failures,
                flush_failures: self.health.flush_failures,
            })
        });
        let pending_partial_record = self.pending_partial_record;
        let writer = self.writer.as_mut().expect("ensure_open supplied a writer");
        let result = (|| {
            if let Some(recovery) = recovery.as_deref() {
                if pending_partial_record {
                    writer.write_all(b"\n").map_err(|_| "write_failed")?;
                }
                append_record(writer, recovery).map_err(|_| "write_failed")?;
            }
            append_record(writer, record).map_err(|_| "write_failed")?;
            writer.flush().map_err(|_| "flush_failed")
        })();
        match result {
            Ok(()) => {
                if recovering {
                    self.health.recoveries = self.health.recoveries.saturating_add(1);
                }
                self.pending_recovery = false;
                self.pending_partial_record = false;
                self.health.last_error_code = None;
                true
            }
            Err(code) => {
                if code == "flush_failed" {
                    self.health.flush_failures = self.health.flush_failures.saturating_add(1);
                } else {
                    self.health.write_failures = self.health.write_failures.saturating_add(1);
                }
                self.health.dropped_records = self.health.dropped_records.saturating_add(1);
                self.mark_unavailable(code, now);
                false
            }
        }
    }

    fn mark_unavailable(&mut self, code: &'static str, now: Instant) {
        self.writer = None;
        self.retry_after = Some(now + RETRY_INTERVAL);
        self.pending_recovery = true;
        self.pending_partial_record |= code == "write_failed";
        self.health.last_error_code = Some(code);
    }
}

fn append_record(writer: &mut impl Write, record: &str) -> io::Result<()> {
    writer.write_all(record.as_bytes())?;
    writer.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct FakeWriter {
        bytes: Arc<Mutex<Vec<u8>>>,
        fail_write: bool,
        partial_write: bool,
        fail_flush: bool,
    }

    impl Write for FakeWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.partial_write && !bytes.is_empty() {
                self.bytes.lock().unwrap().push(bytes[0]);
                self.partial_write = false;
                self.fail_write = true;
                return Ok(1);
            }
            if self.fail_write {
                return Err(io::Error::other("simulated write failure"));
            }
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("simulated flush failure"))
            } else {
                Ok(())
            }
        }
    }

    fn recovery_record(recovery: DiagnosticRecovery) -> String {
        format!(
            "diagnostic_sink event=recovered dropped_records={} open_failures={} write_failures={} flush_failures={}",
            recovery.dropped_records,
            recovery.open_failures,
            recovery.write_failures,
            recovery.flush_failures,
        )
    }

    #[test]
    fn early_unconfigured_record_does_not_disable_later_configuration() {
        let mut state = SinkState::<FakeWriter>::default();
        let now = Instant::now();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut open = |_: &Path| {
            Ok(FakeWriter {
                bytes: Arc::clone(&bytes),
                ..FakeWriter::default()
            })
        };
        assert!(!state.write_record("early", recovery_record, now, &mut open));
        assert_eq!(state.health().last_error_code, Some("not_configured"));
        assert!(state.configure(PathBuf::from("log"), now, &mut open));
        assert!(state.write_record("ready", recovery_record, now, &mut open));
        assert_eq!(state.health().dropped_records, 1);
        assert_eq!(state.health().recoveries, 1);
        let text = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(text.contains("event=recovered dropped_records=1 open_failures=0"));
        assert!(text.ends_with("ready\n"));
        assert!(!text.contains("early"));
    }

    #[test]
    fn failed_open_is_throttled_and_retries_at_the_deadline() {
        let mut state = SinkState::<FakeWriter>::default();
        let now = Instant::now();
        let attempts = Cell::new(0);
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut open = |_: &Path| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                Err(io::Error::other("simulated open failure"))
            } else {
                Ok(FakeWriter {
                    bytes: Arc::clone(&bytes),
                    ..FakeWriter::default()
                })
            }
        };
        assert!(!state.configure(PathBuf::from("log"), now, &mut open));
        assert!(!state.write_record("dropped", recovery_record, now, &mut open));
        assert!(!state.write_record(
            "still_throttled",
            recovery_record,
            now + RETRY_INTERVAL - Duration::from_millis(1),
            &mut open,
        ));
        assert_eq!(attempts.get(), 1);
        assert!(state.write_record("restored", recovery_record, now + RETRY_INTERVAL, &mut open));
        assert_eq!(attempts.get(), 2);
        assert_eq!(state.health().open_failures, 1);
        assert_eq!(state.health().dropped_records, 2);
        assert_eq!(state.health().recoveries, 1);
        assert!(state.health().ready);
        let text = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(text.contains("event=recovered dropped_records=2 open_failures=1"));
        assert!(text.ends_with("restored\n"));
    }

    #[test]
    fn write_and_flush_failures_discard_the_handle_then_recover() {
        for fail_flush in [false, true] {
            let mut state = SinkState::<FakeWriter>::default();
            let now = Instant::now();
            let attempts = Cell::new(0);
            let recovered_bytes = Arc::new(Mutex::new(Vec::new()));
            let mut open = |_: &Path| {
                attempts.set(attempts.get() + 1);
                if attempts.get() == 1 {
                    Ok(FakeWriter {
                        fail_write: !fail_flush,
                        fail_flush,
                        ..FakeWriter::default()
                    })
                } else {
                    Ok(FakeWriter {
                        bytes: Arc::clone(&recovered_bytes),
                        ..FakeWriter::default()
                    })
                }
            };
            assert!(state.configure(PathBuf::from("log"), now, &mut open));
            assert!(!state.write_record("failed_record", recovery_record, now, &mut open));
            assert!(!state.health().ready);
            assert_eq!(
                state.health().last_error_code,
                Some(if fail_flush {
                    "flush_failed"
                } else {
                    "write_failed"
                }),
            );
            assert!(!state.write_record("throttled", recovery_record, now, &mut open));
            assert_eq!(attempts.get(), 1);
            assert!(state.write_record("next", recovery_record, now + RETRY_INTERVAL, &mut open));
            assert_eq!(state.health().write_failures, u64::from(!fail_flush));
            assert_eq!(state.health().flush_failures, u64::from(fail_flush));
            assert_eq!(state.health().dropped_records, 2);
            assert_eq!(state.health().recoveries, 1);
            assert_eq!(state.health().last_error_code, None);
            let text = String::from_utf8(recovered_bytes.lock().unwrap().clone()).unwrap();
            assert!(text.contains("diagnostic_sink event=recovered dropped_records=2"));
            assert!(text.ends_with("next\n"));
            assert!(!text.contains("failed_record"));
            assert!(!text.contains("throttled"));
        }
    }

    #[test]
    fn partial_record_recovery_starts_on_a_new_line() {
        let mut state = SinkState::<FakeWriter>::default();
        let now = Instant::now();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let attempts = Cell::new(0);
        let mut open = |_: &Path| {
            attempts.set(attempts.get() + 1);
            Ok(FakeWriter {
                bytes: Arc::clone(&bytes),
                partial_write: attempts.get() == 1,
                ..FakeWriter::default()
            })
        };
        assert!(state.configure(PathBuf::from("log"), now, &mut open));
        assert!(!state.write_record("partial", recovery_record, now, &mut open));
        assert!(state.write_record("next", recovery_record, now + RETRY_INTERVAL, &mut open));
        let text = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(text.starts_with("p\ndiagnostic_sink event=recovered"));
        assert!(text.ends_with("next\n"));
    }

    #[test]
    fn health_is_serializable_without_paths_or_record_content() {
        let mut state = SinkState::<FakeWriter>::default();
        let mut open = |_: &Path| Err(io::Error::other("private failure detail"));
        let now = Instant::now();
        assert!(!state.configure(PathBuf::from("private-directory/log"), now, &mut open));
        assert!(!state.write_record("private record", recovery_record, now, &mut open));
        let json = serde_json::to_string(&state.health()).unwrap();
        assert!(json.contains("\"lastErrorCode\":\"open_failed\""));
        assert!(json.contains("\"droppedRecords\":1"));
        assert!(!json.contains("private"));
    }

    #[test]
    fn filesystem_parent_obstruction_recovers_after_real_retry_interval() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "sayall-diagnostic-sink-test-{}-{nonce}",
            std::process::id(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let parent = directory.join("parent");
        std::fs::write(&parent, b"temporary parent obstruction").unwrap();
        let path = parent.join("diagnostic.log");
        let sink = DiagnosticSink::default();
        let started = Instant::now();
        assert!(!sink.configure(path.clone()));
        assert_eq!(sink.health().last_error_code, Some("open_failed"));
        // Preserve the obstruction; test cleanup must not permanently delete files.
        std::fs::rename(&parent, directory.join("parent-obstruction.saved")).unwrap();
        assert!(!sink.write_record("during_throttle", recovery_record));
        assert_eq!(sink.health().open_failures, 1);
        assert!(!path.exists());
        let delay = RETRY_INTERVAL.saturating_sub(started.elapsed()) + Duration::from_millis(100);
        std::thread::sleep(delay);
        assert!(sink.write_record("filesystem_recovered", recovery_record));
        let recovery_elapsed = started.elapsed();
        let health = sink.health();
        assert!(health.ready);
        assert_eq!(health.open_failures, 1);
        assert_eq!(health.dropped_records, 1);
        assert_eq!(health.recoveries, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("event=recovered dropped_records=1 open_failures=1"));
        assert!(text.ends_with("filesystem_recovered\n"));
        println!(
            "diagnostic_sink_filesystem_recovery retry_interval_ms={} recovery_elapsed_ms={} open_failures={} dropped_records={} recoveries={}",
            RETRY_INTERVAL.as_millis(),
            recovery_elapsed.as_millis(),
            health.open_failures,
            health.dropped_records,
            health.recoveries,
        );
        drop(sink);
        // Retain a marked fixture directory for the caller's recycle-bin cleanup.
        std::fs::rename(&directory, directory.with_extension("completed")).unwrap();
    }
}
