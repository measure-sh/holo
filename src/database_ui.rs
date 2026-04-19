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

use crate::database::{DatabaseState, ReplLine, TreeNode};
use crate::panel;
use crate::theme;
use crate::ui::{panel_title, render_pane_chip, split_chip};

pub fn render_database_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    db_state: &mut DatabaseState,
) {
    let t = theme::current();
    let color = panel::by_number(panel::DATABASE).border_color(focused);
    let muted = Style::new().fg(t.muted);

    if let Some(ref err) = db_state.error {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(panel_title(panel::DATABASE, focused))
            .border_style(Style::new().fg(color));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let item = ListItem::new(Line::from(Span::styled(err.as_str(), Style::new().fg(t.danger))));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    let title = build_title(db_state, focused);
    let bottom = build_bottom_bar(db_state, focused, color);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(Style::new().fg(color));
    if !bottom.spans.is_empty() {
        block = block.title_bottom(bottom);
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let tree_focused = focused && !db_state.detail_focused && !db_state.repl_active;
    let detail_active = focused && db_state.detail_focused;
    let tree_hint = focused && !tree_focused;
    let detail_hint = focused && !detail_active;

    if db_state.detail_open {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(inner);
        render_tree(frame, chunks[0], db_state, tree_focused, true, tree_hint);
        if db_state.repl_active {
            render_repl(frame, chunks[1], db_state, focused, true);
        } else {
            render_detail(frame, chunks[1], db_state, detail_active, detail_hint);
        }
    } else if db_state.repl_active {
        render_repl(frame, inner, db_state, focused, false);
    } else {
        render_tree(frame, inner, db_state, tree_focused, false, false);
    }

    if db_state.copied_at
        .is_some_and(|ts| ts.elapsed() >= std::time::Duration::from_secs(2))
    {
        db_state.copied_at = None;
    }
    let _ = muted;
}

fn build_title(state: &DatabaseState, focused: bool) -> Line<'static> {
    let t = theme::current();
    let base = panel_title(panel::DATABASE, focused);
    if state.repl_active
        && let Some(db) = &state.repl_db
    {
        let mut spans = base.spans;
        spans.push(Span::styled(format!("› {} ", db), Style::new().fg(t.muted)));
        return Line::from(spans);
    }
    base
}

fn build_bottom_bar(
    state: &DatabaseState,
    focused: bool,
    color: ratatui::style::Color,
) -> Line<'static> {
    let t = theme::current();
    let accent = Style::new().fg(t.danger);
    let muted = Style::new().fg(t.muted);
    let sep = Style::new().fg(color);

    if !focused {
        return Line::default();
    }

    if let Some(db) = &state.confirming_pull {
        return Line::from(vec![
            Span::styled(" p", accent),
            Span::styled(format!(" to pull {} ", db), Style::new().fg(t.fg)),
        ]);
    }

    if state.repl_active {
        if state.editing_query {
            return Line::from(vec![
                Span::styled(" ↩", accent),
                Span::styled(" run ", muted),
                Span::styled("───", sep),
                Span::styled(" esc", accent),
                Span::styled(" cancel ", muted),
            ]);
        }
        let copied = state.copied_at
            .is_some_and(|ts| ts.elapsed() < std::time::Duration::from_secs(2));
        let copy_spans: Vec<Span> = if copied {
            vec![Span::styled(" copied! ", Style::new().fg(t.success))]
        } else {
            vec![Span::styled(" c", accent), Span::styled("opy ", muted)]
        };
        let mut spans = vec![
            Span::styled(" e", accent),
            Span::styled("dit ", muted),
            Span::styled("───", sep),
        ];
        spans.extend(copy_spans);
        spans.extend([
            Span::styled("───", sep),
            Span::styled(" esc", accent),
            Span::styled(" back ", muted),
        ]);
        return Line::from(spans);
    }

    if state.detail_open && state.detail_focused {
        return Line::from(vec![
            Span::styled(" hjkl", accent),
            Span::styled(" scroll ", muted),
            Span::styled("───", sep),
            Span::styled(" esc", accent),
            Span::styled(" close ", muted),
        ]);
    }

    if state.detail_open {
        return Line::from(vec![
            Span::styled(" /", accent),
            Span::styled("repl ", muted),
            Span::styled("───", sep),
            Span::styled(" p", accent),
            Span::styled("ull ", muted),
            Span::styled("───", sep),
            Span::styled(" r", accent),
            Span::styled("efresh ", muted),
        ]);
    }

    Line::from(vec![
        Span::styled(" ↩", accent),
        Span::styled(" open ", muted),
        Span::styled("───", sep),
        Span::styled(" /", accent),
        Span::styled("repl ", muted),
        Span::styled("───", sep),
        Span::styled(" p", accent),
        Span::styled("ull ", muted),
        Span::styled("───", sep),
        Span::styled(" r", accent),
        Span::styled("efresh ", muted),
    ])
}

fn render_tree(frame: &mut Frame, area: Rect, state: &mut DatabaseState, active: bool, in_split: bool, show_hint: bool) {
    let t = theme::current();
    let muted = Style::new().fg(t.muted);
    let fg = Style::new().fg(t.fg);

    let body_area = if in_split {
        let (chip_area, body) = split_chip(area);
        render_pane_chip(frame, chip_area, "tables", active, show_hint);
        body
    } else {
        area
    };
    if body_area.width == 0 || body_area.height == 0 {
        return;
    }

    if state.databases.is_empty() {
        let item = ListItem::new(Line::from(Span::styled("detecting databases…", muted)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    }

    let nodes = state.flatten_tree();
    let total = nodes.len();
    if state.tree_cursor >= total && total > 0 {
        state.tree_cursor = total - 1;
    }

    let visible_height = body_area.height as usize;
    let start = if state.tree_cursor >= visible_height {
        state.tree_cursor - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let selected_style = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);
    let selected_inactive = Style::new().fg(t.muted).add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = nodes[start..end]
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let idx = start + i;
            let is_selected = idx == state.tree_cursor;
            let highlight = if is_selected && active {
                selected_style
            } else if is_selected {
                selected_inactive
            } else {
                fg
            };
            let line = match node {
                TreeNode::Database { name, expanded } => {
                    let marker = if *expanded { "▾ " } else { "▸ " };
                    Line::from(vec![
                        Span::styled(marker, highlight),
                        Span::styled(name.clone(), highlight),
                    ])
                }
                TreeNode::Table { name, .. } => {
                    let prefix = if is_selected && active { "  ▸ " } else { "    " };
                    Line::from(Span::styled(format!("{prefix}{name}"), highlight))
                }
                TreeNode::Loading { .. } => {
                    Line::from(Span::styled("    loading…", muted))
                }
                TreeNode::NoTables { .. } => {
                    Line::from(Span::styled("    (no tables)", muted))
                }
            };
            ListItem::new(line)
        })
        .collect();

    frame.render_widget(List::new(items), body_area);

    if total > visible_height && body_area.height > 0 && body_area.width > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, body_area, &mut scrollbar_state);
    }
}

fn render_detail(frame: &mut Frame, area: Rect, state: &mut DatabaseState, active: bool, show_hint: bool) {
    let t = theme::current();
    let fg = Style::new().fg(t.fg);
    let muted = Style::new().fg(t.muted);
    let header_style = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);
    let danger = Style::new().fg(t.danger);

    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.muted));
    let detail_inner = separator.inner(area);
    frame.render_widget(separator, area);
    if detail_inner.width == 0 || detail_inner.height == 0 {
        return;
    }

    let Some((_, table)) = state.selected_table.clone() else {
        let (chip_area, body_area) = split_chip(detail_inner);
        render_pane_chip(frame, chip_area, "detail", active, show_hint);
        let item = ListItem::new(Line::from(Span::styled(" select a table", muted)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    };

    let (chip_area, rest) = split_chip(detail_inner);
    render_pane_chip(frame, chip_area, &format!("{table} table"), active, show_hint);
    let stats_text = state
        .table_data
        .as_ref()
        .map(|d| format!("{}/{} rows ", d.rows.len(), d.row_count))
        .unwrap_or_else(|| "loading… ".to_string());
    let stats_line = Line::from(Span::styled(stats_text, muted)).alignment(Alignment::Right);
    frame.render_widget(stats_line, chip_area);

    let body_area = Rect {
        x: rest.x,
        y: rest.y + 1,
        width: rest.width,
        height: rest.height.saturating_sub(1),
    };
    if body_area.height == 0 {
        return;
    }

    if let Some(err) = &state.table_error {
        let item = ListItem::new(Line::from(Span::styled(format!(" {err}"), danger)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    }

    let Some(data) = state.table_data.as_ref() else {
        let item = ListItem::new(Line::from(Span::styled(" loading…", muted)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    };

    if data.row_count == 0 {
        let item = ListItem::new(Line::from(Span::styled(" (empty table)", muted)));
        frame.render_widget(List::new(vec![item]), body_area);
        return;
    }

    let col_widths = compute_col_widths(&data.columns, &data.rows);
    let gutter = 2;
    let rownum_width = data.row_count.max(1).to_string().chars().count();
    let rownum_prefix_width = rownum_width + gutter;
    let data_view_width = (body_area.width as usize).saturating_sub(rownum_prefix_width);
    let total_width: usize = col_widths.iter().sum::<usize>()
        + col_widths.len().saturating_sub(1) * gutter;
    let max_h_scroll = total_width.saturating_sub(data_view_width);
    if state.detail_h_scroll > max_h_scroll {
        state.detail_h_scroll = max_h_scroll;
    }
    let h_off = state.detail_h_scroll;

    let header_line = format_row(&data.columns, &col_widths, gutter);
    let row_lines: Vec<String> = data
        .rows
        .iter()
        .map(|r| format_row(r, &col_widths, gutter))
        .collect();

    // Pinned header above data rows. Row-number column uses "#".
    let header_area = Rect {
        x: body_area.x,
        y: body_area.y,
        width: body_area.width,
        height: 1,
    };
    let header_prefix = format!("{:>w$}  ", "#", w = rownum_width);
    frame.render_widget(
        Line::from(vec![
            Span::styled(header_prefix, muted),
            Span::styled(
                slice_line(&header_line, h_off, data_view_width),
                header_style,
            ),
        ]),
        header_area,
    );

    let rows_area = Rect {
        x: body_area.x,
        y: body_area.y + 1,
        width: body_area.width,
        height: body_area.height.saturating_sub(1),
    };
    if rows_area.height == 0 {
        return;
    }

    let need_hbar = max_h_scroll > 0;
    let data_rows_visible =
        (rows_area.height as usize).saturating_sub(if need_hbar { 1 } else { 0 });
    state.detail_visible_rows = data_rows_visible;
    let max_v_scroll = row_lines.len().saturating_sub(data_rows_visible);
    if state.detail_scroll > max_v_scroll {
        state.detail_scroll = max_v_scroll;
    }

    let start = state.detail_scroll;
    let end = (start + data_rows_visible).min(row_lines.len());
    let items: Vec<ListItem> = row_lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let rownum = data.offset + start + i + 1;
            let prefix = format!("{:>w$}  ", rownum, w = rownum_width);
            ListItem::new(Line::from(vec![
                Span::styled(prefix, muted),
                Span::styled(slice_line(line, h_off, data_view_width), fg),
            ]))
        })
        .collect();
    let data_area = Rect {
        x: rows_area.x,
        y: rows_area.y,
        width: rows_area.width,
        height: data_rows_visible as u16,
    };
    frame.render_widget(List::new(items), data_area);

    if row_lines.len() > data_rows_visible && data_rows_visible > 0 {
        let mut vbar = ScrollbarState::new(row_lines.len().saturating_sub(data_rows_visible))
            .position(state.detail_scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, data_area, &mut vbar);
    }

    if need_hbar {
        let hbar_area = Rect {
            x: body_area.x,
            y: body_area.y + body_area.height.saturating_sub(1),
            width: body_area.width,
            height: 1,
        };
        let mut hbar = ScrollbarState::new(max_h_scroll).position(h_off);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, hbar_area, &mut hbar);
    }
}

fn format_row(cells: &[String], widths: &[usize], gutter: usize) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            for _ in 0..gutter {
                out.push(' ');
            }
        }
        let w = widths.get(i).copied().unwrap_or(0);
        let pad = w.saturating_sub(cell.chars().count());
        out.push_str(cell);
        for _ in 0..pad {
            out.push(' ');
        }
    }
    out
}

fn slice_line(line: &str, offset: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = line.chars().collect();
    let end = (offset + width).min(chars.len());
    let start = offset.min(chars.len());
    chars[start..end].iter().collect()
}

fn compute_col_widths(columns: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = columns.iter().map(|c| c.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }
    widths
}

fn render_repl(frame: &mut Frame, area: Rect, state: &mut DatabaseState, focused: bool, in_split: bool) {
    let t = theme::current();
    let muted = Style::new().fg(t.muted);
    let selected = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);
    let fg = Style::new().fg(t.fg);
    let danger = Style::new().fg(t.danger);

    let content_area = if in_split {
        let separator = Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(t.muted));
        let inner = separator.inner(area);
        frame.render_widget(separator, area);
        inner
    } else {
        area
    };
    if content_area.width == 0 || content_area.height == 0 {
        return;
    }

    let repl_inner = if in_split {
        let (chip_area, body) = split_chip(content_area);
        let chip_label = state.repl_db.as_deref().unwrap_or("repl");
        render_pane_chip(frame, chip_area, chip_label, focused, false);
        body
    } else {
        content_area
    };

    if repl_inner.height == 0 {
        return;
    }

    let prompt_area = Rect {
        x: repl_inner.x,
        y: repl_inner.y + repl_inner.height.saturating_sub(1),
        width: 2.min(repl_inner.width),
        height: 1.min(repl_inner.height),
    };
    let textarea_area = Rect {
        x: repl_inner.x + 2.min(repl_inner.width),
        y: repl_inner.y + repl_inner.height.saturating_sub(1),
        width: repl_inner.width.saturating_sub(2),
        height: 1.min(repl_inner.height),
    };
    let history_area = Rect {
        x: repl_inner.x,
        y: repl_inner.y,
        width: repl_inner.width,
        height: repl_inner.height.saturating_sub(1),
    };

    if state.editing_query {
        frame.render_widget(Line::from(Span::styled("> ", selected)), prompt_area);
        state.textarea.set_style(selected);
        state.textarea.set_cursor_style(Style::new().fg(t.bg).bg(t.fg));
        frame.render_widget(&state.textarea, textarea_area);
    } else if state.history.is_empty() {
        let full = Rect {
            x: repl_inner.x,
            y: prompt_area.y,
            width: repl_inner.width,
            height: 1.min(repl_inner.height),
        };
        frame.render_widget(Line::from(Span::styled("press e to enter a query", muted)), full);
    } else {
        frame.render_widget(Line::from(Span::styled("> ", muted)), prompt_area);
    }

    if !state.history.is_empty() {
        let visible_height = history_area.height as usize;
        let total = state.history.len();
        state.clamp_repl_scroll(total, visible_height);
        let end = total.saturating_sub(state.repl_scroll);
        let start = end.saturating_sub(visible_height);
        let items: Vec<ListItem> = state.history[start..end]
            .iter()
            .map(|line| match line {
                ReplLine::Input(s) => {
                    ListItem::new(Line::from(Span::styled(format!("> {s}"), selected)))
                }
                ReplLine::Output(s) => ListItem::new(Line::from(Span::styled(s.as_str(), fg))),
                ReplLine::Error(s) => ListItem::new(Line::from(Span::styled(s.as_str(), danger))),
            })
            .collect();
        frame.render_widget(List::new(items), history_area);
        if total > visible_height && history_area.height > 0 && history_area.width > 0 {
            let mut scrollbar_state =
                ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(t.muted))
                .track_style(Style::new().fg(t.surface));
            frame.render_stateful_widget(scrollbar, history_area, &mut scrollbar_state);
        }
    }
}
