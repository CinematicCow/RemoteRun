//! Daemon transport: one blocking round-trip per request over the unix
//! socket, plus the long-lived per-process log stream.
//!
//! The server closes the connection after the terminating response, so there
//! is no session state to keep between requests.

use std::io::{BufRead, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use super::{LOG_HISTORY, LOG_READ_TIMEOUT, Message, Reply};
use crate::paths;
use crate::protocol::{Request, Response};

pub(super) fn spawn_worker(tx: Sender<Message>) -> Sender<Request> {
    let (req_tx, req_rx) = mpsc::channel::<Request>();
    let _ = std::thread::spawn(move || {
        for req in req_rx {
            let reply = match send_request(&req) {
                Ok(responses) => translate(&req, responses),
                Err(e) => Reply::Error(e),
            };
            if tx.send(Message::Reply(reply)).is_err() {
                break;
            }
        }
    });
    req_tx
}

fn send_request(req: &Request) -> Result<Vec<Response>, String> {
    let mut stream = UnixStream::connect(paths::socket_path())
        .map_err(|e| format!("not connected to daemon: {e}"))?;
    let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("send failed: {e}"))?;

    let mut out = Vec::new();
    for raw in std::io::BufReader::new(stream).lines() {
        let raw = raw.map_err(|e| format!("connection lost: {e}"))?;
        let resp: Response =
            serde_json::from_str(&raw).map_err(|e| format!("bad response: {e}"))?;
        let terminal = matches!(
            resp,
            Response::Ok | Response::Error { .. } | Response::Processes { .. }
        );
        out.push(resp);
        if terminal {
            break;
        }
    }
    Ok(out)
}

fn translate(req: &Request, responses: Vec<Response>) -> Reply {
    for resp in &responses {
        if let Response::Error { message } = resp {
            return Reply::Error(message.clone());
        }
    }
    let processes = responses.into_iter().find_map(|resp| match resp {
        Response::Processes { processes, .. } => Some(processes),
        _ => None,
    });
    match req {
        Request::Ps => Reply::Processes(processes.unwrap_or_default()),
        _ => Reply::Done(status_for(req)),
    }
}

fn status_for(req: &Request) -> String {
    match req {
        Request::Start { name, .. } => format!("started {name}"),
        Request::Stop { name } => format!("stopped {name}"),
        Request::Restart { name } => format!("restarted {name}"),
        Request::Remove { name } => format!("removed {name}"),
        Request::Ps => "listed".to_owned(),
        Request::Logs { name, .. } => format!("logs {name}"),
    }
}

fn open_log_stream(name: &str) -> Result<UnixStream, String> {
    let mut stream =
        UnixStream::connect(paths::socket_path()).map_err(|e| format!("connect failed: {e}"))?;
    let req = Request::Logs {
        name: name.to_owned(),
        lines: LOG_HISTORY,
        follow: true,
    };
    let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("request failed: {e}"))?;
    stream
        .set_read_timeout(Some(LOG_READ_TIMEOUT))
        .map_err(|e| format!("cannot set read timeout: {e}"))?;
    Ok(stream)
}

/// Stream logs for one process until `generation` moves on (another process
/// was selected). Reconnects on failure so the daemon can restart underneath.
pub(super) fn log_stream(
    name: &str,
    generation: &Arc<AtomicU64>,
    epoch: u64,
    tx: &Sender<Message>,
) {
    loop {
        if generation.load(Ordering::SeqCst) != epoch {
            return;
        }
        if let Ok(mut stream) = open_log_stream(name) {
            if tx.send(Message::LogReset(name.to_owned())).is_err() {
                return;
            }
            read_log_stream(&mut stream, generation, epoch, tx);
        }
        for _ in 0..10 {
            if generation.load(Ordering::SeqCst) != epoch {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Frame newline-delimited JSON out of the stream by hand so a read timeout
/// mid-line neither loses nor duplicates data. History arrives as one batched
/// `LogHistory`, then live lines as individual `LogLine`s.
fn read_log_stream(
    stream: &mut UnixStream,
    generation: &AtomicU64,
    epoch: u64,
    tx: &Sender<Message>,
) {
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        if generation.load(Ordering::SeqCst) != epoch {
            return;
        }
        match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                    let mut line: Vec<u8> = pending.drain(..=pos).collect();
                    line.pop();
                    match serde_json::from_slice::<Response>(&line) {
                        Ok(Response::LogLine { line }) => {
                            if tx.send(Message::Log(line)).is_err() {
                                return;
                            }
                        }
                        Ok(Response::LogHistory { lines }) => {
                            for line in lines {
                                if tx.send(Message::Log(line)).is_err() {
                                    return;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}
