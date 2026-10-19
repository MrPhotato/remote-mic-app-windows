use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Running,
    Done,
}

/// Concurrent exit requests share one cleanup and must not confuse "started"
/// with "finished". A waiter that times out must prevent the Tauri exit.
pub struct ExitCleanup {
    phase: Mutex<Phase>,
    finished: Condvar,
}

impl ExitCleanup {
    pub const fn new() -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
            finished: Condvar::new(),
        }
    }

    pub fn run(&self, wait: Duration, cleanup: impl FnOnce()) -> bool {
        let mut phase = self.phase.lock().unwrap_or_else(|e| e.into_inner());
        match *phase {
            Phase::Done => true,
            Phase::Running => {
                let (phase, _) = self
                    .finished
                    .wait_timeout_while(phase, wait, |phase| *phase == Phase::Running)
                    .unwrap_or_else(|e| e.into_inner());
                *phase == Phase::Done
            }
            Phase::Idle => {
                *phase = Phase::Running;
                drop(phase);
                cleanup();
                *self.phase.lock().unwrap_or_else(|e| e.into_inner()) = Phase::Done;
                self.finished.notify_all();
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc};

    #[test]
    fn concurrent_exit_waits_for_the_first_cleanup_to_finish() {
        let gate = Arc::new(ExitCleanup::new());
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let first_gate = Arc::clone(&gate);
        let first = std::thread::spawn(move || {
            first_gate.run(Duration::from_secs(1), || {
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
            })
        });
        started_rx.recv().unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        let second_gate = Arc::clone(&gate);
        let second = std::thread::spawn(move || {
            let done = second_gate.run(Duration::from_secs(1), || panic!("cleanup must run once"));
            done_tx.send(done).unwrap();
        });
        assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
        finish_tx.send(()).unwrap();
        assert!(first.join().unwrap());
        assert!(done_rx.recv_timeout(Duration::from_secs(1)).unwrap());
        second.join().unwrap();
        assert!(gate.run(Duration::ZERO, || panic!(
            "finished cleanup must not repeat"
        )));
    }

    #[test]
    fn timed_out_waiter_does_not_report_cleanup_complete() {
        let gate = Arc::new(ExitCleanup::new());
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let worker_gate = Arc::clone(&gate);
        let worker = std::thread::spawn(move || {
            worker_gate.run(Duration::from_secs(1), || {
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
            })
        });
        started_rx.recv().unwrap();
        assert!(!gate.run(Duration::from_millis(10), || panic!("must not run twice")));
        finish_tx.send(()).unwrap();
        assert!(worker.join().unwrap());
        assert!(gate.run(Duration::ZERO, || panic!("must not run twice")));
    }
}
