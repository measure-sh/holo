use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};

use crate::files::{FilesState, FlatEntry};
use crate::panel;
use crate::theme;
use crate::ui::panel_title;

pub fn render_files_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &FilesState,
) {
    let color = panel::by_number(panel::FILES).border_color(focused);
    let accent = Style::new().fg(panel::by_number(panel::FILES).bright_color);
    let muted = Style::new().fg(theme::MUTED);
    let border_fg = Style::new().fg(color);

    let pull_flash_active = state
        .pull_flash
        .as_ref()
        .is_some_and(|(_, t)| t.elapsed() < std::time::Duration::from_secs(2));

    let bottom_spans = if focused {
        let mut spans = vec![
            Span::styled(" p", accent),
            Span::styled("ull ", muted),
            Span::styled("───", border_fg),
            Span::styled(" o", accent),
            Span::styled("pen ", muted),
            Span::styled("───", border_fg),
            Span::styled(" esc", accent),
            Span::styled(" back ", muted),
        ];
        if pull_flash_active {
            spans.insert(0, Span::styled("───", border_fg));
            spans.insert(0, Span::styled(" pulled! ", Style::new().fg(theme::GREEN)));
        }
        Some(spans)
    } else {
        None
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::FILES, focused))
        .border_style(border_fg);
    if let Some(spans) = bottom_spans {
        block = block.title_bottom(Line::from(spans));
    }
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

    let children = match &state.root_children {
        Some(c) => c,
        None => {
            let item = ListItem::new(Line::from(Span::styled(
                "loading...",
                Style::new().fg(theme::MUTED),
            )));
            frame.render_widget(List::new(vec![item]), inner);
            return;
        }
    };

    if children.is_empty() {
        let item = ListItem::new(Line::from(Span::styled(
            "empty",
            Style::new().fg(theme::MUTED),
        )));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    let flat = state.flatten_visible();
    let visible_height = inner.height as usize;
    if visible_height == 0 || flat.is_empty() {
        return;
    }

    let selected = state.selected_index;
    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(flat.len());

    let accent = panel::by_number(panel::FILES).bright_color;

    let items: Vec<ListItem> = flat[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = (start + i) == selected;
            build_tree_line(entry, is_selected, focused, accent)
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn build_tree_line(
    entry: &FlatEntry,
    selected: bool,
    focused: bool,
    accent: ratatui::style::Color,
) -> ListItem<'static> {
    let mut spans: Vec<Span> = Vec::new();

    for level in 0..entry.depth {
        let is_last = entry.ancestor_is_last.get(level).copied().unwrap_or(false);
        if is_last {
            spans.push(Span::styled("   ", Style::new().fg(theme::MUTED)));
        } else {
            spans.push(Span::styled("│  ", Style::new().fg(theme::MUTED)));
        }
    }

    if entry.depth > 0 {
        if entry.is_last_sibling {
            spans.push(Span::styled("└─ ", Style::new().fg(theme::MUTED)));
        } else {
            spans.push(Span::styled("├─ ", Style::new().fg(theme::MUTED)));
        }
    }

    if entry.is_dir {
        if entry.loading {
            spans.push(Span::styled("⟳ ", Style::new().fg(theme::MUTED)));
        } else if entry.expanded {
            spans.push(Span::styled("▾ ", Style::new().fg(theme::FG)));
        } else {
            spans.push(Span::styled("▸ ", Style::new().fg(theme::FG)));
        }
    }

    let name_style = if selected && focused {
        Style::new().fg(accent).add_modifier(Modifier::BOLD)
    } else if selected {
        Style::new().fg(theme::FG).add_modifier(Modifier::BOLD)
    } else if entry.is_dir {
        Style::new().fg(theme::FG)
    } else {
        Style::new().fg(theme::MUTED)
    };

    spans.push(Span::styled(entry.name.clone(), name_style));

    ListItem::new(Line::from(spans))
}
