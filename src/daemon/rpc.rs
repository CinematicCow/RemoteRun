//! Newline-delimited JSON RPC over the unix socket. One request per
//! connection; `logs --follow` keeps the connection open and streams.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast::error::RecvError;

use super::core::Core;
use crate::protocol::{Request, Response};

pub async fn serve(core: Arc<Core>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let core = core.clone();
                tokio::spawn(async move {
                    let _ = handle_conn(core, stream).await;
                });
            }
            Err(e) => {
                eprintln!("rr: accept failed: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

async fn write_response(
    w: &mut (impl AsyncWriteExt + Unpin),
    resp: &Response,
) -> std::io::Result<()> {
    let mut buf = serde_json::to_vec(resp)?;
    buf.push(b'\n');
    w.write_all(&buf).await
}

async fn handle_conn(core: Arc<Core>, stream: UnixStream) -> std::io::Result<()> {
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(r).lines();
    let Some(line) = lines.next_line().await? else {
        return Ok(());
    };

    let req: Request = match serde_json::from_str(&line) {
        Ok(req) => req,
        Err(e) => {
            let resp = Response::Error {
                message: format!("bad request: {e}"),
            };
            return write_response(&mut w, &resp).await;
        }
    };

    match req {
        Request::Logs {
            name,
            lines: n,
            follow,
        } => {
            if !core.store.contains(&name) {
                let resp = Response::Error {
                    message: format!("no such process: {name}"),
                };
                return write_response(&mut w, &resp).await;
            }
            // Subscribing after reading history can drop a line emitted in
            // between; acceptable for a tail. Duplicates would be worse.
            for line in core.logs.history(&name, n) {
                write_response(&mut w, &Response::LogLine { line }).await?;
            }
            write_response(&mut w, &Response::LogHistoryEnd).await?;
            if follow {
                let mut rx = core.logs.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(line) if line.name == name => {
                            // Write error == client hung up; we're done.
                            write_response(&mut w, &Response::LogLine { line }).await?;
                        }
                        Ok(_) | Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => break,
                    }
                }
            }
            Ok(())
        }
        other => {
            let resp = core.handle(other).await;
            write_response(&mut w, &resp).await
        }
    }
}
