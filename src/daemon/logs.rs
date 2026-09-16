//! Log capture: timestamps child output, persists it to disk and fans it out
//! to live subscribers (CLI `logs -f` and the dashboard's SSE stream).
//!
//! On-disk format is one line per entry: `<timestamp> <text>`, with stdout and
//! stderr in separate files (`<name>.out.log` / `<name>.err.log`).

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;

use crate::protocol::{LogLine, LogStream};
use crate::util::now_rfc3339;

const BROADCAST_CAPACITY: usize = 1024;
const STREAMS: [LogStream; 2] = [LogStream::Stdout, LogStream::Stderr];

pub struct LogManager {
    dir: PathBuf,
    tx: broadcast::Sender<LogLine>,
}

impl LogManager {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Ok(Self { dir, tx })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    pub fn file_path(&self, name: &str, stream: LogStream) -> PathBuf {
        let ext = match stream {
            LogStream::Stdout => "out.log",
            LogStream::Stderr => "err.log",
        };
        self.dir.join(format!("{name}.{ext}"))
    }

    /// Spawn a task that consumes `reader` line by line until EOF (child
    /// exit), appending each timestamped line to disk and broadcasting it.
    pub fn pipe<R>(&self, name: String, stream: LogStream, reader: R)
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let path = self.file_path(&name, stream);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut file = match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("rr: cannot open log file {}: {e}", path.display());
                    return;
                }
            };
            let mut lines = BufReader::new(reader).lines();
            loop {
                let line = match lines.next_line().await {
                    Ok(Some(line)) => line,
                    Ok(None) => break, // EOF: child exited
                    Err(e) => {
                        eprintln!("rr: log read failed for {name}: {e}");
                        break;
                    }
                };
                let entry = LogLine {
                    name: name.clone(),
                    stream,
                    ts: now_rfc3339(),
                    line,
                };
                let disk = format_line(&entry);
                if let Err(e) = async {
                    file.write_all(disk.as_bytes()).await?;
                    file.flush().await
                }
                .await
                {
                    eprintln!("rr: log write failed for {name}: {e}");
                }
                let _ = tx.send(entry);
            }
        });
    }

    /// Last `n` lines for `name`, stdout and stderr merged by timestamp.
    pub fn history(&self, name: &str, n: usize) -> Vec<LogLine> {
        let mut lines = Vec::new();
        for stream in STREAMS {
            let path = self.file_path(name, stream);
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut tail: Vec<LogLine> = content
                .lines()
                .rev()
                .take(n)
                .filter_map(|l| parse_line(name, stream, l))
                .collect();
            tail.reverse();
            lines.extend(tail);
        }
        lines.sort_by(|a, b| a.ts.cmp(&b.ts));
        if lines.len() > n {
            lines.drain(..lines.len() - n);
        }
        lines
    }

    /// Delete both log files (used by `rr rm`).
    pub fn remove(&self, name: &str) {
        for stream in STREAMS {
            let _ = std::fs::remove_file(self.file_path(name, stream));
        }
    }
}

fn format_line(entry: &LogLine) -> String {
    format!("{} {}\n", entry.ts, entry.line)
}

fn parse_line(name: &str, stream: LogStream, raw: &str) -> Option<LogLine> {
    let (ts, line) = raw.split_once(' ')?;
    Some(LogLine {
        name: name.to_string(),
        stream,
        ts: ts.to_string(),
        line: line.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ts: &str, line: &str) -> LogLine {
        LogLine {
            name: "api".into(),
            stream: LogStream::Stdout,
            ts: ts.into(),
            line: line.into(),
        }
    }

    #[test]
    fn format_parse_roundtrip() {
        let e = entry("2026-08-01T10:15:30.123Z", "hello world");
        let disk = format_line(&e);
        let parsed = parse_line("api", LogStream::Stdout, disk.trim_end()).unwrap();
        assert_eq!(parsed.ts, e.ts);
        assert_eq!(parsed.line, e.line);
    }

    #[test]
    fn history_merges_streams_by_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = LogManager::new(dir.path().to_path_buf()).unwrap();
        std::fs::write(
            mgr.file_path("api", LogStream::Stdout),
            "2026-08-01T10:00:01.000Z out one\n2026-08-01T10:00:03.000Z out two\n",
        )
        .unwrap();
        std::fs::write(
            mgr.file_path("api", LogStream::Stderr),
            "2026-08-01T10:00:02.000Z err one\n",
        )
        .unwrap();

        let hist = mgr.history("api", 10);
        let lines: Vec<&str> = hist.iter().map(|l| l.line.as_str()).collect();
        assert_eq!(lines, vec!["out one", "err one", "out two"]);

        let last_two = mgr.history("api", 2);
        let lines: Vec<&str> = last_two.iter().map(|l| l.line.as_str()).collect();
        assert_eq!(lines, vec!["err one", "out two"]);
    }

    #[tokio::test]
    async fn pipe_writes_and_broadcasts() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = LogManager::new(dir.path().to_path_buf()).unwrap();
        let mut rx = mgr.subscribe();

        mgr.pipe(
            "api".into(),
            LogStream::Stdout,
            std::io::Cursor::new(b"first\nsecond\n".to_vec()),
        );

        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        assert_eq!(a.line, "first");
        assert_eq!(b.line, "second");

        // Piping has finished (both lines broadcast), so the file is complete.
        let hist = mgr.history("api", 10);
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].line, "first");
    }
}
