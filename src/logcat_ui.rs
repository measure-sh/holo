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
    app: &mut App,
) {
    let input_mode = app.input_mode();

    let copied_active = app.logcat_state().copied_at
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1));
    if !copied_active {
        app.logcat_state_mut().copied_at = None;
    }

    let color = panel::by_number(panel::LOGCAT).border_color(focused);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::LOGCAT, focused))
        .border_style(Style::new().fg(color));
    if focused {
        block = block.title_bottom(logcat_filter_bar(&app.logcat_state().filter, input_mode, copied_active));
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
        .map(|l| ListItem::new(style_logcat_line(l, &search)))
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

fn logcat_filter_bar(filter: &LogcatFilter, input_mode: InputMode, copied_active: bool) -> Line<'static> {
    let accent = Style::new().fg(theme::KEY_HINT);
    let muted = Style::new().fg(theme::MUTED);
    let border = Style::new().fg(panel::by_number(panel::LOGCAT).border_color(true));

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

    spans.push(Span::styled("───", border));

    if copied_active {
        spans.push(Span::styled(" copied! ", Style::new().fg(theme::GREEN)));
    } else {
        spans.push(Span::styled(" c", accent));
        spans.push(Span::styled("opy ", muted));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Style {
        Style::new().fg(theme::FG)
    }

    #[test]
    fn highlight_empty_search_returns_single_span() {
        let spans = highlight_spans("hello world", "", base());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
    }

    #[test]
    fn highlight_no_match_returns_single_span() {
        let spans = highlight_spans("hello world", "xyz", base());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
    }

    #[test]
    fn highlight_match_at_start() {
        let spans = highlight_spans("hello world", "hel", base());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "hel");
        assert_eq!(spans[1].content, "lo world");
    }

    #[test]
    fn highlight_match_at_end() {
        let spans = highlight_spans("hello world", "rld", base());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "hello wo");
        assert_eq!(spans[1].content, "rld");
    }

    #[test]
    fn highlight_match_in_middle() {
        let spans = highlight_spans("hello world", "lo wo", base());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "hel");
        assert_eq!(spans[1].content, "lo wo");
        assert_eq!(spans[2].content, "rld");
    }

    #[test]
    fn highlight_multiple_matches() {
        let spans = highlight_spans("abcabc", "abc", base());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "abc");
        assert_eq!(spans[1].content, "abc");
    }

    #[test]
    fn highlight_case_insensitive() {
        let spans = highlight_spans("Hello World", "hello", base());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "Hello");
        assert_eq!(spans[1].content, " World");
    }
}

fn highlight_spans<'a>(text: &'a str, search: &str, base_style: Style) -> Vec<Span<'a>> {
    if search.is_empty() {
        return vec![Span::styled(text, base_style)];
    }

    let highlight_style = Style::new()
        .fg(theme::BG)
        .bg(theme::YELLOW)
        .add_modifier(Modifier::UNDERLINED);

    let lower_text = text.to_lowercase();
    let lower_search = search.to_lowercase();
    let mut spans = Vec::new();
    let mut last_end = 0;

    for (start, _) in lower_text.match_indices(&lower_search) {
        if start > last_end {
            spans.push(Span::styled(&text[last_end..start], base_style));
        }
        spans.push(Span::styled(
            &text[start..start + search.len()],
            highlight_style,
        ));
        last_end = start + search.len();
    }

    if last_end < text.len() {
        spans.push(Span::styled(&text[last_end..], base_style));
    }

    if spans.is_empty() {
        vec![Span::styled(text, base_style)]
    } else {
        spans
    }
}

fn style_logcat_line<'a>(raw: &'a str, search: &str) -> Line<'a> {
    let Some(parsed) = logcat::parse(raw) else {
        return Line::from(raw.replace('\t', "  "));
    };

    let level_fg = theme::level_color(parsed.level);
    let label = Span::styled(
        theme::level_label(parsed.level),
        Style::new().fg(level_fg).add_modifier(Modifier::BOLD),
    );

    let sep = Span::raw(" ");

    let timestamp = Span::styled(parsed.timestamp, Style::new().fg(theme::MUTED));

    let tag_style = Style::new().fg(level_fg).add_modifier(Modifier::BOLD);
    let tag_spans = highlight_spans(parsed.tag, search, tag_style);

    let msg_text = parsed.message.replace('\t', "  ");
    let msg_prefix = Span::styled(": ", Style::new().fg(theme::FG));

    let mut spans = vec![label, sep.clone(), timestamp, sep];
    spans.extend(tag_spans);
    spans.push(msg_prefix);
    // msg_text is owned, so we need to handle it separately
    let msg_style = Style::new().fg(theme::FG);
    if search.is_empty() {
        spans.push(Span::styled(msg_text, msg_style));
    } else {
        let highlight_style = Style::new()
            .fg(theme::BG)
            .bg(theme::YELLOW)
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
