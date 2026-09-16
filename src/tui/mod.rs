//! Terminal UI: an interactive process dashboard over the unix socket.
//!
//! A thin client like `rr ps`, but full-screen. Functionality runs on
//! background threads — input, request/response RPC, and one live log stream
//! per selected process — while the main loop blocks on a single message
//! channel, so the screen redraws the moment anything changes.

mod app;
mod socket;
mod view;
mod widgets;

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::style::Color;

use crate::protocol::{LogLine, ProcessInfo};
use app::App;

const PROC_POLL_INTERVAL: Duration = Duration::from_secs(1);
const LOOP_TICK: Duration = Duration::from_millis(50);
const LOG_HISTORY: usize = 500;
const MAX_LOG_LINES: usize = 5000;
const LOG_READ_TIMEOUT: Duration = Duration::from_millis(250);
const STATUS_TTL: Duration = Duration::from_secs(5);
const CONNECTED_TTL: Duration = Duration::from_secs(3);

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::Indexed(245);
const FAINT: Color = Color::Indexed(240);
const SELECT_BG: Color = Color::Indexed(236);

/// Run the TUI, ensuring the daemon is up first so startup failures print
/// cleanly instead of corrupting the alternate screen.
pub fn run() -> Result<(), String> {
    crate::client::ensure_daemon()?;
    let (msg_tx, msg_rx) = mpsc::channel::<Message>();
    let req_tx = socket::spawn_worker(msg_tx.clone());
    let mut app = App::new(msg_rx, req_tx, msg_tx);
    ratatui::run(|terminal| app.run(terminal).map_err(std::io::Error::other))
        .map_err(|e| format!("tui failed: {e}"))
}

/// Everything the background threads send to the single main loop.
enum Message {
    Key(KeyEvent),
    Resize,
    Reply(Reply),
    LogReset(String),
    Log(LogLine),
}

enum Reply {
    Processes(Vec<ProcessInfo>),
    Done(String),
    Error(String),
}

fn spawn_input(tx: Sender<Message>) {
    let _ = std::thread::spawn(move || {
        loop {
            let Ok(event) = event::read() else { return };
            let message = match event {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    Message::Key(key)
                }
                Event::Resize(..) => Message::Resize,
                _ => continue,
            };
            if tx.send(message).is_err() {
                return;
            }
        }
    });
}
