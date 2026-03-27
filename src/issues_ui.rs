use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::issues::{IssuesState, IssuesTab};
use crate::panel;
use crate::theme;
use crate::ui::panel_title;

pub fn render_issues_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &IssuesState,
) {
    let color = panel::by_number(panel::ISSUES).border_color(focused);
    let accent = panel::by_number(panel::ISSUES).bright_color;
    let muted = Style::new().fg(theme::MUTED);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel::ISSUES, focused))
        .border_style(Style::new().fg(color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" h/l", Style::new().fg(accent)),
            Span::styled(" tab ", muted),
            Span::styled("↩", Style::new().fg(accent)),
            Span::styled(" detail ", muted),
        ]));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let tab_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);

    render_tabs(frame, tab_area, state.active_tab, accent);

    if let Some(ref err) = state.error {
        let item = ListItem::new(Line::from(Span::styled(
            err.as_str(),
            Style::new().fg(theme::RED),
        )));
        frame.render_widget(List::new(vec![item]), content_area);
        return;
    }

    match state.active_tab {
        IssuesTab::Crashes => {
            if state.crashes.is_empty() {
                let item = ListItem::new(Line::from(Span::styled(
                    " no crashes",
                    muted,
                )));
                frame.render_widget(List::new(vec![item]), content_area);
                return;
            }

            if state.viewing_detail {
                render_detail(frame, content_area, &state.crashes[state.crash_selected].full_text);
            } else {
                render_crash_list(frame, content_area, state, focused, accent);
            }
        }
        IssuesTab::Anrs => {
            if state.anrs.is_empty() {
                let item = ListItem::new(Line::from(Span::styled(
                    " no ANRs",
                    muted,
                )));
                frame.render_widget(List::new(vec![item]), content_area);
                return;
            }

            if state.viewing_detail {
                render_detail(frame, content_area, &state.anrs[state.anr_selected].full_text);
            } else {
                render_anr_list(frame, content_area, state, focused, accent);
            }
        }
    }
}

fn render_tabs(frame: &mut Frame, area: Rect, active: IssuesTab, accent: ratatui::style::Color) {
    let crashes_style = if active == IssuesTab::Crashes {
        Style::new().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::MUTED)
    };
    let anrs_style = if active == IssuesTab::Anrs {
        Style::new().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::MUTED)
    };

    let line = Line::from(vec![
        Span::styled(" crashes", crashes_style),
        Span::styled("  ", Style::new()),
        Span::styled("anrs", anrs_style),
    ]);
    frame.render_widget(ratatui::widgets::Paragraph::new(line), area);
}

fn render_crash_list(
    frame: &mut Frame,
    area: Rect,
    state: &IssuesState,
    focused: bool,
    accent: ratatui::style::Color,
) {
    let visible_height = area.height as usize;
    let total = state.crashes.len();
    let selected = state.crash_selected;

    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = state.crashes[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let actual = start + i;
            let is_selected = actual == selected && focused;
            let style = if is_selected {
                Style::new().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::FG)
            };
            let prefix = if is_selected { "▸ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&entry.timestamp, Style::new().fg(theme::MUTED)),
                Span::styled(" ", Style::new()),
                Span::styled(&entry.exception, style),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), area);

    if total > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(theme::MUTED))
            .track_style(Style::new().fg(theme::SURFACE));
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn render_anr_list(
    frame: &mut Frame,
    area: Rect,
    state: &IssuesState,
    focused: bool,
    accent: ratatui::style::Color,
) {
    let visible_height = area.height as usize;
    let total = state.anrs.len();
    let selected = state.anr_selected;

    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = state.anrs[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let actual = start + i;
            let is_selected = actual == selected && focused;
            let style = if is_selected {
                Style::new().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::FG)
            };
            let prefix = if is_selected { "▸ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&entry.timestamp, Style::new().fg(theme::MUTED)),
                Span::styled(" ", Style::new()),
                Span::styled(&entry.reason, style),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), area);

    if total > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(theme::MUTED))
            .track_style(Style::new().fg(theme::SURFACE));
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn render_detail(frame: &mut Frame, area: Rect, text: &str) {
    let lines: Vec<Line> = text
        .lines()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::new().fg(theme::FG))))
        .collect();
    let paragraph = ratatui::widgets::Paragraph::new(lines)
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
