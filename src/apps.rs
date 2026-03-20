use std::sync::{mpsc, Arc};
use std::time::Duration;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{List, ListItem, ListState},
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

pub fn render_apps(frame: &mut Frame, area: Rect, packages: Option<&[String]>, selected: Option<usize>) {
    let Some(packages) = packages else {
        let list = List::new(vec![ListItem::new(Span::styled(
            "Loading…",
            Style::new().fg(theme::MUTED),
        ))]);
        frame.render_widget(list, area);
        return;
    };

    let items: Vec<ListItem> = packages
        .iter()
        .map(|name| ListItem::new(Span::styled(name.clone(), Style::new().fg(theme::FG))))
        .collect();
    let list = List::new(items)
        .highlight_style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    let clamped = selected.map(|i| i.min(packages.len().saturating_sub(1)));
    let mut state = ListState::default().with_selected(clamped);
    frame.render_stateful_widget(list, area, &mut state);
}
