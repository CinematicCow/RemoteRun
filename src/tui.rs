//! Terminal UI: an interactive process dashboard over the unix socket.
//!
//! A thin client like `rr ps`, but full-screen. Functionality runs on
//! background threads — input, request/response RPC, and one live log stream
//! per selected process — while the main loop blocks on a single message
//! channel, so the screen redraws the moment anything changes.

use std::io::{BufRead, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, HighlightSpacing, Padding, Paragraph, Row, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Table, TableState,
};
use ratatui::{DefaultTerminal, Frame};

use crate::client::format_duration;
use crate::paths;
use crate::protocol::{LogLine, LogStream, ProcessInfo, ProcessStatus, Request, Response};

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
    let req_tx = spawn_worker(msg_tx.clone());
    let mut app = App::new(msg_rx, req_tx, msg_tx);
    ratatui::run(|terminal| app.run(terminal).map_err(std::io::Error::other))
        .map_err(|e| format!("tui failed: {e}"))
}

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

fn spawn_worker(tx: Sender<Message>) -> Sender<Request> {
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

/// One blocking round-trip on a fresh connection. The server closes the
/// connection after the terminating response, so there is no state to keep.
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
fn log_stream(name: &str, generation: &Arc<AtomicU64>, epoch: u64, tx: &Sender<Message>) {
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
/// mid-line neither loses nor duplicates data.
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
                    if let Ok(Response::LogLine { line }) =
                        serde_json::from_slice::<Response>(&line)
                    {
                        if tx.send(Message::Log(line)).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}

struct App {
    msg_rx: Receiver<Message>,
    req_tx: Sender<Request>,
    msg_tx: Sender<Message>,
    processes: Vec<ProcessInfo>,
    table: TableState,
    logs_name: Option<String>,
    logs: Vec<LogLine>,
    log_follow: bool,
    log_scroll: u16,
    log_generation: Arc<AtomicU64>,
    modal: Modal,
    status: String,
    status_at: Instant,
    status_error: bool,
    last_ok: Option<Instant>,
    should_quit: bool,
    next_proc_poll: Instant,
}

enum Modal {
    None,
    Start(StartForm),
    Confirm { name: String },
    Help,
}

#[derive(Default)]
struct StartForm {
    name: String,
    command: String,
    cwd: String,
    field: usize,
    error: Option<String>,
}

impl StartForm {
    const fn active(&self) -> &String {
        match self.field {
            0 => &self.name,
            1 => &self.command,
            _ => &self.cwd,
        }
    }

    const fn active_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.name,
            1 => &mut self.command,
            _ => &mut self.cwd,
        }
    }
}

fn default_form() -> StartForm {
    StartForm {
        cwd: std::env::current_dir()
            .map_or_else(|_| String::new(), |d| d.to_string_lossy().into_owned()),
        ..StartForm::default()
    }
}

enum ModalAction {
    Close,
    SubmitStart,
    Remove(String),
}

impl App {
    fn new(msg_rx: Receiver<Message>, req_tx: Sender<Request>, msg_tx: Sender<Message>) -> Self {
        Self {
            msg_rx,
            req_tx,
            msg_tx,
            processes: Vec::new(),
            table: TableState::default(),
            logs_name: None,
            logs: Vec::new(),
            log_follow: true,
            log_scroll: 0,
            log_generation: Arc::new(AtomicU64::new(0)),
            modal: Modal::None,
            status: String::new(),
            status_at: Instant::now(),
            status_error: false,
            last_ok: None,
            should_quit: false,
            next_proc_poll: Instant::now(),
        }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), String> {
        spawn_input(self.msg_tx.clone());
        let mut dirty = true;
        loop {
            // Coalesce bursts: drain everything queued before drawing, so a
            // flood of log lines becomes one render per pass instead of many.
            match self.msg_rx.recv_timeout(LOOP_TICK) {
                Ok(message) => {
                    self.handle_message(message);
                    while let Ok(message) = self.msg_rx.try_recv() {
                        self.handle_message(message);
                    }
                    dirty = true;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if self.expire_status() {
                dirty = true;
            }
            if self.sync_logs_target() {
                dirty = true;
            }
            self.poll_due();
            if dirty {
                terminal
                    .draw(|frame| self.render(frame))
                    .map_err(|e| format!("draw failed: {e}"))?;
                dirty = false;
            }
            if self.should_quit {
                return Ok(());
            }
        }
        Ok(())
    }

    fn handle_message(&mut self, message: Message) {
        match message {
            Message::Key(key) => self.on_key(key),
            Message::Resize => {}
            Message::Reply(reply) => self.on_reply(reply),
            Message::LogReset(name) => self.on_log_reset(&name),
            Message::Log(line) => self.on_log(line),
        }
    }

    fn on_reply(&mut self, reply: Reply) {
        match reply {
            Reply::Processes(processes) => self.set_processes(processes),
            Reply::Done(message) => {
                self.set_status(message, false);
                self.next_proc_poll = Instant::now();
            }
            Reply::Error(message) => self.set_status(message, true),
        }
    }

    fn on_log_reset(&mut self, name: &str) {
        if self.logs_name.as_deref() == Some(name) {
            self.logs.clear();
            self.log_scroll = 0;
        }
    }

    fn on_log(&mut self, line: LogLine) {
        if self.logs_name.as_deref() == Some(line.name.as_str()) {
            self.logs.push(line);
            let overflow = self.logs.len().saturating_sub(MAX_LOG_LINES);
            if overflow > 0 {
                self.logs.drain(..overflow);
            }
        }
    }

    fn poll_due(&mut self) {
        if Instant::now() >= self.next_proc_poll {
            self.next_proc_poll = Instant::now() + PROC_POLL_INTERVAL;
            let _ = self.req_tx.send(Request::Ps);
        }
    }

    fn set_status(&mut self, message: String, is_error: bool) {
        self.status = message;
        self.status_at = Instant::now();
        self.status_error = is_error;
    }

    fn expire_status(&mut self) -> bool {
        if !self.status.is_empty() && self.status_at.elapsed() > STATUS_TTL {
            self.status.clear();
            return true;
        }
        false
    }

    fn set_processes(&mut self, processes: Vec<ProcessInfo>) {
        let current = self.selected_name().map(str::to_owned);
        self.processes = processes;
        let selected = current
            .and_then(|name| self.processes.iter().position(|p| p.name == name))
            .or_else(|| (!self.processes.is_empty()).then_some(0));
        self.table.select(selected);
        self.last_ok = Some(Instant::now());
    }

    /// Switch the live log stream when the selected process changes.
    fn sync_logs_target(&mut self) -> bool {
        let name = self.selected_name().map(str::to_owned);
        if name == self.logs_name {
            return false;
        }
        self.logs_name.clone_from(&name);
        self.logs.clear();
        self.log_scroll = 0;
        self.log_follow = true;
        let epoch = self.log_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(name) = name {
            let generation = self.log_generation.clone();
            let tx = self.msg_tx.clone();
            let _ = std::thread::spawn(move || log_stream(&name, &generation, epoch, &tx));
        }
        true
    }

    fn selected(&self) -> Option<&ProcessInfo> {
        self.table.selected().and_then(|i| self.processes.get(i))
    }

    fn selected_name(&self) -> Option<&str> {
        self.selected().map(|p| p.name.as_str())
    }

    fn connected(&self) -> bool {
        self.last_ok.is_some_and(|t| t.elapsed() < CONNECTED_TTL)
    }

    fn select_next(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let next = match self.table.selected() {
            Some(i) if i + 1 < self.processes.len() => i + 1,
            _ => 0,
        };
        self.table.select(Some(next));
    }

    fn select_prev(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let prev = match self.table.selected() {
            Some(0) | None => self.processes.len() - 1,
            Some(i) => i - 1,
        };
        self.table.select(Some(prev));
    }

    fn send(&mut self, req: Request, status: &str) {
        if self.req_tx.send(req).is_err() {
            self.set_status("daemon connection lost".to_owned(), true);
            return;
        }
        self.set_status(status.to_owned(), false);
        self.next_proc_poll = Instant::now();
    }

    fn on_key(&mut self, key: KeyEvent) {
        if !matches!(self.modal, Modal::None) {
            self.on_modal_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Char('s') => self.modal = Modal::Start(default_form()),
            KeyCode::Char('d') => self.confirm_remove(),
            KeyCode::Char('?') => self.modal = Modal::Help,
            KeyCode::Char('r') => {
                if let Some(name) = self.selected_name().map(str::to_owned) {
                    self.send(Request::Restart { name }, "restarting…");
                }
            }
            KeyCode::Char('x') => {
                if let Some(name) = self.selected_name().map(str::to_owned) {
                    self.send(Request::Stop { name }, "stopping…");
                }
            }
            KeyCode::Char('f') => self.log_follow = !self.log_follow,
            KeyCode::Char('G') | KeyCode::End => self.log_follow = true,
            KeyCode::Char('g') | KeyCode::Home => {
                self.log_follow = false;
                self.log_scroll = 0;
            }
            KeyCode::PageUp => {
                self.log_follow = false;
                self.log_scroll = self.log_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.log_scroll = self.log_scroll.saturating_add(10);
            }
            _ => {}
        }
    }

    fn on_modal_key(&mut self, key: KeyEvent) {
        let action = match &mut self.modal {
            Modal::None => return,
            Modal::Help => match key.code {
                KeyCode::Esc | KeyCode::Char('?' | 'q') => Some(ModalAction::Close),
                _ => None,
            },
            Modal::Start(form) => match key.code {
                KeyCode::Esc => Some(ModalAction::Close),
                KeyCode::Enter => Some(ModalAction::SubmitStart),
                KeyCode::Tab | KeyCode::Down => {
                    form.field = (form.field + 1) % 3;
                    None
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.field = (form.field + 2) % 3;
                    None
                }
                KeyCode::Backspace => {
                    let _ = form.active_mut().pop();
                    None
                }
                KeyCode::Char(c) => {
                    form.active_mut().push(c);
                    None
                }
                _ => None,
            },
            Modal::Confirm { name } => match key.code {
                KeyCode::Char('y' | 'Y') => Some(ModalAction::Remove(name.clone())),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(ModalAction::Close),
                _ => None,
            },
        };
        match action {
            Some(ModalAction::Close) => self.modal = Modal::None,
            Some(ModalAction::SubmitStart) => self.submit_start(),
            Some(ModalAction::Remove(name)) => {
                self.modal = Modal::None;
                self.send(Request::Remove { name }, "removing…");
            }
            None => {}
        }
    }

    fn confirm_remove(&mut self) {
        if let Some(name) = self.selected_name().map(str::to_owned) {
            self.modal = Modal::Confirm { name };
        } else {
            self.set_status("no process selected".to_owned(), false);
        }
    }

    fn submit_start(&mut self) {
        let (name, command, cwd) = {
            let Modal::Start(form) = &mut self.modal else {
                return;
            };
            if form.name.trim().is_empty() {
                form.error = Some("name is required".to_owned());
                return;
            }
            if form.command.trim().is_empty() {
                form.error = Some("command is required".to_owned());
                return;
            }
            (
                form.name.trim().to_owned(),
                form.command.trim().to_owned(),
                form.cwd.trim().to_owned(),
            )
        };
        self.modal = Modal::None;
        self.send(Request::Start { name, command, cwd }, "starting…");
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);

        self.render_header(frame, header);
        let [left, right] = Layout::horizontal([Constraint::Length(48), Constraint::Min(20)])
            .spacing(1)
            .areas(body);
        if self.processes.is_empty() {
            Self::render_empty_processes(frame, left);
        } else {
            self.render_processes(frame, left);
        }
        let [details, logs] = Layout::vertical([Constraint::Length(9), Constraint::Min(3)])
            .spacing(1)
            .areas(right);
        self.render_details(frame, details);
        self.render_logs(frame, logs);
        Self::render_footer(frame, footer);
        self.render_modal(frame, area);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let conn = if self.connected() {
            "connected"
        } else {
            "disconnected"
        };
        let status_len = if self.status.is_empty() {
            0
        } else {
            u16::try_from(self.status.chars().count() + 2).unwrap_or(u16::MAX)
        };
        let right_width =
            (status_len + 2 + u16::try_from(conn.len()).unwrap_or(u16::MAX)).min(area.width);
        let [left, right] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);

        let running = self
            .processes
            .iter()
            .filter(|p| p.status == ProcessStatus::Running)
            .count();
        let line = Line::from(vec![
            Span::styled(
                " rr ",
                Style::new()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", plural(self.processes.len(), "process")),
                Style::new().fg(MUTED),
            ),
            Span::styled(
                format!(" · {running} running"),
                Style::new().fg(Color::Green),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), left);

        let mut spans = Vec::new();
        if !self.status.is_empty() {
            let color = if self.status_error { Color::Red } else { MUTED };
            spans.push(Span::styled(self.status.clone(), Style::new().fg(color)));
            spans.push(Span::raw("  "));
        }
        let conn_color = if self.connected() {
            Color::Green
        } else {
            Color::Red
        };
        spans.push(Span::styled("● ", Style::new().fg(conn_color)));
        spans.push(Span::styled(conn, Style::new().fg(MUTED)));
        frame.render_widget(
            Paragraph::new(Line::from(spans).alignment(Alignment::Right)),
            right,
        );
    }

    fn render_footer(frame: &mut Frame, area: Rect) {
        let keys = [
            ("q", "quit"),
            ("j/k", "move"),
            ("s", "start"),
            ("x", "stop"),
            ("r", "restart"),
            ("d", "remove"),
            ("f", "follow"),
            ("?", "help"),
        ];
        let mut spans = vec![Span::raw(" ")];
        for (key, desc) in keys {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!(" {desc}  "), Style::new().fg(FAINT)));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_empty_processes(frame: &mut Frame, area: Rect) {
        let block = panel(" processes ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let message = vec![
            Line::from(Span::styled(
                "No managed processes",
                Style::new().fg(MUTED).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            Line::raw(""),
            Line::from(Span::styled("press s to start one", Style::new().fg(FAINT)))
                .alignment(Alignment::Center),
        ];
        let [_, mid, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas(inner);
        frame.render_widget(Paragraph::new(message), mid);
    }

    fn render_processes(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!(" processes ({}) ", self.processes.len());
        let header = Row::new(["NAME", "STATUS", "PID", "UPTIME", "RESTARTS"])
            .style(Style::new().fg(FAINT).add_modifier(Modifier::BOLD));
        let rows = self.processes.iter().map(|p| {
            Row::new(vec![
                Cell::from(p.name.clone()).style(Style::new().add_modifier(Modifier::BOLD)),
                Cell::from(status_line(p.status)),
                Cell::from(right(or_dash(p.pid))),
                Cell::from(right(uptime_text(p.uptime_secs))),
                Cell::from(right(p.restarts.to_string())),
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Min(8),
                Constraint::Length(9),
                Constraint::Length(6),
                Constraint::Length(8),
                Constraint::Length(8),
            ],
        )
        .header(header)
        .block(panel(title))
        .row_highlight_style(Style::new().bg(SELECT_BG).add_modifier(Modifier::BOLD))
        .highlight_symbol(Line::from(Span::styled("▌ ", Style::new().fg(ACCENT))))
        .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let block = panel_padded(" details ");
        let Some(p) = self.selected() else {
            frame.render_widget(block, area);
            return;
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(format!("{:<9}", "status"), Style::new().fg(FAINT)),
                Span::styled(
                    p.status.to_string(),
                    Style::new()
                        .fg(status_color(p.status))
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            field("pid", or_dash(p.pid)),
            field("uptime", uptime_text(p.uptime_secs)),
            field("restarts", p.restarts.to_string()),
            field("last exit", or_dash(p.last_exit_code)),
            field("cwd", p.cwd.clone()),
            field("command", p.command.clone()),
        ];
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let title = self
            .logs_name
            .as_ref()
            .map_or_else(|| " logs ".to_owned(), |name| format!(" logs: {name} "));
        let (dot, state, color) = if self.log_follow {
            ("●", "following", Color::Green)
        } else {
            ("‖", "paused", Color::Yellow)
        };
        let bottom = Line::from(vec![
            Span::styled(format!("{dot} {state}"), Style::new().fg(color)),
            Span::styled(
                format!("  {} ", plural(self.logs.len(), "line")),
                Style::new().fg(FAINT),
            ),
        ])
        .alignment(Alignment::Right);
        let block = panel_padded(title).title_bottom(bottom);

        let view = usize::from(area.height.saturating_sub(2));
        let max_start = self.logs.len().saturating_sub(view);
        if self.log_follow {
            self.log_scroll = clamp_u16(max_start);
        } else {
            self.log_scroll = clamp_u16(usize::from(self.log_scroll).min(max_start));
        }

        if self.logs.is_empty() {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let [_, mid, _] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(inner);
            frame.render_widget(
                Paragraph::new(
                    Line::from(Span::styled("waiting for output…", Style::new().fg(FAINT)))
                        .alignment(Alignment::Center),
                ),
                mid,
            );
            return;
        }

        let lines: Vec<Line> = self.logs.iter().map(log_line_widget).collect();
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .scroll((self.log_scroll, 0)),
            area,
        );

        if self.logs.len() > view {
            let mut scrollbar =
                ScrollbarState::new(self.logs.len()).position(usize::from(self.log_scroll));
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .style(Style::new().fg(FAINT)),
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 1,
                }),
                &mut scrollbar,
            );
        }
    }

    fn render_modal(&self, frame: &mut Frame, area: Rect) {
        match &self.modal {
            Modal::None => {}
            Modal::Help => Self::render_help(frame, area),
            Modal::Start(form) => Self::render_start(frame, area, form),
            Modal::Confirm { name } => Self::render_confirm(frame, area, name),
        }
    }

    fn render_help(frame: &mut Frame, area: Rect) {
        let rows = [
            ("j / ↓", "move down"),
            ("k / ↑", "move up"),
            ("s", "start a process"),
            ("x", "stop selected"),
            ("r", "restart selected"),
            ("d", "remove selected"),
            ("f", "toggle log follow"),
            ("PgUp/PgDn", "scroll logs"),
            ("g / G", "logs top / follow"),
            ("? / Esc", "close help"),
            ("q", "quit"),
        ];
        let height = u16::try_from(rows.len()).unwrap_or(u16::MAX) + 2;
        let rect = centered_rect(area, 46, height);
        frame.render_widget(Clear, rect);
        let lines = rows
            .iter()
            .map(|(key, desc)| {
                Line::from(vec![
                    Span::styled(
                        format!(" {key:<10}"),
                        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled((*desc).to_owned(), Style::new().fg(MUTED)),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).block(panel_padded(" keys ")), rect);
    }

    fn render_start(frame: &mut Frame, area: Rect, form: &StartForm) {
        let rect = centered_rect(area, 64, 11);
        frame.render_widget(Clear, rect);
        let fields = [
            ("name", &form.name),
            ("command", &form.command),
            ("cwd", &form.cwd),
        ];
        let mut lines = Vec::new();
        for (i, (label, value)) in fields.iter().enumerate() {
            let active = i == form.field;
            let label_style = if active {
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(FAINT)
            };
            let value_style = if active {
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(MUTED)
            };
            let marker = if active { "▸" } else { " " };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} {label:<8}  "), label_style),
                Span::styled((*value).clone(), value_style),
            ]));
            lines.push(Line::raw(""));
        }
        match &form.error {
            Some(err) => lines.push(Line::from(Span::styled(
                format!(" {err}"),
                Style::new().fg(Color::Red),
            ))),
            None => lines.push(Line::raw("")),
        }
        lines.push(Line::from(Span::styled(
            " Tab next · Enter start · Esc cancel",
            Style::new().fg(FAINT),
        )));
        frame.render_widget(
            Paragraph::new(lines).block(panel_padded(" start process ")),
            rect,
        );

        let prefix = 12;
        let cursor_x = rect.x + 2 + prefix + clamp_u16(form.active().chars().count());
        let cursor_y = rect.y + 1 + clamp_u16(form.field * 2);
        let max_x = rect.x + rect.width.saturating_sub(2);
        frame.set_cursor_position((cursor_x.min(max_x), cursor_y));
    }

    fn render_confirm(frame: &mut Frame, area: Rect, name: &str) {
        let rect = centered_rect(area, 54, 6);
        frame.render_widget(Clear, rect);
        let lines = vec![
            Line::from(vec![
                Span::styled(" Remove ", Style::new().fg(MUTED)),
                Span::styled(
                    name.to_owned(),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::raw("?"),
            ]),
            Line::raw(""),
            Line::from(Span::styled(
                " y confirm     n cancel ",
                Style::new().fg(MUTED),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).block(panel_padded(" confirm ")), rect);
    }
}

fn panel(title: impl Into<String>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(FAINT))
        .title(Line::raw(title.into()).style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)))
}

fn panel_padded(title: impl Into<String>) -> Block<'static> {
    panel(title).padding(Padding::horizontal(1))
}

const fn status_color(status: ProcessStatus) -> Color {
    match status {
        ProcessStatus::Running => Color::Green,
        ProcessStatus::Stopped => Color::DarkGray,
        ProcessStatus::Crashed => Color::Red,
        ProcessStatus::Backoff => Color::Yellow,
    }
}

fn status_line(status: ProcessStatus) -> Line<'static> {
    let (dot, label) = match status {
        ProcessStatus::Running => ("●", "running"),
        ProcessStatus::Stopped => ("○", "stopped"),
        ProcessStatus::Crashed => ("✖", "crashed"),
        ProcessStatus::Backoff => ("◐", "backoff"),
    };
    let color = status_color(status);
    Line::from(vec![
        Span::styled(format!("{dot} "), Style::new().fg(color)),
        Span::styled(label, Style::new().fg(color)),
    ])
}

fn right(value: String) -> Line<'static> {
    Line::raw(value).alignment(Alignment::Right)
}

fn or_dash<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

fn uptime_text(secs: Option<u64>) -> String {
    secs.map_or_else(|| "-".to_owned(), format_duration)
}

fn field(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9} "), Style::new().fg(FAINT)),
        Span::styled(value, Style::new().fg(MUTED)),
    ])
}

fn short_time(ts: &str) -> &str {
    ts.get(11..19).unwrap_or(ts)
}

fn log_line_widget(l: &LogLine) -> Line<'static> {
    let (tag, color) = match l.stream {
        LogStream::Stdout => ("out", Color::Indexed(109)),
        LogStream::Stderr => ("err", Color::Red),
    };
    Line::from(vec![
        Span::styled(format!("{} ", short_time(&l.ts)), Style::new().fg(FAINT)),
        Span::styled(format!("{tag:<3} "), Style::new().fg(color)),
        Span::raw(l.line.clone()),
    ])
}

fn clamp_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("{count} {word}")
    } else {
        format!("{count} {word}s")
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn short_time_extracts_clock() {
        assert_eq!(short_time("2026-08-01T10:15:30.123Z"), "10:15:30");
        assert_eq!(short_time("nope"), "nope");
    }

    #[test]
    fn centered_rect_is_clamped_to_area() {
        let area = Rect::new(0, 0, 20, 10);
        assert_eq!(centered_rect(area, 50, 50), Rect::new(0, 0, 20, 10));
        assert_eq!(centered_rect(area, 10, 4), Rect::new(5, 3, 10, 4));
    }

    fn sample() -> ProcessInfo {
        ProcessInfo {
            name: "api".into(),
            command: "bun start".into(),
            cwd: "/srv/api".into(),
            status: ProcessStatus::Running,
            pid: Some(4242),
            uptime_secs: Some(3_725),
            restarts: 2,
            last_exit_code: None,
            last_crash_at: None,
            created_at: 0,
        }
    }

    fn new_app() -> App {
        let (msg_tx, msg_rx) = mpsc::channel();
        let (req_tx, _req_rx) = mpsc::channel();
        App::new(msg_rx, req_tx, msg_tx)
    }

    fn screen(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_dashboard_layout() {
        let mut app = new_app();
        app.set_processes(vec![sample()]);
        app.logs_name = Some("api".into());
        app.on_log(LogLine {
            name: "api".into(),
            stream: LogStream::Stderr,
            ts: "2026-08-01T10:15:30.123Z".into(),
            line: "listening on :3000".into(),
        });

        let out = screen(&mut app);
        println!("{out}");

        for needle in [
            "rr",
            "1 process · 1 running",
            "processes (1)",
            "api",
            "● running",
            "RESTARTS",
            "details",
            "logs: api",
            "following",
            "listening on :3000",
            "help",
        ] {
            assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
        }
    }

    #[test]
    fn renders_modals() {
        let mut app = new_app();
        app.set_processes(vec![sample()]);

        app.modal = Modal::Help;
        let out = screen(&mut app);
        for needle in ["keys", "move down", "toggle log follow", "quit"] {
            assert!(out.contains(needle), "help missing {needle:?} in:\n{out}");
        }

        app.modal = Modal::Start(default_form());
        let out = screen(&mut app);
        for needle in ["start process", "name", "command", "cwd", "Enter start"] {
            assert!(out.contains(needle), "start missing {needle:?} in:\n{out}");
        }

        app.modal = Modal::Confirm { name: "api".into() };
        let out = screen(&mut app);
        for needle in ["confirm", "Remove", "api", "y confirm"] {
            assert!(
                out.contains(needle),
                "confirm missing {needle:?} in:\n{out}"
            );
        }
    }
}
