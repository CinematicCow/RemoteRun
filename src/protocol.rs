//! Request/response types shared by the CLI client, unix socket RPC and HTTP API.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Start {
        name: String,
        command: String,
        cwd: String,
    },
    Ps,
    Stop {
        name: String,
    },
    Restart {
        name: String,
    },
    Remove {
        name: String,
    },
    /// Returns `lines` of history, then streams live lines while `follow`.
    Logs {
        name: String,
        lines: usize,
        follow: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Pong,
    Error { message: String },
    Processes { processes: Vec<ProcessInfo> },
    Process { process: ProcessInfo },
    LogLine { line: LogLine },
    /// Marks the end of log history; live lines follow after this.
    LogHistoryEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Running,
    Stopped,
    Crashed,
    /// Crashed and waiting out the restart backoff delay.
    Backoff,
}

impl std::fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Crashed => "crashed",
            Self::Backoff => "backoff",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub restarts: u64,
    pub last_exit_code: Option<i32>,
    /// Unix timestamp of the most recent crash.
    pub last_crash_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub name: String,
    pub stream: LogStream,
    /// RFC 3339 timestamp.
    pub ts: String,
    pub line: String,
}
