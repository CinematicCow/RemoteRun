//! The daemon's core: owns the store, log manager and supervisor, and
//! dispatches protocol requests. Both the unix socket RPC and the HTTP API
//! call into this.

use std::sync::Arc;

use super::logs::LogManager;
use super::state::{ProcSpec, Store};
use super::supervisor::Supervisor;
use crate::protocol::{Request, Response};
use crate::util::now_ts;
use crate::{paths, protocol::ProcessStatus};

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
                Err(message) => Response::Error { message },
            },
            Request::Restart { name } => self.restart(name).await,
            Request::Remove { name } => {
                if self.supervisor.is_active(&name) {
                    return Response::Error {
                        message: format!("process '{name}' is running; stop it first"),
                    };
                }
                match self.store.remove(&name) {
                    Ok(()) => {
                        self.logs.remove(&name);
                        Response::Ok
                    }
                    Err(message) => Response::Error { message },
                }
            }
            Request::Logs { .. } => Response::Error {
                message: "logs must be requested over a streaming transport".into(),
            },
        }
    }

    async fn start(&self, name: String, command: String, cwd: String) -> Response {
        if !valid_name(&name) {
            return Response::Error {
                message: "invalid name: use letters, digits, '-', '_', '.' (max 64 chars)".into(),
            };
        }
        if command.trim().is_empty() {
            return Response::Error {
                message: "command is empty".into(),
            };
        }
        let cwd = if cwd.trim().is_empty() {
            dirs::home_dir().map_or_else(|| "/".into(), |h| h.to_string_lossy().into_owned())
        } else {
            cwd
        };
        if self.store.contains(&name) {
            return Response::Error {
                message: format!("process '{name}' already exists (try `rr restart {name}`)"),
            };
        }
        if let Err(message) = self.store.insert(ProcSpec {
            name: name.clone(),
            command,
            cwd,
            created_at: now_ts(),
        }) {
            return Response::Error { message };
        }
        if let Err(message) = self.supervisor.start(&name) {
            return Response::Error { message };
        }
        self.await_pid(&name).await;
        self.process_response(&name)
    }

    async fn restart(&self, name: String) -> Response {
        if !self.store.contains(&name) {
            return Response::Error {
                message: format!("no such process: {name}"),
            };
        }
        if self.supervisor.is_active(&name) {
            if let Err(message) = self.supervisor.stop(&name).await {
                return Response::Error { message };
            }
        }
        if let Err(message) = self.supervisor.start(&name) {
            return Response::Error { message };
        }
        self.await_pid(&name).await;
        self.process_response(&name)
    }

    /// The supervisor task spawns the child asynchronously; give it a moment
    /// so start/restart responses can include the fresh pid.
    async fn await_pid(&self, name: &str) {
        for _ in 0..20 {
            match self.store.info(name) {
                Some(info) if info.status == ProcessStatus::Running && info.pid.is_some() => break,
                _ => tokio::time::sleep(std::time::Duration::from_millis(25)).await,
            }
        }
    }

    fn process_response(&self, name: &str) -> Response {
        self.store.info(name).map_or_else(
            || Response::Error {
                message: format!("no such process: {name}"),
            },
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
