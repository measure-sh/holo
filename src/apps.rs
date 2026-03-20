use std::sync::{mpsc, Arc};
use std::time::Duration;

use ratatui::{
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{List, ListItem},
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

pub fn render_apps(frame: &mut Frame, area: Rect, packages: Option<&[String]>) {
    let Some(packages) = packages else {
        let list = List::new(vec![ListItem::new(Span::styled(
            "Loading…",
            Style::new().fg(theme::MUTED),
        ))]);
        frame.render_widget(list, area);
        return;
    };

    let mut items: Vec<ListItem> = Vec::with_capacity(packages.len() + 1);
    items.push(ListItem::new(Span::styled(
        format!("{} packages", packages.len()),
        Style::new().fg(theme::MUTED),
    )));
    for name in packages {
        items.push(ListItem::new(Span::styled(
            name.clone(),
            Style::new().fg(theme::FG),
        )));
    }
    let list = List::new(items);
    frame.render_widget(list, area);
}
