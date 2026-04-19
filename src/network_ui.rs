use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::network::{NetworkEntry, NetworkState};
use crate::panel;
use crate::theme;
use crate::ui::{panel_title, render_pane_chip, split_chip, wrap_line};

pub fn format_latency(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

pub fn render_network_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &mut NetworkState,
    measure_sdk_detected: bool,
) {
    let t = theme::current();
    let color = panel::by_number(panel::NETWORK).border_color(focused);
    let muted = Style::new().fg(t.muted);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::NETWORK, focused))
        .border_style(Style::new().fg(color));

    let inner = block.inner(area);

    if !measure_sdk_detected {
        frame.render_widget(block, area);
        let item = ListItem::new(Line::from(Span::styled(
            " measure SDK is required for inspecting each network call",
            muted,
        )));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    if state.entries.is_empty() {
        frame.render_widget(block, area);
        let item = ListItem::new(Line::from(Span::styled(" no requests", muted)));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    let total = state.entries.len();
    let failures = state.failure_count;

    // Bottom bar: actions
    if focused {
        let accent = Style::new().fg(t.danger);
        let action_muted = Style::new().fg(t.muted);
        if state.detail_open && state.detail_focused {
            block = block.title_bottom(Line::from(vec![
                Span::styled(" o", accent),
                Span::styled("pen ", action_muted),
                Span::styled("───", Style::new().fg(color)),
                Span::styled(" esc", accent),
                Span::styled(" back ", action_muted),
            ]));
        } else if state.detail_open {
            block = block.title_bottom(Line::from(vec![
                Span::styled(" o", accent),
                Span::styled("pen ", action_muted),
                Span::styled("───", Style::new().fg(color)),
                Span::styled(" esc", accent),
                Span::styled(" close ", action_muted),
            ]));
        } else {
            block = block.title_bottom(network_filter_bar(state, color));
        }
    }

    // Bottom bar: stats
    let mut stats_spans = vec![
        Span::styled(format!(" {total} reqs "), Style::new().fg(t.muted)),
    ];
    if failures > 0 {
        stats_spans.push(Span::styled(format!("{failures} err "), Style::new().fg(t.danger)));
    }
    block = block.title_bottom(Line::from(stats_spans).alignment(Alignment::Right));
    frame.render_widget(block, area);

    if state.detail_open {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(inner);
        render_detail(frame, chunks[1], state, focused);
        let filtered = state.filtered_entries();
        render_list(frame, chunks[0], &filtered, state, focused, true);
    } else {
        let filtered = state.filtered_entries();
        render_list(frame, inner, &filtered, state, focused, false);
    }
}


fn render_list(frame: &mut Frame, area: Rect, filtered: &[(usize, &NetworkEntry)], state: &NetworkState, focused: bool, in_split: bool) {
    let t = theme::current();
    let list_active = focused && !state.detail_focused;

    let area = if in_split {
        let (chip_area, body) = split_chip(area);
        render_pane_chip(frame, chip_area, "requests", list_active, focused && !list_active);
        body
    } else {
        area
    };
    if area.width == 0 || area.height == 0 {
        return;
    }

    let visible_height = area.height as usize;
    let width = area.width as usize;
    let total = filtered.len();
    let selected = state.selected;
    let search = &state.search;

    let build_entry_line = |entry: &NetworkEntry, is_selected: bool| -> Line {
        let style = if is_selected && list_active {
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::new().fg(t.muted).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.fg)
        };
        let prefix = if is_selected && list_active { "▸ " } else { "  " };
        let method = format!("{:<6}", entry.method.to_uppercase());
        let status = format!("{:<3}", entry.status_code);
        let latency = format!("{:>6}", format_latency(entry.latency_ms));

        let highlight_style = Style::new()
            .fg(t.bg)
            .bg(t.warning)
            .add_modifier(Modifier::UNDERLINED);
        let lower_search = search.to_lowercase();

        let status_style = Style::new().fg(theme::status_color(entry.status_code));

        let mut spans = vec![
            Span::styled(prefix, style),
            Span::styled(format!("{:<12} ", &entry.timestamp), if is_selected { style } else { Style::new().fg(t.muted) }),
        ];

        highlight_spans(&mut spans, &format!("{method} "), style, highlight_style, &lower_search);
        highlight_spans(&mut spans, &status, status_style, highlight_style, &lower_search);
        spans.push(Span::styled("  ", Style::new()));
        spans.push(Span::styled(latency, style));
        spans.push(Span::styled("  ", Style::new()));
        highlight_spans(&mut spans, &entry.url, style, highlight_style, &lower_search);

        Line::from(spans)
    };

    // indent(2) + timestamp(13) + method(7) + status(3) + gap(2) + latency(6) + gap(2)
    let network_pad = 2 + 13 + 7 + 3 + 2 + 6 + 2;

    if state.wrap {
        let mut display_lines: Vec<ListItem> = Vec::new();
        let mut selected_display_start = 0;
        let mut selected_display_count = 0;
        for (_, (orig_idx, entry)) in filtered.iter().enumerate() {
            let is_selected = *orig_idx == selected && focused;
            let line = build_entry_line(entry, is_selected);
            let wrapped = wrap_line(line, width, network_pad);
            if *orig_idx == selected {
                selected_display_start = display_lines.len();
                selected_display_count = wrapped.len();
            }
            for w in wrapped {
                display_lines.push(ListItem::new(w));
            }
        }
        let total_display = display_lines.len();
        let start = if selected_display_start + selected_display_count > visible_height {
            (selected_display_start + selected_display_count).saturating_sub(visible_height)
        } else {
            0
        };
        let end = (start + visible_height).min(total_display);
        let items: Vec<ListItem> = display_lines.into_iter().skip(start).take(end - start).collect();
        frame.render_widget(List::new(items), area);

        if total_display > visible_height && area.height > 0 && area.width > 0 {
            let mut scrollbar_state =
                ScrollbarState::new(total_display.saturating_sub(visible_height)).position(start);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(t.muted))
                .track_style(Style::new().fg(t.surface));
            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    } else {
        // Find the position of the selected entry in the filtered list
        let selected_pos = filtered.iter().position(|(idx, _)| *idx == selected).unwrap_or(0);
        let start = if selected_pos >= visible_height {
            selected_pos - visible_height + 1
        } else {
            0
        };
        let end = (start + visible_height).min(total);

        let items: Vec<ListItem> = filtered[start..end]
            .iter()
            .map(|(orig_idx, entry)| {
                let is_selected = *orig_idx == selected && focused;
                ListItem::new(build_entry_line(entry, is_selected))
            })
            .collect();

        frame.render_widget(List::new(items), area);

        if total > visible_height && area.height > 0 && area.width > 0 {
            let mut scrollbar_state =
                ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(t.muted))
                .track_style(Style::new().fg(t.surface));
            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

fn highlight_spans(spans: &mut Vec<Span<'_>>, text: &str, base: Style, highlight: Style, lower_search: &str) {
    if lower_search.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
        return;
    }
    let lower_text = text.to_lowercase();
    let mut last_end = 0;
    for (start, _) in lower_text.match_indices(lower_search) {
        if start > last_end {
            spans.push(Span::styled(text[last_end..start].to_string(), base));
        }
        spans.push(Span::styled(
            text[start..start + lower_search.len()].to_string(),
            highlight,
        ));
        last_end = start + lower_search.len();
    }
    if last_end < text.len() {
        spans.push(Span::styled(text[last_end..].to_string(), base));
    }
    if last_end == 0 {
        spans.push(Span::styled(text.to_string(), base));
    }
}

fn render_detail(frame: &mut Frame, area: Rect, state: &mut NetworkState, focused: bool) {
    let t = theme::current();
    let detail_active = focused && state.detail_focused;

    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.muted));
    let content_area = separator.inner(area);
    frame.render_widget(separator, area);
    if content_area.width == 0 || content_area.height == 0 {
        return;
    }
    let (chip_area, detail_inner) = split_chip(content_area);

    let entry = match state.entries.get(state.selected) {
        Some(e) => e,
        None => {
            render_pane_chip(frame, chip_area, "detail", detail_active, focused && !detail_active);
            return;
        }
    };

    let method_label = entry.method.to_uppercase();
    render_pane_chip(frame, chip_area, &method_label, detail_active, focused && !detail_active);
    let status_latency = Line::from(vec![
        Span::styled(
            format!("{} ", entry.status_code),
            Style::new().fg(theme::status_color(entry.status_code)),
        ),
        Span::styled(format!("{} ", format_latency(entry.latency_ms)), Style::new().fg(t.muted)),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(status_latency, chip_area);

    let style = Style::new().fg(t.fg);
    let header_style = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);
    let muted = Style::new().fg(t.muted);
    let key_style = Style::new().fg(t.muted);

    let mut lines: Vec<Line> = Vec::new();

    // URL
    lines.push(Line::from(Span::styled(format!(" {}", &entry.url), style)));

    // Failure info
    if let Some(reason) = &entry.failure_reason {
        lines.push(Line::from(vec![
            Span::styled(" Failure: ", Style::new().fg(t.danger).add_modifier(Modifier::BOLD)),
            Span::styled(reason.clone(), Style::new().fg(t.danger)),
        ]));
    }
    if let Some(desc) = &entry.failure_description {
        lines.push(Line::from(Span::styled(format!(" {desc}"), Style::new().fg(t.danger))));
    }

    lines.push(Line::from(""));

    // Request Headers
    lines.push(Line::from(Span::styled(" Request Headers", header_style)));
    push_header_lines(&mut lines, &entry.request_headers, key_style, style);

    lines.push(Line::from(""));

    // Response Headers
    lines.push(Line::from(Span::styled(" Response Headers", header_style)));
    push_header_lines(&mut lines, &entry.response_headers, key_style, style);

    lines.push(Line::from(""));

    // Request Body
    lines.push(Line::from(Span::styled(" Request Body", header_style)));
    push_body_lines(&mut lines, &entry.request_body, style, muted);

    lines.push(Line::from(""));

    // Response Body
    lines.push(Line::from(Span::styled(" Response Body", header_style)));
    push_body_lines(&mut lines, &entry.response_body, style, muted);

    let visible_height = detail_inner.height as usize;
    let total = lines.len();

    state.clamp_detail_scroll(total, visible_height);

    let items: Vec<ListItem> = lines
        .into_iter()
        .skip(state.detail_scroll)
        .take(visible_height)
        .map(ListItem::new)
        .collect();

    frame.render_widget(List::new(items), detail_inner);

    if total > visible_height && detail_inner.height > 0 && detail_inner.width > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(state.detail_scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, detail_inner, &mut scrollbar_state);
    }
}

/// Parse headers from the `{key1=value1, key2=value2}` map format
/// and render each as `key: value` on its own line.
fn push_header_lines<'a>(lines: &mut Vec<Line<'a>>, raw: &str, key_style: Style, value_style: Style) {
    if is_empty_value(raw) {
        lines.push(Line::from(Span::styled("   (empty)", key_style)));
        return;
    }
    // Strip outer braces: {key=val, key=val} -> key=val, key=val
    let inner = raw.trim().strip_prefix('{').unwrap_or(raw);
    let inner = inner.strip_suffix('}').unwrap_or(inner).trim();
    if inner.is_empty() {
        lines.push(Line::from(Span::styled("   (empty)", key_style)));
        return;
    }
    for pair in split_header_pairs(inner) {
        if let Some((k, v)) = pair.split_once('=') {
            lines.push(Line::from(vec![
                Span::styled(format!("   {k}"), key_style),
                Span::styled(": ", key_style),
                Span::styled(v.to_string(), value_style),
            ]));
        } else {
            lines.push(Line::from(Span::styled(format!("   {pair}"), value_style)));
        }
    }
}

/// Try to pretty-print as JSON, otherwise show raw text.
fn push_body_lines<'a>(lines: &mut Vec<Line<'a>>, raw: &str, style: Style, muted: Style) {
    if is_empty_value(raw) {
        lines.push(Line::from(Span::styled("   (empty)", muted)));
        return;
    }
    // Try JSON pretty-print
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Ok(pretty) = serde_json::to_string_pretty(&json) {
            for line in pretty.lines() {
                lines.push(Line::from(Span::styled(format!("   {line}"), style)));
            }
            return;
        }
    }
    // Fallback: raw text
    for line in raw.lines() {
        lines.push(Line::from(Span::styled(format!("   {line}"), style)));
    }
}

fn network_filter_bar(state: &NetworkState, color: ratatui::style::Color) -> Line<'static> {
    let t = theme::current();
    let accent = Style::new().fg(t.danger);
    let muted = Style::new().fg(t.muted);
    let border = Style::new().fg(color);

    let mut spans = Vec::new();

    spans.push(Span::styled(" /", accent));
    if state.editing_search {
        let search_text = if state.search.is_empty() { String::new() } else { state.search.clone() };
        spans.push(Span::styled(search_text, Style::new().fg(t.fg)));
        spans.push(Span::styled("_", Style::new().fg(t.fg)));
        spans.push(Span::styled(" ↩ ", Style::new().fg(t.danger)));
    } else if state.search.is_empty() {
        spans.push(Span::styled("search ", muted));
    } else {
        spans.push(Span::styled(format!("{} ", state.search), Style::new().fg(t.fg)));
    }

    spans.push(Span::styled("───", border));

    spans.push(Span::styled(" ↩", accent));
    spans.push(Span::styled(" detail ", muted));

    spans.push(Span::styled("───", border));

    spans.push(Span::styled(" o", accent));
    spans.push(Span::styled("pen ", muted));

    spans.push(Span::styled("───", border));

    spans.push(Span::styled(" w", accent));
    spans.push(Span::styled("rap ", muted));

    Line::from(spans)
}

fn is_empty_value(value: &str) -> bool {
    value.is_empty() || value == "null" || value == "{}"
}

/// Split header pairs on `, ` but only when the next segment contains `=`,
/// so values like `Wed, 15 Apr 2026 09:22:25 GMT` stay intact.
fn split_header_pairs(input: &str) -> Vec<&str> {
    let mut pairs = Vec::new();
    let mut start = 0;
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if i + 2 <= len && &input[i..i + 2] == ", " {
            let rest = &input[i + 2..];
            if rest.contains('=') && rest.find('=') < rest.find(", ") {
                pairs.push(input[start..i].trim());
                start = i + 2;
            }
        }
        i += 1;
    }
    let last = input[start..].trim();
    if !last.is_empty() {
        pairs.push(last);
    }
    pairs
}
