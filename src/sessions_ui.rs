use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::session::{self, SessionId, SessionSummary};
use crate::sessions::SessionsState;
use crate::theme;

/// Centered dialog listing past sessions for the current package across
/// every device the user has captured against. Mirrors the device/app
/// dropdowns in spirit — modal, dim background, single selectable list
/// with a hint footer.
///
/// `viewing_id` is the id of the session currently on screen (sourced
/// from `App::session_view()`). Used to draw the `(viewing)` badge on
/// the matching row.
///
/// `flash` carries the app-wide status flash through so it can render
/// inside the dialog. The main block's `title_bottom` flash is buried
/// under the dim overlay while this dialog is open.
pub fn render_sessions_dialog(
    frame: &mut Frame,
    area: Rect,
    state: &SessionsState,
    viewing_id: Option<&SessionId>,
    flash: Option<(String, bool)>,
) {
    let t = theme::current();

    // 96 cols comfortably fits all five columns; cap to terminal size on
    // tiny terminals so we don't draw outside the screen.
    let width = 96u16.min(area.width.saturating_sub(4));
    let height = 24u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let bottom = Line::from(vec![
        // Same glyph the files panel uses for `Enter` (↩, U+21A9) so the
        // visual vocabulary stays consistent across modal lists.
        Span::styled(" \u{21a9}", Style::new().fg(t.danger)),
        Span::styled(" open ", Style::new().fg(t.muted)),
        Span::styled("c", Style::new().fg(t.danger)),
        Span::styled("opy path ", Style::new().fg(t.muted)),
        Span::styled("d", Style::new().fg(t.danger)),
        Span::styled("elete ", Style::new().fg(t.muted)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(" history ", Style::new().fg(t.fg))))
        .title_bottom(bottom)
        .border_style(Style::new().fg(t.surface))
        .style(Style::new().bg(t.bg));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if let Some(err) = state.error.as_deref() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                err,
                Style::new().fg(t.danger),
            ))),
            inner,
        );
        return;
    }

    let live_summary = state
        .live_id
        .as_ref()
        .and_then(|id| state.sessions.iter().find(|s| &s.id == id));
    let history: Vec<&SessionSummary> = state.visible_history().collect();
    let total = 1 + history.len();
    let selected = state.selected.min(total.saturating_sub(1));

    let mut rows: Vec<Row> = Vec::with_capacity(total);
    rows.push(live_row(t, live_summary));
    for s in &history {
        rows.push(history_row(t, s, viewing_id));
    }

    // Column widths chosen to fit a typical terminal with a 96-col dialog.
    // The trailing `Min(0)` column carries the optional `(viewing)` badge
    // and absorbs any leftover space so the row's right edge lines up.
    let widths = [
        Constraint::Length(19), // WHEN — "YYYY-MM-DD HH:MM:SS"
        Constraint::Length(8),  // DURATION — "1h2m" / "n/a" / "live"
        Constraint::Length(9),  // SIZE — "999.9 MB"
        Constraint::Length(20), // DEVICE — model preferred, serial fallback
        Constraint::Length(7),  // PID
        Constraint::Min(0),     // STATUS — "(viewing)" or empty
    ];

    let header = Row::new(vec![
        Cell::from("WHEN"),
        Cell::from("DURATION"),
        Cell::from("SIZE"),
        Cell::from("DEVICE"),
        Cell::from("PID"),
        Cell::from(""),
    ])
    .style(Style::new().fg(t.muted).add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2)
        .row_highlight_style(Style::new().fg(t.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25b8} ");

    // Reserve a single footer row at the bottom of the inner area for the
    // sessions root path and the retention notice. A blank row above keeps
    // the footer visually detached from the last table row.
    let [table_area, _gap, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, table_area, &mut table_state);

    let footer = match flash {
        Some((msg, is_error)) => {
            let color = if is_error { t.danger } else { t.success };
            Line::from(Span::styled(msg, Style::new().fg(color)))
        }
        None => {
            let muted = Style::new().fg(t.muted);
            let dim = Style::new().fg(t.surface);
            let path = session::sessions_root().display().to_string();
            Line::from(vec![
                Span::styled("saved to ", dim),
                Span::styled(path, muted),
                Span::styled("  \u{00b7}  ", dim),
                Span::styled("auto-deleted after 5 days", muted),
            ])
        }
    };
    frame.render_widget(Paragraph::new(footer), footer_area);
}

/// Synthetic Live row pinned at the top. Selecting it always returns the
/// user to live data — closing any captured-session view that was open and
/// closing the dialog.
fn live_row(t: &theme::Theme, live: Option<&SessionSummary>) -> Row<'static> {
    let bold = Style::new().fg(t.fg).add_modifier(Modifier::BOLD);
    let muted = Style::new().fg(t.muted);
    let success = Style::new().fg(t.success);

    match live {
        Some(s) => {
            let when = s.started_at.format("%Y-%m-%d %H:%M:%S").to_string();
            Row::new(vec![
                Cell::from(when).style(bold),
                Cell::from("live").style(muted),
                Cell::from(format_size(s.size_bytes)).style(muted),
                Cell::from(device_label(s)).style(muted),
                Cell::from(s.pid.to_string()).style(muted),
                Cell::from("(live)").style(success),
            ])
        }
        None => {
            // No app attached. Em-dashes hold the DURATION/SIZE/DEVICE/PID
            // columns so the grid alignment matches every other row, and
            // the variable-width STATUS column carries the hint about what
            // selecting this row does.
            let dash = "\u{2014}";
            Row::new(vec![
                Cell::from("Live").style(bold),
                Cell::from(dash).style(muted),
                Cell::from(dash).style(muted),
                Cell::from(dash).style(muted),
                Cell::from(dash).style(muted),
                Cell::from("no app attached").style(muted),
            ])
        }
    }
}

fn history_row(t: &theme::Theme, s: &SessionSummary, viewing_id: Option<&SessionId>) -> Row<'static> {
    let is_viewing = viewing_id == Some(&s.id);
    let when = s.started_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let duration = match s.duration {
        Some(d) => format_duration(d),
        None => "n/a".to_string(),
    };
    let size = format_size(s.size_bytes);
    let device = device_label(s);
    let muted = Style::new().fg(t.muted);
    let fg = Style::new().fg(t.fg);
    let badge = if is_viewing {
        Cell::from("(viewing)").style(Style::new().fg(t.accent))
    } else {
        Cell::from("")
    };
    Row::new(vec![
        Cell::from(when).style(fg),
        Cell::from(duration).style(muted),
        Cell::from(size).style(muted),
        Cell::from(device).style(muted),
        Cell::from(s.pid.to_string()).style(muted),
        badge,
    ])
}

/// Prefer a human-readable model when one is captured; fall back to the
/// raw serial. Truncation is handled by the Table's column width.
fn device_label(s: &SessionSummary) -> String {
    s.device_model
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or(&s.device_serial)
        .to_string()
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats() {
        assert_eq!(format_duration(std::time::Duration::from_secs(5)), "5s");
        assert_eq!(format_duration(std::time::Duration::from_secs(75)), "1m15s");
        assert_eq!(format_duration(std::time::Duration::from_secs(3725)), "1h2m");
    }

    #[test]
    fn size_formats() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    fn summary_with_device(model: Option<&str>, serial: &str) -> SessionSummary {
        SessionSummary {
            id: crate::session::SessionId {
                package: "com.test".into(),
                dir_name: "x".into(),
            },
            started_at: chrono::Local::now(),
            ended_at: None,
            duration: None,
            size_bytes: 0,
            pid: 0,
            device_serial: serial.into(),
            device_model: model.map(String::from),
        }
    }

    #[test]
    fn device_label_prefers_model_when_present() {
        let s = summary_with_device(Some("Pixel 8"), "1234abcd");
        assert_eq!(device_label(&s), "Pixel 8");
    }

    #[test]
    fn device_label_falls_back_to_serial_for_empty_or_missing_model() {
        let s = summary_with_device(None, "emulator-5554");
        assert_eq!(device_label(&s), "emulator-5554");
        let s = summary_with_device(Some(""), "emulator-5554");
        assert_eq!(device_label(&s), "emulator-5554");
    }
}
