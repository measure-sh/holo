use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use crate::files::{DetailKind, FilesState, FlatEntry};
use crate::panel;
use crate::theme;
use crate::ui::{panel_title, render_pane_chip, split_chip, wrap_line};

pub fn render_files_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &mut FilesState,
) {
    let t = theme::current();
    let color = panel::by_number(panel::FILES).border_color(focused);
    let border_fg = Style::new().fg(color);

    let tree_active = focused && !state.detail_focused;
    let detail_active = focused && state.detail_focused;
    let tree_hint = focused && !tree_active && state.detail_open;
    let detail_hint = focused && !detail_active;

    let bottom_spans = build_bottom_bar(state, focused, color);
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

    if state.detail_open {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(inner);
        render_tree(frame, chunks[0], state, tree_active, true, tree_hint);
        render_detail(frame, chunks[1], state, detail_active, detail_hint);
    } else {
        render_tree(frame, inner, state, focused, false, false);
    }
    let _ = t;
}

fn build_bottom_bar(
    state: &FilesState,
    focused: bool,
    color: ratatui::style::Color,
) -> Option<Vec<Span<'static>>> {
    if !focused {
        return None;
    }
    let t = theme::current();
    let accent = Style::new().fg(t.danger);
    let muted = Style::new().fg(t.muted);
    let border_fg = Style::new().fg(color);

    let flash_active = state
        .action_flash
        .as_ref()
        .is_some_and(|(_, ts)| ts.elapsed() < std::time::Duration::from_secs(1));
    let flash_label = if flash_active {
        Some(state.action_flash.as_ref().unwrap().0)
    } else {
        None
    };

    let mut spans: Vec<Span<'static>> = Vec::new();

    if state.detail_focused {
        spans.extend([
            Span::styled(" jk", accent),
            Span::styled(" scroll ", muted),
            Span::styled("───", border_fg),
            Span::styled(" o", accent),
            Span::styled(" open in editor ", muted),
            Span::styled("───", border_fg),
            Span::styled(" esc", accent),
            Span::styled(" close ", muted),
        ]);
        return Some(spans);
    }

    if flash_label == Some("opening...") {
        spans.push(Span::styled(" opening... ", Style::new().fg(t.success)));
    } else {
        spans.extend([
            Span::styled(" ↩", accent),
            Span::styled(" detail ", muted),
            Span::styled("───", border_fg),
            Span::styled(" o", accent),
            Span::styled(" open ", muted),
        ]);
    }

    if state.detail_open {
        spans.extend([
            Span::styled("───", border_fg),
            Span::styled(" tab", accent),
            Span::styled(" focus ", muted),
        ]);
    }

    spans.extend([
        Span::styled("───", border_fg),
        Span::styled(" r", accent),
        Span::styled("efresh ", muted),
    ]);

    Some(spans)
}

fn render_tree(
    frame: &mut Frame,
    area: Rect,
    state: &FilesState,
    active: bool,
    in_split: bool,
    show_hint: bool,
) {
    let t = theme::current();

    let body_area = if in_split {
        let (chip_area, body_area) = split_chip(area);
        render_pane_chip(frame, chip_area, "files", active, show_hint);
        body_area
    } else {
        area
    };
    if body_area.width == 0 || body_area.height == 0 {
        return;
    }

    let root_label = format!("data/data/{}", state.package);
    let root_line = ListItem::new(Line::from(Span::styled(root_label, Style::new().fg(t.fg))));
    let root_area = Rect { height: 1, ..body_area };
    frame.render_widget(List::new(vec![root_line]), root_area);

    let tree_area = Rect {
        y: body_area.y + 1,
        height: body_area.height.saturating_sub(1),
        ..body_area
    };

    if let Some(ref err) = state.error {
        let item = ListItem::new(Line::from(Span::styled(
            err.as_str(),
            Style::new().fg(t.danger),
        )));
        frame.render_widget(List::new(vec![item]), tree_area);
        return;
    }

    let children = match &state.root_children {
        Some(c) => c,
        None => {
            let item = ListItem::new(Line::from(Span::styled(
                "loading...",
                Style::new().fg(t.muted),
            )));
            frame.render_widget(List::new(vec![item]), tree_area);
            return;
        }
    };

    if children.is_empty() {
        let item = ListItem::new(Line::from(Span::styled(
            "empty",
            Style::new().fg(t.muted),
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

    let items: Vec<ListItem> = flat[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = (start + i) == selected;
            build_tree_line(entry, is_selected, active)
        })
        .collect();

    frame.render_widget(List::new(items), tree_area);
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut FilesState,
    active: bool,
    show_hint: bool,
) {
    let t = theme::current();
    let fg = Style::new().fg(t.fg);
    let muted = Style::new().fg(t.muted);
    let danger = Style::new().fg(t.danger);
    let accent = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);

    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.muted));
    let detail_inner = separator.inner(area);
    frame.render_widget(separator, area);
    if detail_inner.width == 0 || detail_inner.height == 0 {
        return;
    }

    let (chip_area, rest) = split_chip(detail_inner);
    let label = state
        .selected_file
        .as_deref()
        .map(file_name)
        .unwrap_or("detail");
    render_pane_chip(frame, chip_area, label, active, show_hint);

    if let Some(meta) = state.selected_meta.as_ref() {
        let stats_text = format!("{} ", format_size(meta.size_bytes));
        let stats_line = Line::from(Span::styled(stats_text, muted)).alignment(Alignment::Right);
        frame.render_widget(stats_line, chip_area);
    }

    let body_area = rest;
    if body_area.width == 0 || body_area.height == 0 {
        return;
    }

    if let Some(err) = &state.detail_error {
        let item = ListItem::new(Line::from(Span::styled(format!(" {err}"), danger)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    }

    if state.loading_meta {
        let item = ListItem::new(Line::from(Span::styled(" loading…", muted)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    }

    let width = body_area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(path) = state.selected_file.as_deref() {
        let path_line = Line::from(vec![
            Span::styled(" path  ", muted),
            Span::styled(path.to_string(), fg),
        ]);
        lines.extend(wrap_line(path_line, width, 7));
    }
    if let Some(meta) = state.selected_meta.as_ref() {
        let size_line = Line::from(vec![
            Span::styled(" size  ", muted),
            Span::styled(format_size(meta.size_bytes), fg),
        ]);
        lines.push(size_line);
        let mode_line = Line::from(vec![
            Span::styled(" mode  ", muted),
            Span::styled(meta.mode.clone(), fg),
        ]);
        lines.push(mode_line);
        if let Some(modified) = meta.modified.as_deref() {
            let mod_line = Line::from(vec![
                Span::styled(" mtime ", muted),
                Span::styled(modified.to_string(), fg),
            ]);
            lines.push(mod_line);
        }
    }
    lines.push(Line::from(""));

    match state.selected_kind.as_ref() {
        Some(DetailKind::Text { language, content }) if state.loading_content => {
            let _ = (language, content);
            lines.push(Line::from(Span::styled(" loading…", muted)));
        }
        Some(DetailKind::Text { language, content }) => {
            push_text_body(&mut lines, language, content, fg, accent, width);
        }
        Some(DetailKind::Binary { reason }) => {
            lines.push(Line::from(Span::styled(
                format!(" ({reason} — meta only, press o to open)"),
                muted,
            )));
        }
        Some(DetailKind::TooLarge { size_bytes }) => {
            lines.push(Line::from(Span::styled(
                format!(
                    " (file is {} — meta only, press o to open)",
                    format_size(*size_bytes)
                ),
                muted,
            )));
        }
        None => {
            if state.loading_content {
                lines.push(Line::from(Span::styled(" loading…", muted)));
            }
        }
    }

    let visible_rows = body_area.height as usize;
    state.detail_visible_rows = visible_rows;
    let max_scroll = lines.len().saturating_sub(visible_rows);
    if state.detail_scroll > max_scroll {
        state.detail_scroll = max_scroll;
    }
    let start = state.detail_scroll;
    let end = (start + visible_rows).min(lines.len());
    let view: Vec<ListItem> = lines[start..end]
        .iter()
        .map(|l| ListItem::new(l.clone()))
        .collect();
    frame.render_widget(List::new(view), body_area);

    if lines.len() > visible_rows && visible_rows > 0 {
        let mut vbar = ScrollbarState::new(lines.len().saturating_sub(visible_rows))
            .position(state.detail_scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, body_area, &mut vbar);
    }
}

fn push_text_body(
    lines: &mut Vec<Line<'static>>,
    language: &str,
    content: &str,
    body_style: Style,
    _header_style: Style,
    width: usize,
) {
    if content.is_empty() {
        lines.push(Line::from(Span::styled(" (empty)", body_style)));
        return;
    }
    let pretty_content: Option<String> = if language == "json" {
        serde_json::from_str::<serde_json::Value>(content)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
    } else {
        None
    };
    let source: &str = pretty_content.as_deref().unwrap_or(content);
    for raw in source.lines() {
        let line = Line::from(Span::styled(format!(" {raw}"), body_style));
        lines.extend(wrap_line(line, width, 1));
    }
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

fn build_tree_line(
    entry: &FlatEntry,
    selected: bool,
    focused: bool,
) -> ListItem<'static> {
    let t = theme::current();
    let mut spans: Vec<Span> = Vec::new();

    for level in 0..entry.depth {
        let is_last = entry.ancestor_is_last.get(level).copied().unwrap_or(false);
        if is_last {
            spans.push(Span::styled("   ", Style::new().fg(t.muted)));
        } else {
            spans.push(Span::styled("│  ", Style::new().fg(t.muted)));
        }
    }

    if entry.depth > 0 {
        if entry.is_last_sibling {
            spans.push(Span::styled("└─ ", Style::new().fg(t.muted)));
        } else {
            spans.push(Span::styled("├─ ", Style::new().fg(t.muted)));
        }
    }

    if entry.is_dir {
        if entry.loading {
            spans.push(Span::styled("⟳ ", Style::new().fg(t.muted)));
        } else if entry.expanded {
            spans.push(Span::styled("▾ ", Style::new().fg(t.fg)));
        } else {
            spans.push(Span::styled("▸ ", Style::new().fg(t.fg)));
        }
    }

    let name_style = if selected && focused {
        Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
    } else if selected {
        Style::new().fg(t.fg).add_modifier(Modifier::BOLD)
    } else if entry.is_dir {
        Style::new().fg(t.fg)
    } else {
        Style::new().fg(t.muted)
    };

    spans.push(Span::styled(entry.name.clone(), name_style));

    ListItem::new(Line::from(spans))
}
