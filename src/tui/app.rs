//! The TUI state machine: process list, log buffer, modal, and the key
//! handling that drives them. Rendering lives in [`super::view`].

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use super::{
    CONNECTED_TTL, LOOP_TICK, MAX_LOG_LINES, Message, PROC_POLL_INTERVAL, Reply, STATUS_TTL,
    spawn_input,
};
use crate::protocol::{LogLine, ProcessInfo, Request};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Pane {
    Processes,
    Logs,
}

pub(super) struct App {
    msg_rx: Receiver<Message>,
    req_tx: Sender<Request>,
    msg_tx: Sender<Message>,
    pub(super) processes: Vec<ProcessInfo>,
    pub(super) table: TableState,
    pub(super) logs_name: Option<String>,
    pub(super) logs: Vec<LogLine>,
    pub(super) log_follow: bool,
    pub(super) log_scroll: u16,
    pub(super) log_view: u16,
    log_generation: Arc<AtomicU64>,
    pub(super) modal: Modal,
    pub(super) focus: Pane,
    pub(super) status: String,
    status_at: Instant,
    pub(super) status_error: bool,
    pub(super) last_ok: Option<Instant>,
    should_quit: bool,
    next_proc_poll: Instant,
}

pub(super) enum Modal {
    None,
    Start(StartForm),
    Confirm { name: String },
    Help,
}

#[derive(Default)]
pub(super) struct StartForm {
    pub(super) name: String,
    pub(super) command: String,
    pub(super) cwd: String,
    pub(super) field: usize,
    pub(super) error: Option<String>,
}

impl StartForm {
    pub(super) const fn active(&self) -> &String {
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
    pub(super) fn new(
        msg_rx: Receiver<Message>,
        req_tx: Sender<Request>,
        msg_tx: Sender<Message>,
    ) -> Self {
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
            log_view: 1,
            log_generation: Arc::new(AtomicU64::new(0)),
            modal: Modal::None,
            focus: Pane::Processes,
            status: String::new(),
            status_at: Instant::now(),
            status_error: false,
            last_ok: None,
            should_quit: false,
            next_proc_poll: Instant::now(),
        }
    }

    pub(super) fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), String> {
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
            let _ = std::thread::spawn(move || {
                super::socket::log_stream(&name, &generation, epoch, &tx);
            });
        }
        true
    }

    pub(super) fn selected(&self) -> Option<&ProcessInfo> {
        self.table.selected().and_then(|i| self.processes.get(i))
    }

    fn selected_name(&self) -> Option<&str> {
        self.selected().map(|p| p.name.as_str())
    }

    pub(super) fn connected(&self) -> bool {
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
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        // Shift+J/K scroll the selected process's logs from either pane.
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('?') => {
                self.modal = Modal::Help;
                return;
            }
            KeyCode::Char('s' | 'n') => {
                self.modal = Modal::Start(default_form());
                return;
            }
            KeyCode::Char('d') => {
                self.confirm_remove();
                return;
            }
            KeyCode::Char('r') => {
                if let Some(name) = self.selected_name().map(str::to_owned) {
                    self.send(Request::Restart { name }, "restarting…");
                }
                return;
            }
            KeyCode::Char('x') => {
                if let Some(name) = self.selected_name().map(str::to_owned) {
                    self.send(Request::Stop { name }, "stopping…");
                }
                return;
            }
            KeyCode::Char('f') => {
                self.log_follow = !self.log_follow;
                return;
            }
            KeyCode::Char('J') => {
                self.scroll_logs(1);
                return;
            }
            KeyCode::Char('K') => {
                self.scroll_logs(-1);
                return;
            }
            KeyCode::Char('j') if shift => {
                self.scroll_logs(1);
                return;
            }
            KeyCode::Char('k') if shift => {
                self.scroll_logs(-1);
                return;
            }
            KeyCode::PageDown => {
                self.scroll_logs_page(1);
                return;
            }
            KeyCode::PageUp => {
                self.scroll_logs_page(-1);
                return;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.log_follow = true;
                return;
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.log_follow = false;
                self.log_scroll = 0;
                return;
            }
            _ => {}
        }

        match self.focus {
            Pane::Processes => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.select_next(),
                KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
                KeyCode::Enter | KeyCode::Tab => self.focus = Pane::Logs,
                KeyCode::Esc => self.should_quit = true,
                _ => {}
            },
            Pane::Logs => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.scroll_logs(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll_logs(-1),
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
                    self.focus = Pane::Processes;
                }
                _ => {}
            },
        }
    }

    fn scroll_logs(&mut self, delta: i32) {
        self.log_follow = false;
        let next = i64::from(self.log_scroll) + i64::from(delta);
        self.log_scroll = super::widgets::clamp_u16(usize::try_from(next.max(0)).unwrap_or(0));
    }

    fn scroll_logs_page(&mut self, direction: i32) {
        let step = i32::from(self.log_view.max(1));
        self.scroll_logs(direction * step);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::protocol::{LogStream, ProcessStatus};

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
        for needle in ["keys", "select or scroll down", "toggle log follow", "quit"] {
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

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn enter_focuses_logs_and_scrolls() {
        let mut app = new_app();
        app.set_processes(vec![sample()]);

        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Pane::Logs);

        app.log_follow = true;
        app.log_scroll = 5;
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.log_scroll, 6);
        assert!(!app.log_follow);

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.focus, Pane::Processes);
    }

    #[test]
    fn shift_jk_scrolls_logs_from_processes() {
        let mut app = new_app();
        app.set_processes(vec![sample()]);
        assert_eq!(app.focus, Pane::Processes);

        app.log_scroll = 5;
        press(&mut app, KeyCode::Char('J'));
        assert_eq!(app.log_scroll, 6);
        press(&mut app, KeyCode::Char('K'));
        assert_eq!(app.log_scroll, 5);
        assert!(!app.log_follow);
    }

    #[test]
    fn s_opens_start_form() {
        let mut app = new_app();
        press(&mut app, KeyCode::Char('s'));
        assert!(matches!(app.modal, Modal::Start(_)));
        app.modal = Modal::None;
        press(&mut app, KeyCode::Char('n'));
        assert!(matches!(app.modal, Modal::Start(_)));
    }
}
