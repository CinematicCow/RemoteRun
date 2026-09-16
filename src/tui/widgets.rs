//! Small drawing helpers: panel chrome, status styling and the string
//! formatting shared by the process and log views.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding};

use super::{ACCENT, FAINT, MUTED};
use crate::client::format_duration;
use crate::protocol::{LogLine, LogStream, ProcessStatus};

pub(super) fn panel(title: impl Into<String>) -> Block<'static> {
    panel_focus(title, false)
}

pub(super) fn panel_focus(title: impl Into<String>, focused: bool) -> Block<'static> {
    let border = if focused { ACCENT } else { FAINT };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(Line::raw(title.into()).style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)))
}

pub(super) fn panel_padded(title: impl Into<String>) -> Block<'static> {
    panel(title).padding(Padding::horizontal(1))
}

pub(super) fn panel_padded_focus(title: impl Into<String>, focused: bool) -> Block<'static> {
    panel_focus(title, focused).padding(Padding::horizontal(1))
}

pub(super) const fn status_color(status: ProcessStatus) -> Color {
    match status {
        ProcessStatus::Running => Color::Green,
        ProcessStatus::Stopped => Color::DarkGray,
        ProcessStatus::Crashed => Color::Red,
        ProcessStatus::Backoff => Color::Yellow,
    }
}

pub(super) fn status_line(status: ProcessStatus) -> Line<'static> {
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

pub(super) fn right(value: String) -> Line<'static> {
    Line::raw(value).alignment(Alignment::Right)
}

pub(super) fn or_dash<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

pub(super) fn uptime_text(secs: Option<u64>) -> String {
    secs.map_or_else(|| "-".to_owned(), format_duration)
}

pub(super) fn field(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9} "), Style::new().fg(FAINT)),
        Span::styled(value, Style::new().fg(MUTED)),
    ])
}

pub(super) fn short_time(ts: &str) -> &str {
    ts.get(11..19).unwrap_or(ts)
}

pub(super) fn log_line_widget(l: &LogLine) -> Line<'static> {
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

pub(super) fn clamp_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

pub(super) fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("{count} {word}")
    } else {
        format!("{count} {word}s")
    }
}

pub(super) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
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
}
