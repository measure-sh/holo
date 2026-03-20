use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::adb::Adb;
use crate::theme;

pub fn spawn_poller(adb: Arc<dyn Adb>, serial: String) -> mpsc::Receiver<Vec<String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(60);
        loop {
            if let Ok(packages) = adb.list_packages(&serial) {
                if tx.send(packages).is_err() {
                    return;
                }
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

const PID_WIDTH: u16 = 7;

pub fn render_apps(frame: &mut Frame, area: Rect, packages: Option<&[String]>, selected: Option<usize>, processes: Option<&HashMap<String, u32>>) {
    let Some(packages) = packages else {
        let list = List::new(vec![ListItem::new(Span::styled(
            "Loading…",
            Style::new().fg(theme::MUTED),
        ))]);
        frame.render_widget(list, area);
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(PID_WIDTH), Constraint::Min(0)])
        .split(rows[0]);

    let header_style = Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD);
    frame.render_widget(Paragraph::new("PID").style(header_style), header_cols[0]);
    frame.render_widget(Paragraph::new("Package").style(header_style), header_cols[1]);

    let items: Vec<ListItem> = packages
        .iter()
        .map(|name| {
            let pid_str = processes
                .and_then(|p| p.get(name.as_str()))
                .map(|pid| format!("{pid:<width$}", width = PID_WIDTH as usize))
                .unwrap_or_else(|| " ".repeat(PID_WIDTH as usize));

            ListItem::new(Line::from(vec![
                Span::styled(pid_str, Style::new().fg(theme::MUTED)),
                Span::styled(name.clone(), Style::new().fg(theme::FG)),
            ]))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    let clamped = selected.map(|i| i.min(packages.len().saturating_sub(1)));
    let mut state = ListState::default().with_selected(clamped);
    frame.render_stateful_widget(list, rows[1], &mut state);
}
