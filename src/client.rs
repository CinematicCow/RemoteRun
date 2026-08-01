//! CLI side: connects to the daemon over the unix socket, auto-starting the
//! daemon if it isn't running, and renders responses for the terminal.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::time::Duration;

use crate::paths;
use crate::protocol::{LogStream, ProcessInfo, Request, Response};

pub fn run(req: Request) -> Result<(), String> {
    let stream = ensure_daemon()?;
    let follow_logs = matches!(req, Request::Logs { follow: true, .. });

    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("socket error: {e}"))?;
    let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
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
            Response::Pong => println!("daemon is up"),
            Response::Error { message } => return Err(message),
            Response::Process { process } => print_process(&process),
            Response::Processes { processes } => print_table(&processes),
            Response::LogLine { line } => print_log_line(&line),
            Response::LogHistoryEnd => {
                if !follow_logs {
                    break;
                }
            }
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

    std::process::Command::new(exe)
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
    }
    Err(format!(
        "daemon did not come up; check {}",
        paths::daemon_log_path().display()
    ))
}

fn print_process(p: &ProcessInfo) {
    let pid = p.pid.map_or("-".into(), |pid| pid.to_string());
    println!("{}  {}  pid {}", p.name, p.status, pid);
}

fn print_table(processes: &[ProcessInfo]) {
    if processes.is_empty() {
        println!("no processes (start one with `rr --name <name> \"<command>\"`)");
        return;
    }
    println!(
        "{:<16} {:<8} {:>7} {:>9} {:>8}  {:<9} {}",
        "NAME", "STATUS", "PID", "UPTIME", "RESTARTS", "LAST EXIT", "COMMAND"
    );
    for p in processes {
        println!(
            "{:<16} {:<8} {:>7} {:>9} {:>8}  {:<9} {}",
            p.name,
            p.status.to_string(),
            p.pid.map_or("-".into(), |pid| pid.to_string()),
            p.uptime_secs.map_or("-".into(), format_duration),
            p.restarts,
            p.last_exit_code.map_or("-".into(), |c| c.to_string()),
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
