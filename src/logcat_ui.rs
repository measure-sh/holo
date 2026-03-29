use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use crate::app::App;
use crate::logcat::{self, LogcatEditTarget, LogcatFilter};
use crate::panel;
use crate::theme;
use crate::ui::panel_title;

pub fn render_logcat_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    logcat_lines: &[String],
    app: &mut App,
) {
    let t = theme::current();
    let editing = app.logcat_state().editing;

    let color = panel::by_number(panel::LOGCAT).border_color(focused);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::LOGCAT, focused))
        .border_style(Style::new().fg(color));
    if focused {
        block = block.title_bottom(logcat_filter_bar(&app.logcat_state().filter, editing));
    }
    let inner = block.inner(area);

    let filter = &app.logcat_state().filter;
    let search = filter.search.clone();
    let filtered: Vec<&String> = logcat_lines
        .iter()
        .filter(|line| filter.matches(line))
        .collect();

    let visible_height = inner.height as usize;
    app.logcat_state_mut().clamp_scroll(filtered.len(), visible_height);
    let logcat_scroll = app.logcat_state().scroll;
    let end = filtered.len().saturating_sub(logcat_scroll);
    let start = end.saturating_sub(visible_height);

    if logcat_scroll > 0 {
        block = block.title_top(
            Line::from(vec![
                Span::styled(
                    format!(" ↑{} ", logcat_scroll),
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

    frame.render_widget(block, area);

    let items: Vec<ListItem> = filtered[start..end]
        .iter()
        .map(|l| ListItem::new(style_logcat_line(l, &search)))
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);

    if filtered.len() > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(filtered.len().saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(t.muted))
            .track_style(Style::new().fg(t.surface));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

fn logcat_filter_bar(filter: &LogcatFilter, editing: Option<LogcatEditTarget>) -> Line<'static> {
    let t = theme::current();
    let accent = Style::new().fg(t.danger);
    let muted = Style::new().fg(t.muted);
    let border = Style::new().fg(panel::by_number(panel::LOGCAT).border_color(true));

    let mut spans = Vec::new();

    spans.push(Span::styled(" /", accent));
    if matches!(editing, Some(LogcatEditTarget::Search)) {
        let search_text = if filter.search.is_empty() { String::new() } else { filter.search.clone() };
        spans.push(Span::styled(search_text, Style::new().fg(t.fg)));
        spans.push(Span::styled("_", Style::new().fg(t.fg)));
        spans.push(Span::styled(" ↩ ", Style::new().fg(t.danger)));
    } else if filter.search.is_empty() {
        spans.push(Span::styled("search ", muted));
    } else {
        spans.push(Span::styled(format!("{} ", filter.search), Style::new().fg(t.fg)));
    }

    spans.push(Span::styled("───", border));

    let tag_value = if filter.tag.is_empty() {
        "*".to_string()
    } else {
        filter.tag.clone()
    };
    let tag_display = match editing {
        Some(LogcatEditTarget::Tag) => tag_value.replace('*', ""),
        _ => tag_value,
    };
    spans.push(Span::styled(" t", accent));
    spans.push(Span::styled("ag:", muted));
    spans.push(Span::styled(tag_display, Style::new().fg(t.fg)));
    if matches!(editing, Some(LogcatEditTarget::Tag)) {
        spans.push(Span::styled("_", Style::new().fg(t.fg)));
        spans.push(Span::styled(" ↩ ", Style::new().fg(t.danger)));
    } else {
        spans.push(Span::styled(" ", muted));
    }

    spans.push(Span::styled("───", border));

    let level_str = match filter.level {
        Some(c) => theme::level_name(c),
        None => "All",
    };
    spans.push(Span::styled(" \u{25C2}", accent));
    spans.push(Span::styled(format!("level:{}", level_str), muted));
    spans.push(Span::styled("\u{25B8} ", accent));

    spans.push(Span::styled("───", border));

    spans.push(Span::styled(" r", accent));
    spans.push(Span::styled("eset ", muted));

    spans.push(Span::styled("───", border));

    spans.push(Span::styled(" o", accent));
    spans.push(Span::styled("pen ", muted));

    Line::from(spans)
}

fn style_logcat_line<'a>(raw: &'a str, search: &str) -> Line<'a> {
    let t = theme::current();
    let Some(parsed) = logcat::parse(raw) else {
        return Line::from(raw.replace('\t', "  "));
    };

    let level_fg = theme::level_color(parsed.level);
    let label = Span::styled(
        theme::level_label(parsed.level),
        Style::new().fg(level_fg).add_modifier(Modifier::BOLD),
    );

    let sep = Span::raw(" ");

    let timestamp = Span::styled(parsed.timestamp, Style::new().fg(t.muted));

    let tag = Span::styled(parsed.tag, Style::new().fg(t.muted));

    let msg_text = parsed.message.replace('\t', "  ");
    let msg_prefix = Span::styled(": ", Style::new().fg(t.fg));

    let mut spans = vec![label, sep.clone(), timestamp, sep, tag];
    spans.push(msg_prefix);
    let msg_style = Style::new().fg(t.fg);
    if search.is_empty() {
        spans.push(Span::styled(msg_text, msg_style));
    } else {
        let highlight_style = Style::new()
            .fg(t.bg)
            .bg(t.warning)
            .add_modifier(Modifier::UNDERLINED);
        let lower_msg = msg_text.to_lowercase();
        let lower_search = search.to_lowercase();
        let mut last_end = 0;
        for (start, _) in lower_msg.match_indices(&lower_search) {
            if start > last_end {
                spans.push(Span::styled(msg_text[last_end..start].to_string(), msg_style));
            }
            spans.push(Span::styled(
                msg_text[start..start + search.len()].to_string(),
                highlight_style,
            ));
            last_end = start + search.len();
        }
        if last_end < msg_text.len() {
            spans.push(Span::styled(msg_text[last_end..].to_string(), msg_style));
        }
        if last_end == 0 {
            spans.push(Span::styled(msg_text, msg_style));
        }
    }

    Line::from(spans)
}
