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
use crate::ui::{panel_title, wrap_line};

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
        let items = vec![
            ListItem::new(Line::from(Span::styled(
                " requires Measure SDK",
                muted,
            ))),
            ListItem::new(Line::from(Span::styled(
                " integrate measure.sh SDK to capture HTTP requests",
                muted,
            ))),
        ];
        frame.render_widget(List::new(items), inner);
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
        if state.detail_open {
            block = block.title_bottom(Line::from(vec![
                Span::styled(" o", accent),
                Span::styled("pen ", action_muted),
                Span::styled("───", Style::new().fg(color)),
                Span::styled(" esc", accent),
                Span::styled(" close ", action_muted),
            ]));
        } else {
            block = block.title_bottom(Line::from(vec![
                Span::styled(" ↩", accent),
                Span::styled(" detail ", action_muted),
                Span::styled("───", Style::new().fg(color)),
                Span::styled(" o", accent),
                Span::styled("pen ", action_muted),
                Span::styled("───", Style::new().fg(color)),
                Span::styled(" w", accent),
                Span::styled("rap ", action_muted),
            ]));
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
        render_list(frame, chunks[0], state, focused);
        if let Some(entry) = state.entries.get(state.selected) {
            render_detail(frame, chunks[1], entry);
        }
    } else {
        render_list(frame, inner, state, focused);
    }
}

fn render_list(frame: &mut Frame, area: Rect, state: &NetworkState, focused: bool) {
    let t = theme::current();
    let visible_height = area.height as usize;
    let width = area.width as usize;
    let total = state.entries.len();
    let selected = state.selected;

    let build_entry_line = |entry: &NetworkEntry, is_selected: bool| -> Line {
        let style = if is_selected {
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.fg)
        };
        let prefix = if is_selected && focused { "▸ " } else { "  " };
        let method = format!("{:<6}", entry.method.to_uppercase());
        let status = format!("{:<3}", entry.status_code);
        let latency = format!("{:>6}", format_latency(entry.latency_ms));

        Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{:<12} ", &entry.timestamp), if is_selected { style } else { Style::new().fg(t.muted) }),
            Span::styled(format!("{method} "), style),
            Span::styled(status, Style::new().fg(theme::status_color(entry.status_code))),
            Span::styled("  ", Style::new()),
            Span::styled(latency, style),
            Span::styled("  ", Style::new()),
            Span::styled(entry.url.clone(), style),
        ])
    };

    // indent(2) + timestamp(13) + method(7) + status(3) + gap(2) + latency(6) + gap(2)
    let network_pad = 2 + 13 + 7 + 3 + 2 + 6 + 2;

    if state.wrap {
        let mut display_lines: Vec<ListItem> = Vec::new();
        let mut selected_display_start = 0;
        let mut selected_display_count = 0;
        for (i, entry) in state.entries.iter().enumerate() {
            let is_selected = i == selected && focused;
            let line = build_entry_line(entry, is_selected);
            let wrapped = wrap_line(line, width, network_pad);
            if i == selected {
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

        if total_display > visible_height {
            let mut scrollbar_state =
                ScrollbarState::new(total_display.saturating_sub(visible_height)).position(start);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(t.muted))
                .track_style(Style::new().fg(t.surface));
            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    } else {
        let start = if selected >= visible_height {
            selected - visible_height + 1
        } else {
            0
        };
        let end = (start + visible_height).min(total);

        let items: Vec<ListItem> = state.entries[start..end]
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let actual = start + i;
                let is_selected = actual == selected && focused;
                ListItem::new(build_entry_line(entry, is_selected))
            })
            .collect();

        frame.render_widget(List::new(items), area);

        if total > visible_height {
            let mut scrollbar_state =
                ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(t.muted))
                .track_style(Style::new().fg(t.surface));
            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

fn render_detail(frame: &mut Frame, area: Rect, entry: &NetworkEntry) {
    let t = theme::current();
    let style = Style::new().fg(t.fg);
    let header_style = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);
    let muted = Style::new().fg(t.muted);
    let key_style = Style::new().fg(t.muted);

    let mut lines: Vec<Line> = Vec::new();

    // Method + Status + Latency
    lines.push(Line::from(vec![
        Span::styled(format!(" {} ", entry.method.to_uppercase()), header_style),
        Span::styled(
            format!("{} ", entry.status_code),
            Style::new().fg(theme::status_color(entry.status_code)),
        ),
        Span::styled(format_latency(entry.latency_ms), style),
    ]));

    // URL
    lines.push(Line::from(Span::styled(format!(" {}", &entry.url), style)));

    // Failure info
    if let Some(reason) = &entry.failure_reason {
        lines.push(Line::from(vec![
            Span::styled(" Failure: ", Style::new().fg(t.danger).add_modifier(Modifier::BOLD)),
            Span::styled(reason.as_str(), Style::new().fg(t.danger)),
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

    // Truncate to available height
    let items: Vec<ListItem> = lines
        .into_iter()
        .take(area.height as usize)
        .map(ListItem::new)
        .collect();

    // Separator line on the left edge
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.muted));

    let detail_inner = separator.inner(area);
    frame.render_widget(separator, area);
    frame.render_widget(List::new(items), detail_inner);
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
