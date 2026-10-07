//! Public logger integration, isolated from other integration-test processes.

use sayall_windows::{
    diagnostic_log_health, gatt_note, initialize_diagnostic_log, DiagnosticLogMetadata,
};
use std::ffi::OsString;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct RestoreLogEnvironment(Option<OsString>);

impl Drop for RestoreLogEnvironment {
    fn drop(&mut self) {
        // SAFETY: This integration-test executable contains one test and starts
        // no application workers; no other thread reads this variable.
        unsafe {
            match self.0.as_ref() {
                Some(value) => std::env::set_var("SAYALL_GATT_LOG", value),
                None => std::env::remove_var("SAYALL_GATT_LOG"),
            }
        }
    }
}

fn metadata() -> DiagnosticLogMetadata {
    DiagnosticLogMetadata {
        app_version: "0.0.0-recovery-test".to_owned(),
        app_build: "7".to_owned(),
        source_revision: "0123456789012345678901234567890123456789".to_owned(),
        build_channel: "local".to_owned(),
        release_tag: "none".to_owned(),
    }
}

fn assert_utc_timestamp(line: &str) {
    let timestamp = line.split_whitespace().next().unwrap();
    assert_eq!(timestamp.len(), 24);
    let delimiters = [
        (4, b'-'),
        (7, b'-'),
        (10, b'T'),
        (13, b':'),
        (16, b':'),
        (19, b'.'),
        (23, b'Z'),
    ];
    for (index, byte) in timestamp.bytes().enumerate() {
        if let Some((_, delimiter)) = delimiters.iter().find(|(position, _)| *position == index) {
            assert_eq!(byte, *delimiter);
        } else {
            assert!(byte.is_ascii_digit());
        }
    }
}

#[test]
fn public_logger_recovers_after_early_write_and_filesystem_failure() {
    let _restore = RestoreLogEnvironment(std::env::var_os("SAYALL_GATT_LOG"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "sayall-public-log-recovery-test-{}-{nonce}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&directory).unwrap();

    // A health query must not initialize the sink, even with an environment path.
    let environment_probe = directory.join("environment-probe.log");
    // SAFETY: This standalone integration-test executable has one test and no
    // application workers, so the logger cannot race these environment changes.
    unsafe { std::env::set_var("SAYALL_GATT_LOG", &environment_probe) };
    let initial = diagnostic_log_health();
    assert!(!initial.configured);
    assert!(!initial.ready);
    assert_eq!(initial.open_failures, 0);
    assert_eq!(initial.dropped_records, 0);
    assert!(!environment_probe.exists());
    unsafe { std::env::remove_var("SAYALL_GATT_LOG") };

    // This used to initialize OnceLock<Option<File>> to None permanently.
    gatt_note("diagnostic_recovery_test marker=early_unconfigured".to_owned());
    let early = diagnostic_log_health();
    assert!(!early.configured);
    assert!(!early.ready);
    assert_eq!(early.dropped_records, 1);
    assert_eq!(early.last_error_code, Some("not_configured"));

    let parent = directory.join("parent");
    std::fs::write(&parent, b"temporary parent obstruction").unwrap();
    let path = parent.join("diagnostic.log");
    let started = Instant::now();
    assert!(!initialize_diagnostic_log(path.clone(), metadata()));
    let failed = diagnostic_log_health();
    assert!(failed.configured);
    assert!(!failed.ready);
    assert_eq!(failed.open_failures, 1);
    assert_eq!(failed.last_error_code, Some("open_failed"));

    // Neither repeated initialization nor a record should bypass failure throttle.
    assert!(!initialize_diagnostic_log(path.clone(), metadata()));
    gatt_note("diagnostic_recovery_test marker=blocked_record".to_owned());
    assert_eq!(diagnostic_log_health().open_failures, 1);
    assert_eq!(diagnostic_log_health().dropped_records, 2);

    // Keep the obstacle for recycle-bin cleanup instead of permanently deleting it.
    std::fs::rename(&parent, directory.join("parent-obstruction.saved")).unwrap();
    gatt_note("diagnostic_recovery_test marker=still_throttled".to_owned());
    assert_eq!(diagnostic_log_health().open_failures, 1);
    assert_eq!(diagnostic_log_health().dropped_records, 3);
    assert!(!path.exists());
    assert!(!environment_probe.exists());

    // Use the real clock and filesystem to exercise the production five-second
    // retry boundary. This is failure-retry verification, not a latency claim.
    std::thread::sleep(Duration::from_millis(5_100).saturating_sub(started.elapsed()));
    gatt_note("diagnostic_recovery_test phase=completed terminal_result=passed".to_owned());
    let recovery_elapsed = started.elapsed();
    let recovered = diagnostic_log_health();
    assert!(recovered.ready);
    assert_eq!(recovered.dropped_records, 3);
    assert_eq!(recovered.open_failures, 1);
    assert_eq!(recovered.write_failures, 0);
    assert_eq!(recovered.flush_failures, 0);
    assert_eq!(recovered.recoveries, 1);
    assert_eq!(recovered.last_error_code, None);

    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = contents.lines().collect();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        assert_utc_timestamp(line);
        assert!(line.contains("ver=0.0.0-recovery-test build=7"));
        assert!(line.contains("source_revision=0123456789012345678901234567890123456789"));
        assert!(line.contains("build_channel=local release_tag=none"));
    }
    assert!(lines[0].contains("diagnostic_sink phase=recovered terminal_result=passed"));
    assert!(
        lines[0].contains("dropped_records=3 open_failures=1 write_failures=0 flush_failures=0")
    );
    assert!(lines[1].contains("diagnostic_recovery_test phase=completed terminal_result=passed"));
    assert!(!contents.contains("early_unconfigured"));
    assert!(!contents.contains("blocked_record"));
    assert!(!contents.contains("still_throttled"));
    assert!(!contents.contains("environment-probe"));
    let health_json = serde_json::to_string(&recovered).unwrap();
    assert!(health_json.contains("\"droppedRecords\":3"));
    assert!(health_json.contains("\"lastErrorCode\":null"));
    assert!(!health_json.contains("path"));

    println!(
        "public_diagnostic_log_recovery retry_interval_ms=5000 recovery_elapsed_ms={} dropped_records={} open_failures={} recoveries={}",
        recovery_elapsed.as_millis(),
        recovered.dropped_records,
        recovered.open_failures,
        recovered.recoveries,
    );
    // The isolated test process will close the global sink at process exit.
    // Windows may retain the file handle until process exit, so mark the
    // retained fixture in place for the caller's recycle-bin cleanup.
    std::fs::write(
        directory.join(".completed"),
        b"public logger recovery passed\n",
    )
    .unwrap();
}
