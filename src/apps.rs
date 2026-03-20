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

pub fn fuzzy_matches(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let name_lower = name.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    let mut chars = query_lower.chars();
    let mut current = chars.next();
    for c in name_lower.chars() {
        if current == Some(c) {
            current = chars.next();
        }
    }
    current.is_none()
}

pub fn filtered_packages<'a>(packages: &'a [String], query: &str) -> Vec<&'a str> {
    packages
        .iter()
        .filter(|p| fuzzy_matches(p, query))
        .map(|s| s.as_str())
        .collect()
}

pub fn render_apps(frame: &mut Frame, area: Rect, packages: Option<&[String]>, selected: Option<usize>, processes: Option<&HashMap<String, u32>>, filter: &str) {
    let Some(packages) = packages else {
        let list = List::new(vec![ListItem::new(Span::styled(
            "Loading…",
            Style::new().fg(theme::MUTED),
        ))]);
        frame.render_widget(list, area);
        return;
    };

    let filtered = filtered_packages(packages, filter);

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

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|&name| {
            let pid_str = if let Some(pid) = processes.and_then(|p| p.get(name)) {
                Span::styled(format!("{pid:<width$}", width = PID_WIDTH as usize), Style::new().fg(theme::MUTED))
            } else {
                Span::styled(format!("{:<width$}", "zzz", width = PID_WIDTH as usize), Style::new().fg(theme::SURFACE))
            };

            ListItem::new(Line::from(vec![
                pid_str,
                Span::styled(name.to_string(), Style::new().fg(theme::FG)),
            ]))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    let clamped = selected.map(|i| i.min(filtered.len().saturating_sub(1)));
    let mut state = ListState::default().with_selected(clamped);
    frame.render_stateful_widget(list, rows[1], &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_empty_query() {
        assert!(fuzzy_matches("com.example.app", ""));
    }

    #[test]
    fn fuzzy_matches_exact() {
        assert!(fuzzy_matches("com.example.app", "com.example.app"));
    }

    #[test]
    fn fuzzy_matches_subsequence() {
        assert!(fuzzy_matches("com.example.app", "cexa"));
    }

    #[test]
    fn fuzzy_matches_case_insensitive() {
        assert!(fuzzy_matches("com.Example.App", "exa"));
        assert!(fuzzy_matches("com.example.app", "EXA"));
    }

    #[test]
    fn fuzzy_no_match() {
        assert!(!fuzzy_matches("com.example.app", "xyz"));
    }

    #[test]
    fn fuzzy_query_longer_than_name() {
        assert!(!fuzzy_matches("ab", "abc"));
    }

    #[test]
    fn filtered_packages_returns_matches() {
        let pkgs = vec![
            "com.spotify.music".to_string(),
            "com.whatsapp".to_string(),
            "com.android.chrome".to_string(),
        ];
        let result = filtered_packages(&pkgs, "spot");
        assert_eq!(result, vec!["com.spotify.music"]);
    }

    #[test]
    fn filtered_packages_empty_query_returns_all() {
        let pkgs = vec!["a".to_string(), "b".to_string()];
        assert_eq!(filtered_packages(&pkgs, "").len(), 2);
    }
}
