use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::panel;
use crate::permissions::PermissionsState;
use crate::theme;
use crate::ui::panel_block;

pub fn render_permissions_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &PermissionsState,
) {
    let block = panel_block(panel::PERMISSIONS, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref err) = state.error {
        let item = ListItem::new(Line::from(Span::styled(
            err.as_str(),
            Style::new().fg(theme::RED),
        )));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    if state.permissions.is_empty() {
        let item = ListItem::new(Line::from(Span::styled(
            "loading permissions…",
            Style::new().fg(theme::MUTED),
        )));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    let visible_height = inner.height as usize;
    let total = state.permissions.len();

    let start = if state.selected_index >= visible_height {
        state.selected_index - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = state.permissions[start..end]
        .iter()
        .enumerate()
        .map(|(i, (perm, granted))| {
            let actual_index = start + i;
            let selected = actual_index == state.selected_index && focused;
            let name = PermissionsState::short_name(perm);
            let accent = panel::by_number(panel::PERMISSIONS).bright_color;

            let style = if selected {
                Style::new().fg(theme::YELLOW).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::FG)
            };
            let prefix = if selected { "▸ " } else { "  " };

            let mut spans = vec![
                Span::styled(prefix, style),
            ];
            if *granted {
                spans.push(Span::styled("● ", Style::new().fg(accent)));
            } else {
                spans.push(Span::styled("  ", Style::default()));
            }
            spans.push(Span::styled(name, style));

            ListItem::new(Line::from(spans))
        })
        .collect();

    frame.render_widget(List::new(items), inner);

    if total > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(theme::MUTED))
            .track_style(Style::new().fg(theme::SURFACE));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}
