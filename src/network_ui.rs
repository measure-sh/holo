use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::network::NetworkState;
use crate::panel;
use crate::theme;
use crate::ui::{panel_title, wrap_line};

fn format_latency(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn status_color(code: u16) -> ratatui::style::Color {
    let t = theme::current();
    match code {
        200..=299 => t.info,
        300..=399 => t.secondary,
        400..=499 => t.warning,
        _ => t.danger,
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

    let visible_height = inner.height as usize;
    let width = inner.width as usize;
    let total = state.entries.len();
    let failures = state.failure_count;

    let build_entry_line = |entry: &crate::network::NetworkEntry| -> Line {
        let style = Style::new().fg(t.fg);
        let prefix = "  ";
        let method = format!("{:<6}", entry.method.to_uppercase());
        let status = format!("{:<3}", entry.status_code);
        let latency = format!("{:>6}", format_latency(entry.latency_ms));

        Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{:<12} ", &entry.timestamp), Style::new().fg(t.muted)),
            Span::styled(format!("{method} "), style),
            Span::styled(status, Style::new().fg(status_color(entry.status_code))),
            Span::styled("  ", Style::new()),
            Span::styled(latency, style),
            Span::styled("  ", Style::new()),
            Span::styled(entry.url.clone(), style),
        ])
    };

    // Build display lines in chronological order (oldest first)
    let entry_lines: Vec<Line> = state.entries.iter().map(build_entry_line).collect();

    let display_lines: Vec<ListItem> = if state.wrap {
        entry_lines
            .into_iter()
            .flat_map(|line| wrap_line(line, width).into_iter().map(ListItem::new))
            .collect()
    } else {
        entry_lines.into_iter().map(ListItem::new).collect()
    };

    let total_display = display_lines.len();
    state.clamp_scroll(total_display, visible_height);

    if state.scroll > 0 {
        block = block.title_top(
            Line::from(vec![
                Span::styled(format!(" ↑{} ", state.scroll), Style::new().fg(t.muted)),
                Span::styled(" esc", Style::new().fg(t.danger).add_modifier(Modifier::BOLD)),
                Span::styled(" resume ", Style::new().fg(t.muted)),
            ])
            .alignment(Alignment::Right),
        );
    }

    let mut stats_spans = vec![
        Span::styled(format!(" {total} reqs "), Style::new().fg(t.muted)),
    ];
    if failures > 0 {
        stats_spans.push(Span::styled(format!("{failures} err "), Style::new().fg(t.danger)));
    }
    if state.wrap {
        stats_spans.push(Span::styled("wrap ", Style::new().fg(t.secondary)));
    }
    block = block.title_bottom(Line::from(stats_spans).alignment(Alignment::Right));
    frame.render_widget(block, area);

    let end = total_display.saturating_sub(state.scroll);
    let start = end.saturating_sub(visible_height);
    let items: Vec<ListItem> = display_lines.into_iter().skip(start).take(end - start).collect();
    frame.render_widget(List::new(items), inner);

    if total_display > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(total_display.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}
