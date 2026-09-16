//! Rendering: the whole screen is redrawn from `App` state each frame.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Cell, Clear, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Table,
};

use super::app::{App, Modal, Pane, StartForm};
use super::widgets::{
    centered_rect, clamp_u16, field, log_line_widget, or_dash, panel, panel_focus, panel_padded,
    panel_padded_focus, plural, right, status_color, status_line, uptime_text,
};
use super::{ACCENT, FAINT, MUTED, SELECT_BG};
use crate::protocol::ProcessStatus;

impl App {
    pub(super) fn render(&mut self, frame: &mut Frame) {
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
        self.render_footer(frame, footer);
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

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let keys: &[(&str, &str)] = if self.focus == Pane::Logs {
            &[
                ("q", "quit"),
                ("j/k", "scroll"),
                ("esc", "back"),
                ("s", "start"),
                ("?", "help"),
            ]
        } else {
            &[
                ("q", "quit"),
                ("j/k", "select"),
                ("enter", "logs"),
                ("s", "start"),
                ("x", "stop"),
                ("?", "help"),
            ]
        };
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
        .block(panel_focus(title, self.focus == Pane::Processes))
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
        let block = panel_padded_focus(title, self.focus == Pane::Logs).title_bottom(bottom);

        self.log_view = area.height.saturating_sub(2);
        let view = usize::from(self.log_view);
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
            ("j / ↓", "select or scroll down"),
            ("k / ↑", "select or scroll up"),
            ("J / K", "scroll logs one line"),
            ("enter / tab", "focus the logs pane"),
            ("esc", "leave logs, then quit"),
            ("pgup/pgdn", "scroll logs a page"),
            ("g / G", "logs top / follow"),
            ("f", "toggle log follow"),
            ("s / n", "start a process"),
            ("x", "stop selected"),
            ("r", "restart selected"),
            ("d", "remove selected"),
            ("? ", "close this help"),
            ("q", "quit"),
        ];
        let height = u16::try_from(rows.len()).unwrap_or(u16::MAX) + 2;
        let rect = centered_rect(area, 48, height);
        frame.render_widget(Clear, rect);
        let lines = rows
            .iter()
            .map(|(key, desc)| {
                Line::from(vec![
                    Span::styled(
                        format!(" {key:<11} "),
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
