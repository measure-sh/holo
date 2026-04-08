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
use crate::ui::panel_title;

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
    let total = state.entries.len();
    state.clamp_scroll(total, visible_height);

    if state.scroll > 0 {
        block = block.title_top(
            Line::from(vec![
                Span::styled(
                    format!(" ↑{} ", state.scroll),
                    Style::new().fg(t.muted),
                ),
                Span::styled(
                    " esc",
                    Style::new().fg(t.danger).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" resume ", Style::new().fg(t.muted)),
            ])
            .alignment(Alignment::Right),
        );
    }

    let failures = state.failure_count;
    let mut stats_spans = vec![
        Span::styled(format!(" {total} reqs "), Style::new().fg(t.muted)),
    ];
    if failures > 0 {
        stats_spans.push(Span::styled(format!("{failures} err "), Style::new().fg(t.danger)));
    }
    block = block.title_bottom(Line::from(stats_spans).alignment(Alignment::Right));

    frame.render_widget(block, area);

    // Display newest first (entries are stored oldest-first, reverse for display)
    let reversed: Vec<&_> = state.entries.iter().rev().collect();

    let end = total.saturating_sub(state.scroll);
    let start = end.saturating_sub(visible_height);

    let items: Vec<ListItem> = reversed[start..end]
        .iter()
        .enumerate()
        .map(|(_, entry)| {
            let style = Style::new().fg(t.fg);
            let prefix = "  ";
            let method = format!("{:<6}", entry.method.to_uppercase());
            let status = format!("{:<3}", entry.status_code);
            let latency = format!("{:>6}", format_latency(entry.latency_ms));
            let fixed_cols = 37usize;
            let url_width = (inner.width as usize).saturating_sub(fixed_cols);

            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{:<12} ", &entry.timestamp), Style::new().fg(t.muted)),
                Span::styled(format!("{method} "), style),
                Span::styled(status, Style::new().fg(status_color(entry.status_code))),
                Span::styled("  ", Style::new()),
                Span::styled(latency, style),
                Span::styled("  ", Style::new()),
                Span::styled(truncate_url(&entry.url, url_width), style),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);

    if total > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

fn truncate_url(url: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if url.len() <= max_width {
        url.to_string()
    } else if max_width > 3 {
        format!("{}...", &url[..max_width - 3])
    } else {
        url[..max_width].to_string()
    }
}
