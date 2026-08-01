//! Process supervision: spawns each managed command in its own process group,
//! restarts it on crash with exponential backoff, and handles graceful stops
//! (SIGTERM, then SIGKILL after a grace period).

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::logs::LogManager;
use super::state::Store;
use crate::protocol::{LogStream, ProcessStatus};
use crate::util::now_ts;

/// Delay before restart attempt `n` (0-based): min(1s * 2^n, 30s).
pub fn backoff_delay(attempt: u32) -> Duration {
    Duration::from_secs((1u64 << attempt.min(6)).min(30))
}

/// A run lasting at least this long resets the backoff counter.
const STABLE_AFTER: Duration = Duration::from_secs(60);
/// How long SIGTERM gets before we escalate to SIGKILL.
const KILL_GRACE: Duration = Duration::from_secs(5);

struct ProcHandle {
    stop_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

pub struct Supervisor {
    store: Arc<Store>,
    logs: Arc<LogManager>,
    procs: Mutex<HashMap<String, ProcHandle>>,
}

impl Supervisor {
    pub fn new(store: Arc<Store>, logs: Arc<LogManager>) -> Self {
        Self {
            store,
            logs,
            procs: Mutex::new(HashMap::new()),
        }
    }

    /// Whether a supervision task is currently active for `name`.
    pub fn is_active(&self, name: &str) -> bool {
        self.procs
            .lock()
            .unwrap()
            .get(name)
            .is_some_and(|h| !h.task.is_finished())
    }

    /// Begin supervising `name` (must exist in the store).
    pub fn start(&self, name: &str) -> Result<(), String> {
        if !self.store.contains(name) {
            return Err(format!("no such process: {name}"));
        }
        let mut procs = self.procs.lock().unwrap();
        if procs.get(name).is_some_and(|h| !h.task.is_finished()) {
            return Err(format!("process '{name}' is already running"));
        }
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(supervise(
            name.to_string(),
            self.store.clone(),
            self.logs.clone(),
            stop_rx,
        ));
        procs.insert(name.to_string(), ProcHandle { stop_tx, task });
        Ok(())
    }

    /// Stop supervising `name`, terminating the child if running.
    pub async fn stop(&self, name: &str) -> Result<(), String> {
        let handle = self.procs.lock().unwrap().remove(name);
        let Some(handle) = handle else {
            return Err(format!("process '{name}' is not running"));
        };
        let _ = handle.stop_tx.send(true);
        let _ = handle.task.await;
        Ok(())
    }
}

async fn supervise(
    name: String,
    store: Arc<Store>,
    logs: Arc<LogManager>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut attempt: u32 = 0;
    let mut first_run = true;

    loop {
        let Some(info) = store.info(&name) else {
            return; // removed underneath us
        };

        let mut child = match spawn_child(&info.command, &info.cwd) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("rr: failed to spawn '{name}': {e}");
                store.update(&name, |p| {
                    p.status = ProcessStatus::Crashed;
                    p.pid = None;
                    p.last_crash_at = Some(now_ts());
                });
                return;
            }
        };

        let pid = child.id();
        if !first_run {
            store.update(&name, |p| p.restarts += 1);
        }
        first_run = false;
        store.update(&name, |p| {
            p.status = ProcessStatus::Running;
            p.pid = pid;
            p.started_at = Some(now_ts());
        });

        if let Some(out) = child.stdout.take() {
            logs.pipe(name.clone(), LogStream::Stdout, out);
        }
        if let Some(err) = child.stderr.take() {
            logs.pipe(name.clone(), LogStream::Stderr, err);
        }

        let started = tokio::time::Instant::now();
        tokio::select! {
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code());
                if started.elapsed() >= STABLE_AFTER {
                    attempt = 0;
                }
                store.update(&name, |p| {
                    p.status = ProcessStatus::Backoff;
                    p.pid = None;
                    p.last_exit_code = code;
                    p.last_crash_at = Some(now_ts());
                });
                let delay = backoff_delay(attempt);
                attempt += 1;
                tokio::select! {
                    _ = tokio::time::sleep(delay) => continue,
                    _ = stop_rx.changed() => {
                        store.update(&name, |p| p.status = ProcessStatus::Stopped);
                        return;
                    }
                }
            }
            _ = stop_rx.changed() => {
                terminate(&mut child).await;
                store.update(&name, |p| {
                    p.status = ProcessStatus::Stopped;
                    p.pid = None;
                });
                return;
            }
        }
    }
}

fn spawn_child(command: &str, cwd: &str) -> std::io::Result<Child> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0) // own group so we can signal the whole tree
        .kill_on_drop(true)
        .spawn()
}

/// SIGTERM the child's process group, escalating to SIGKILL after the grace
/// period.
async fn terminate(child: &mut Child) {
    let Some(pid) = child.id() else {
        return; // already reaped
    };
    signal_group(pid, Signal::SIGTERM);
    if tokio::time::timeout(KILL_GRACE, child.wait()).await.is_err() {
        signal_group(pid, Signal::SIGKILL);
        let _ = child.wait().await;
    }
}

/// Signal the child's whole process group (pgid == child pid, because we
/// spawn with `process_group(0)`).
fn signal_group(pid: u32, sig: Signal) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    let _ = killpg(Pid::from_raw(pid), sig);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Arc<Store>, Arc<LogManager>, Supervisor) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::load(dir.path().join("state.json")).unwrap());
        let logs = Arc::new(LogManager::new(dir.path().join("logs")).unwrap());
        let sup = Supervisor::new(store.clone(), logs.clone());
        (dir, store, logs, sup)
    }

    fn spec(name: &str, command: &str) -> super::super::state::ProcSpec {
        super::super::state::ProcSpec {
            name: name.into(),
            command: command.into(),
            cwd: "/tmp".into(),
            created_at: now_ts(),
        }
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let secs: Vec<u64> = (0..7).map(|n| backoff_delay(n).as_secs()).collect();
        assert_eq!(secs, vec![1, 2, 4, 8, 16, 30, 30]);
        assert_eq!(backoff_delay(63).as_secs(), 30);
    }

    #[tokio::test]
    async fn start_and_stop_long_running_process() {
        let (_dir, store, _logs, sup) = setup();
        store.insert(spec("api", "sleep 30")).unwrap();
        sup.start("api").unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;
        let info = store.info("api").unwrap();
        assert_eq!(info.status, ProcessStatus::Running);
        assert!(info.pid.is_some());

        sup.stop("api").await.unwrap();
        let info = store.info("api").unwrap();
        assert_eq!(info.status, ProcessStatus::Stopped);
        assert_eq!(info.pid, None);
        assert!(!sup.is_active("api"));
    }

    #[tokio::test]
    async fn crash_records_exit_code_and_restarts() {
        let (_dir, store, _logs, sup) = setup();
        store.insert(spec("flaky", "exit 3")).unwrap();
        sup.start("flaky").unwrap();

        // First run exits immediately; after ~1s backoff it restarts.
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let info = store.info("flaky").unwrap();
        assert_eq!(info.last_exit_code, Some(3));
        assert!(info.last_crash_at.is_some());
        assert!(info.restarts >= 1, "expected at least one restart");

        sup.stop("flaky").await.unwrap();
        assert_eq!(store.info("flaky").unwrap().status, ProcessStatus::Stopped);
    }

    #[tokio::test]
    async fn double_start_rejected() {
        let (_dir, store, _logs, sup) = setup();
        store.insert(spec("api", "sleep 30")).unwrap();
        sup.start("api").unwrap();
        assert!(sup.start("api").is_err());
        sup.stop("api").await.unwrap();
    }
}
