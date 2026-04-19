use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::issues::{IssueKind, IssuesState};
use crate::panel;
use crate::theme;
use crate::ui::panel_title;

pub fn render_issues_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &IssuesState,
) {
    let t = theme::current();
    let color = panel::by_number(panel::ISSUES).border_color(focused);
    let muted = Style::new().fg(t.muted);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::ISSUES, focused))
        .border_style(Style::new().fg(color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" \u{21a9}", Style::new().fg(t.danger)),
            Span::styled(" open ", muted),
        ]));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(err) = state.error() {
        let item = ListItem::new(Line::from(Span::styled(
            err,
            Style::new().fg(t.danger),
        )));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    if state.issues.is_empty() {
        let item = ListItem::new(Line::from(Span::styled(
            " no issues",
            muted,
        )));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    let visible_height = inner.height as usize;
    let total = state.issues.len();
    let selected = state.selected;

    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = state.issues[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let actual = start + i;
            let is_selected = actual == selected && focused;
            let style = if is_selected {
                Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.fg)
            };
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let tag = match entry.kind {
                IssueKind::Crash => "CRASH",
                IssueKind::Anr => "ANR  ",
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(tag, Style::new().fg(t.muted)),
                Span::styled(" ", Style::new()),
                Span::styled(&entry.timestamp, Style::new().fg(t.muted)),
                Span::styled(" ", Style::new()),
                Span::styled(&entry.description, style),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);

    if total > visible_height && inner.height > 0 && inner.width > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}
