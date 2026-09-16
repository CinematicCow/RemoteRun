//! CLI side: connects to the daemon over the unix socket, auto-starting the
//! daemon if it isn't running, and renders responses for the terminal.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::time::Duration;

use crate::paths;
use crate::protocol::{LogStream, ProcessInfo, Request, Response};

pub fn run(req: &Request) -> Result<(), String> {
    let stream = ensure_daemon()?;

    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("socket error: {e}"))?;
    let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .map_err(|e| format!("failed to send request: {e}"))?;

    let reader = BufReader::new(stream);
    for resp_line in reader.lines() {
        let resp_line = resp_line.map_err(|e| format!("connection lost: {e}"))?;
        let resp: Response =
            serde_json::from_str(&resp_line).map_err(|e| format!("bad response: {e}"))?;
        match resp {
            Response::Ok => println!("ok"),
            Response::Error { message } => return Err(message),
            Response::Process { process } => print_process(&process),
            Response::Processes { processes, .. } => print_table(&processes),
            Response::LogHistory { lines } => lines.iter().for_each(print_log_line),
            Response::LogLine { line } => print_log_line(&line),
        }
    }
    Ok(())
}

/// Connect to the daemon, spawning it (detached) first if needed.
fn ensure_daemon() -> Result<UnixStream, String> {
    let sock = paths::socket_path();
    if let Ok(stream) = UnixStream::connect(&sock) {
        return Ok(stream);
    }

    std::fs::create_dir_all(paths::data_dir())
        .map_err(|e| format!("cannot create {}: {e}", paths::data_dir().display()))?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate rr binary: {e}"))?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::daemon_log_path())
        .map_err(|e| format!("cannot open daemon log: {e}"))?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;

    let mut child = std::process::Command::new(exe)
        .arg("__daemon")
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(log2)
        .process_group(0) // detach from our session so it survives terminal close
        .spawn()
        .map_err(|e| format!("failed to spawn daemon: {e}"))?;

    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if let Ok(stream) = UnixStream::connect(&sock) {
            return Ok(stream);
        }
        // Daemon exited instead of coming up: report its error right away.
        if child.try_wait().ok().flatten().is_some() {
            return Err(format!("daemon failed to start:{}", daemon_log_tail(3)));
        }
    }
    Err(format!("daemon did not come up:{}", daemon_log_tail(3)))
}

/// Last `n` lines of the daemon log, prefixed with newlines for display.
fn daemon_log_tail(n: usize) -> String {
    let path = paths::daemon_log_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return format!(" check {}", path.display());
    };
    let tail: Vec<&str> = content.lines().rev().take(n).collect();
    let mut out = String::new();
    for line in tail.iter().rev() {
        let _ = write!(out, "\n  {line}");
    }
    out
}

/// Render an optional value for table output, `-` when absent.
fn or_dash<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

fn print_process(p: &ProcessInfo) {
    println!("{}  {}  pid {}", p.name, p.status, or_dash(p.pid));
}

fn print_table(processes: &[ProcessInfo]) {
    if processes.is_empty() {
        println!("no processes (start one with `rr --name <name> \"<command>\"`)");
        return;
    }
    println!("NAME             STATUS       PID    UPTIME RESTARTS  LAST EXIT COMMAND");
    for p in processes {
        println!(
            "{:<16} {:<8} {:>7} {:>9} {:>8}  {:<9} {}",
            p.name,
            p.status.to_string(),
            or_dash(p.pid),
            p.uptime_secs
                .map_or_else(|| "-".to_owned(), format_duration),
            p.restarts,
            or_dash(p.last_exit_code),
            p.command,
        );
    }
}

fn print_log_line(l: &crate::protocol::LogLine) {
    let tag = match l.stream {
        LogStream::Stdout => "out",
        LogStream::Stderr => "err",
    };
    println!("{} [{tag}] {}", l.ts, l.line);
}

pub fn format_duration(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        (secs % 86_400) / 3_600,
        (secs % 3_600) / 60,
        secs % 60,
    );
    match (d, h, m) {
        (0, 0, 0) => format!("{s}s"),
        (0, 0, _) => format!("{m}m {s}s"),
        (0, _, _) => format!("{h}h {m}m"),
        _ => format!("{d}d {h}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn durations_humanize() {
        assert_eq!(format_duration(5), "5s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3_700), "1h 1m");
        assert_eq!(format_duration(90_000), "1d 1h");
    }
}
