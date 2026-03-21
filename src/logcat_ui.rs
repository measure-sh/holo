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

use crate::app::{App, InputMode};
use crate::logcat;
use crate::logcat_state::LogcatFilter;
use crate::panel;
use crate::theme;
use crate::ui::panel_title;

pub fn render_logcat_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    logcat_lines: &[String],
    monitored_pid: Option<u32>,
    app: &mut App,
) {
    let filter_tag = app.logcat_state().filter.tag.clone();
    let filter_search = app.logcat_state().filter.search.clone();
    let filter_level = app.logcat_state().filter.level;
    let input_mode = app.input_mode();

    let color = panel::by_number(panel::LOGCAT).border_color(focused);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::LOGCAT, focused))
        .title_bottom(logcat_filter_bar(&app.logcat_state().filter, input_mode, focused))
        .border_style(Style::new().fg(color));
    let inner = block.inner(area);

    let pid_str = monitored_pid.map(|p| p.to_string());

    let filtered: Vec<&String> = logcat_lines
        .iter()
        .filter(|line| {
            let Some(parsed) = logcat::parse(line) else {
                return true;
            };
            let tag_ok = filter_tag.is_empty()
                || parsed.tag.to_lowercase().contains(&filter_tag.to_lowercase());
            let search_ok = filter_search.is_empty()
                || line.to_lowercase().contains(&filter_search.to_lowercase());
            let level_ok =
                filter_level.is_none() || Some(parsed.level) == filter_level;
            tag_ok && search_ok && level_ok
        })
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
                    Style::new().fg(theme::MUTED),
                ),
                Span::styled(
                    " esc",
                    Style::new().fg(theme::KEY_HINT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" resume ", Style::new().fg(theme::MUTED)),
            ])
            .alignment(Alignment::Right),
        );
    }

    frame.render_widget(block, area);

    let items: Vec<ListItem> = filtered[start..end]
        .iter()
        .map(|l| ListItem::new(style_logcat_line(l, pid_str.as_deref())))
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);

    if filtered.len() > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(filtered.len().saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(theme::MUTED))
            .track_style(Style::new().fg(theme::SURFACE));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

fn logcat_filter_bar(filter: &LogcatFilter, input_mode: InputMode, focused: bool) -> Line<'static> {
    let accent = Style::new().fg(theme::KEY_HINT);
    let muted = Style::new().fg(theme::MUTED);
    let border = Style::new().fg(panel::by_number(panel::LOGCAT).border_color(focused));

    let mut spans = Vec::new();

    let tag_value = if filter.tag.is_empty() {
        "*".to_string()
    } else {
        filter.tag.clone()
    };
    let tag_display = match input_mode {
        InputMode::EditingTag => tag_value.replace('*', ""),
        _ => tag_value,
    };
    spans.push(Span::styled(" t", accent));
    spans.push(Span::styled("ag:", muted));
    spans.push(Span::styled(format!("{}", tag_display), Style::new().fg(theme::FG)));
    if matches!(input_mode, InputMode::EditingTag) {
        spans.push(Span::styled("_", Style::new().fg(theme::FG)));
        spans.push(Span::styled(" ↩ ", Style::new().fg(theme::RED)));
    } else {
        spans.push(Span::styled(" ", muted));
    }

    spans.push(Span::styled("───", border));

    let search_value = if filter.search.is_empty() {
        String::new()
    } else {
        filter.search.clone()
    };
    let search_display = match input_mode {
        InputMode::EditingSearch => search_value.clone(),
        _ => {
            if search_value.is_empty() {
                "*".to_string()
            } else {
                search_value
            }
        }
    };
    spans.push(Span::styled(" s", accent));
    spans.push(Span::styled("earch:", muted));
    spans.push(Span::styled(format!("{}", search_display), Style::new().fg(theme::FG)));
    if matches!(input_mode, InputMode::EditingSearch) {
        spans.push(Span::styled("_", Style::new().fg(theme::FG)));
        spans.push(Span::styled(" ↩ ", Style::new().fg(theme::RED)));
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

    Line::from(spans)
}

fn style_logcat_line<'a>(raw: &'a str, pid: Option<&str>) -> Line<'a> {
    let Some(parsed) = logcat::parse(raw) else {
        return Line::from(raw);
    };

    let level_fg = theme::level_color(parsed.level);
    let label = Span::styled(
        theme::level_label(parsed.level),
        Style::new().fg(level_fg).add_modifier(Modifier::BOLD),
    );

    let sep = Span::raw(" ");

    let timestamp = Span::styled(parsed.timestamp, Style::new().fg(theme::MUTED));

    let is_main = pid.is_some_and(|p| parsed.tid == p);
    let thread = if is_main {
        Span::styled("main", Style::new().fg(theme::MUTED))
    } else {
        Span::styled(parsed.tid, Style::new().fg(theme::MUTED))
    };

    let tag = Span::styled(parsed.tag, Style::new().fg(level_fg).add_modifier(Modifier::BOLD));

    let message = Span::styled(format!(": {}", parsed.message), Style::new().fg(theme::FG));

    Line::from(vec![label, sep.clone(), timestamp, sep.clone(), thread, sep, tag, message])
}
