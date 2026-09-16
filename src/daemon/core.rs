//! The daemon's core: owns the store, log manager and supervisor, and
//! dispatches protocol requests. Both the unix socket RPC and the HTTP API
//! call into this.

use std::sync::Arc;
use std::time::Duration;

use super::logs::LogManager;
use super::state::{ProcEntry, Store};
use super::supervisor::Supervisor;
use crate::paths;
use crate::protocol::{LogLine, ProcessStatus, Request, Response};
use crate::util::now_ts;

pub struct Core {
    pub store: Arc<Store>,
    pub logs: Arc<LogManager>,
    pub supervisor: Supervisor,
    /// Whether the HTTP dashboard is allowed to start processes. Off unless
    /// `RR_DASHBOARD_START` is set; the CLI always may.
    pub dashboard_start: bool,
}

fn dashboard_start_enabled() -> bool {
    std::env::var("RR_DASHBOARD_START")
        .ok()
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"))
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn error(message: impl Into<String>) -> Response {
    Response::Error {
        message: message.into(),
    }
}

impl Core {
    pub fn new() -> std::io::Result<Arc<Self>> {
        let store = Arc::new(Store::load(paths::state_path())?);
        let logs = Arc::new(LogManager::new(paths::logs_dir())?);
        let supervisor = Supervisor::new(store.clone(), logs.clone());
        Ok(Arc::new(Self {
            store,
            logs,
            supervisor,
            dashboard_start: dashboard_start_enabled(),
        }))
    }

    /// Handle every request except `Logs`, which is transport-specific
    /// because it streams.
    pub async fn handle(&self, req: Request) -> Response {
        match req {
            Request::Start { name, command, cwd } => self.start(name, command, cwd).await,
            Request::Ps => Response::Processes {
                processes: self.store.list(),
                start_enabled: self.dashboard_start,
            },
            Request::Stop { name } => match self.supervisor.stop(&name).await {
                Ok(()) => self.process_response(&name),
                Err(message) => error(message),
            },
            Request::Restart { name } => self.restart(name).await,
            Request::Remove { name } => self.remove(&name),
            Request::Logs { .. } => error("logs must be requested over a streaming transport"),
        }
    }

    fn remove(&self, name: &str) -> Response {
        if self.supervisor.is_active(name) {
            return error(format!("process '{name}' is running; stop it first"));
        }
        match self.store.remove(name) {
            Ok(()) => {
                self.logs.remove(name);
                Response::Ok
            }
            Err(message) => error(message),
        }
    }

    async fn start(&self, name: String, command: String, cwd: String) -> Response {
        if !valid_name(&name) {
            return error("invalid name: use letters, digits, '-', '_', '.' (max 64 chars)");
        }
        if command.trim().is_empty() {
            return error("command is empty");
        }
        if self.store.contains(&name) {
            return error(format!(
                "process '{name}' already exists (try `rr restart {name}`)"
            ));
        }
        let cwd = if cwd.trim().is_empty() {
            dirs::home_dir().map_or_else(|| "/".into(), |h| h.to_string_lossy().into_owned())
        } else {
            cwd
        };
        if let Err(message) =
            self.store
                .insert(ProcEntry::new(name.clone(), command, cwd, now_ts()))
        {
            return error(message);
        }
        if let Err(message) = self.supervisor.start(&name) {
            return error(message);
        }
        self.settle(&name).await
    }

    async fn restart(&self, name: String) -> Response {
        if !self.store.contains(&name) {
            return error(format!("no such process: {name}"));
        }
        if self.supervisor.is_active(&name) {
            if let Err(message) = self.supervisor.stop(&name).await {
                return error(message);
            }
        }
        if let Err(message) = self.supervisor.start(&name) {
            return error(message);
        }
        self.settle(&name).await
    }

    /// Wait for the supervisor's async spawn, then report the process so
    /// start/restart responses carry the fresh pid.
    async fn settle(&self, name: &str) -> Response {
        for _ in 0..20 {
            match self.store.info(name) {
                Some(info) if info.status == ProcessStatus::Running && info.pid.is_some() => break,
                _ => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }
        self.process_response(name)
    }

    /// Log history for an existing process, or why it can't be read. Shared by
    /// the unix socket and HTTP transports, which stream the result differently.
    pub fn log_history(&self, name: &str, lines: usize) -> Result<Vec<LogLine>, String> {
        if !self.store.contains(name) {
            return Err(format!("no such process: {name}"));
        }
        Ok(self.logs.history(name, lines))
    }

    fn process_response(&self, name: &str) -> Response {
        self.store.info(name).map_or_else(
            || error(format!("no such process: {name}")),
            |process| Response::Process { process },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation() {
        assert!(valid_name("api"));
        assert!(valid_name("my-app_2.0"));
        assert!(!valid_name(""));
        assert!(!valid_name("has space"));
        assert!(!valid_name("slash/y"));
        assert!(!valid_name(&"x".repeat(65)));
    }
}
