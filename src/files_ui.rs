use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};

use crate::files::{FileConfirm, FilesState, FlatEntry};
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

    let flash_active = state
        .action_flash
        .as_ref()
        .is_some_and(|(_, t)| t.elapsed() < std::time::Duration::from_secs(1));

    let flash_label = if flash_active {
        Some(state.action_flash.as_ref().unwrap().0)
    } else {
        None
    };

    let bottom_spans = if let Some(FileConfirm::Pull(_)) = &state.confirming {
        Some(vec![
            Span::styled(" pp", accent),
            Span::styled("ull ", Style::new().fg(theme::YELLOW)),
            Span::styled("───", border_fg),
            Span::styled(" any", accent),
            Span::styled(" cancel ", muted),
        ])
    } else if let Some(FileConfirm::Open(_)) = &state.confirming {
        Some(vec![
            Span::styled(" oo", accent),
            Span::styled("pen ", Style::new().fg(theme::YELLOW)),
            Span::styled("───", border_fg),
            Span::styled(" any", accent),
            Span::styled(" cancel ", muted),
        ])
    } else if focused {
        let pull_spans: Vec<Span> = if flash_label == Some("pulling...") {
            vec![Span::styled(" pulling... ", Style::new().fg(theme::GREEN))]
        } else {
            vec![
                Span::styled(" p", accent),
                Span::styled("ull ", muted),
            ]
        };
        let open_spans: Vec<Span> = if flash_label == Some("opening...") {
            vec![Span::styled(" opening... ", Style::new().fg(theme::GREEN))]
        } else {
            vec![
                Span::styled(" o", accent),
                Span::styled("pen ", muted),
            ]
        };
        let mut spans = Vec::new();
        spans.extend(pull_spans);
        spans.push(Span::styled("───", border_fg));
        spans.extend(open_spans);
        spans.extend([
            Span::styled("───", border_fg),
            Span::styled(" esc", accent),
            Span::styled(" back ", muted),
        ]);
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

    let root_label = format!("data/data/{}", state.package);
    let root_line = ListItem::new(Line::from(Span::styled(root_label, Style::new().fg(theme::FG))));
    frame.render_widget(List::new(vec![root_line]), inner);

    let tree_area = Rect {
        y: inner.y + 1,
        height: inner.height.saturating_sub(1),
        ..inner
    };

    if let Some(ref err) = state.error {
        let item = ListItem::new(Line::from(Span::styled(
            err.as_str(),
            Style::new().fg(theme::RED),
        )));
        frame.render_widget(List::new(vec![item]), tree_area);
        return;
    }

    let children = match &state.root_children {
        Some(c) => c,
        None => {
            let item = ListItem::new(Line::from(Span::styled(
                "loading...",
                Style::new().fg(theme::MUTED),
            )));
            frame.render_widget(List::new(vec![item]), tree_area);
            return;
        }
    };

    if children.is_empty() {
        let item = ListItem::new(Line::from(Span::styled(
            "empty",
            Style::new().fg(theme::MUTED),
        )));
        frame.render_widget(List::new(vec![item]), tree_area);
        return;
    }

    let flat = state.flatten_visible();
    let visible_height = tree_area.height as usize;
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

    frame.render_widget(List::new(items), tree_area);
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
