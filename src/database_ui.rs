use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use crate::database::{DatabaseState, ReplLine};
use crate::panel;
use crate::theme;
use crate::ui::{panel_block, panel_title, SUPERSCRIPT_DIGITS};

pub fn render_database_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    db_state: &mut DatabaseState,
) {
    let t = theme::current();
    let accent = Style::new().fg(t.danger);
    let muted = Style::new().fg(t.muted);

    if let Some(ref err) = db_state.error {
        let block = panel_block(panel::DATABASE, focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let item = ListItem::new(Line::from(Span::styled(err.as_str(), Style::new().fg(t.danger))));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    if let Some(ref db_name) = db_state.selected_db {
        let title = Line::from(vec![
            Span::styled(
                format!(" {} ", SUPERSCRIPT_DIGITS[(panel::DATABASE - 1) as usize]),
                Style::new().fg(panel::by_number(panel::DATABASE).border_color(focused)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("d", Style::new().fg(t.danger)),
            Span::styled(format!("atabase: {} ", db_name), Style::new().fg(t.fg)),
        ]);

        let border_fg = Style::new().fg(panel::by_number(panel::DATABASE).border_color(focused));
        let copied_active = db_state.copied_at
            .is_some_and(|ts| ts.elapsed() < std::time::Duration::from_secs(2));
        if !copied_active {
            db_state.copied_at = None;
        }
        let copy_spans: Vec<Span> = if copied_active {
            vec![Span::styled(" copied! ", Style::new().fg(t.success))]
        } else {
            vec![
                Span::styled(" c", accent),
                Span::styled("opy ", muted),
            ]
        };
        let mut bottom_spans = vec![
            Span::styled(" e", accent),
            Span::styled("dit ", muted),
            Span::styled("───", border_fg),
        ];
        bottom_spans.extend(copy_spans);
        bottom_spans.extend([
            Span::styled("───", border_fg),
            Span::styled(" r", accent),
            Span::styled("eset ", muted),
        ]);

        let color = panel::by_number(panel::DATABASE).border_color(focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title)
            .title_bottom(Line::from(bottom_spans))
            .border_style(Style::new().fg(color));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let editing = db_state.editing_query;

        let prompt_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: 2.min(inner.width),
            height: 1.min(inner.height),
        };
        let textarea_area = Rect {
            x: inner.x + 2.min(inner.width),
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width.saturating_sub(2),
            height: 1.min(inner.height),
        };
        let history_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };

        let selected = Style::new().fg(t.accent).add_modifier(Modifier::BOLD);

        if editing {
            frame.render_widget(Line::from(Span::styled("> ", selected)), prompt_area);
            db_state.textarea.set_style(selected);
            db_state.textarea.set_cursor_style(Style::new().fg(t.bg).bg(t.fg));
            frame.render_widget(&db_state.textarea, textarea_area);
        } else if db_state.history.is_empty() {
            let full_input_row = Rect { x: inner.x, y: prompt_area.y, width: inner.width, height: 1.min(inner.height) };
            frame.render_widget(Line::from(Span::styled("press e to enter a query", muted)), full_input_row);
        } else {
            frame.render_widget(Line::from(Span::styled("> ", muted)), prompt_area);
        }

        if !db_state.history.is_empty() {
            let visible_height = history_area.height as usize;
            let total = db_state.history.len();
            db_state.clamp_scroll(total, visible_height);
            let end = total.saturating_sub(db_state.scroll);
            let start = end.saturating_sub(visible_height);
            let items: Vec<ListItem> = db_state.history[start..end].iter().map(|line| {
                match line {
                    ReplLine::Input(s) => {
                        ListItem::new(Line::from(Span::styled(format!("> {s}"), selected)))
                    }
                    ReplLine::Output(s) => {
                        ListItem::new(Line::from(Span::styled(s.as_str(), Style::new().fg(t.fg))))
                    }
                    ReplLine::Error(s) => {
                        ListItem::new(Line::from(Span::styled(s.as_str(), Style::new().fg(t.danger))))
                    }
                }
            }).collect();
            frame.render_widget(List::new(items), history_area);
            if total > visible_height {
                let mut scrollbar_state =
                    ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::new().fg(t.muted))
                    .track_style(Style::new().fg(t.surface));
                frame.render_stateful_widget(scrollbar, history_area, &mut scrollbar_state);
            }
        }
    } else {
        let color = panel::by_number(panel::DATABASE).border_color(focused);
        let border_fg = Style::new().fg(color);
        let bottom_line = if db_state.confirming_pull.is_some() {
            Line::from(vec![
                Span::styled(" p", accent),
                Span::styled(" to confirm ", Style::new().fg(t.fg)),
            ])
        } else if focused && !db_state.databases.is_empty() {
            Line::from(vec![
                Span::styled(" p", accent),
                Span::styled("ull ", muted),
                Span::styled("───", border_fg),
                Span::styled(" r", accent),
                Span::styled("efresh ", muted),
            ])
        } else if focused {
            Line::from(vec![
                Span::styled(" r", accent),
                Span::styled("efresh ", muted),
            ])
        } else {
            Line::default()
        };

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(panel_title(panel::DATABASE, focused))
            .border_style(border_fg);
        if !bottom_line.spans.is_empty() {
            block = block.title_bottom(bottom_line);
        }
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if db_state.databases.is_empty() {
            let item = ListItem::new(Line::from(Span::styled("detecting databases…", muted)));
            frame.render_widget(List::new(vec![item]), inner);
        } else {
            let items: Vec<ListItem> = db_state
                .databases
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let selected = i == db_state.selected_index && focused;
                    let style = if selected {
                        Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(t.fg)
                    };
                    let prefix = if selected { "▸ " } else { "  " };
                    ListItem::new(Line::from(Span::styled(format!("{prefix}{name}"), style)))
                })
                .collect();
            frame.render_widget(List::new(items), inner);
        }
    }
}
