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

use crate::app::InputMode;
use crate::database::{DatabaseState, ReplLine};
use crate::panel;
use crate::theme;
use crate::ui::{panel_block, panel_title, SUPERSCRIPT_DIGITS};

pub fn render_database_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    db_state: &mut DatabaseState,
    input_mode: InputMode,
) {
    let accent = Style::new().fg(theme::KEY_HINT);
    let muted = Style::new().fg(theme::MUTED);

    if let Some(ref err) = db_state.error {
        let block = panel_block(panel::DATABASE, focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let item = ListItem::new(Line::from(Span::styled(err.as_str(), Style::new().fg(theme::RED))));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    if let Some(ref db_name) = db_state.selected_db {
        let title = Line::from(vec![
            Span::styled(
                format!(" {} ", SUPERSCRIPT_DIGITS[(panel::DATABASE - 1) as usize]),
                Style::new().fg(panel::by_number(panel::DATABASE).border_color(focused)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("d", Style::new().fg(theme::KEY_HINT)),
            Span::styled(format!("atabase: {} ", db_name), Style::new().fg(theme::FG)),
        ]);

        let bottom_spans = if db_state.confirming_pull.is_some() {
            vec![
                Span::styled(" pull? ", Style::new().fg(theme::YELLOW)),
                Span::styled("enter", accent),
                Span::styled(" confirm ", muted),
                Span::styled("───", Style::new().fg(panel::by_number(panel::DATABASE).border_color(focused))),
                Span::styled(" any key", accent),
                Span::styled(" cancel ", muted),
            ]
        } else {
            vec![
                Span::styled(" e", accent),
                Span::styled("nter query ", muted),
                Span::styled("───", Style::new().fg(panel::by_number(panel::DATABASE).border_color(focused))),
                Span::styled(" p", accent),
                Span::styled("ull ", muted),
                Span::styled("───", Style::new().fg(panel::by_number(panel::DATABASE).border_color(focused))),
                Span::styled(" esc", accent),
                Span::styled(" back ", muted),
            ]
        };

        let color = panel::by_number(panel::DATABASE).border_color(focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title)
            .title_bottom(Line::from(bottom_spans))
            .border_style(Style::new().fg(color));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let editing = matches!(input_mode, InputMode::EditingQuery);

        let input_row = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1.min(inner.height),
        };
        let history_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };

        let input_line = if editing {
            Line::from(vec![
                Span::styled("> ", Style::new().fg(theme::ACCENT)),
                Span::styled(db_state.input.clone(), Style::new().fg(theme::FG)),
                Span::styled("_", Style::new().fg(theme::FG)),
            ])
        } else if db_state.history.is_empty() {
            Line::from(Span::styled("press e to enter a query", muted))
        } else {
            Line::from(Span::styled("> ", Style::new().fg(theme::MUTED)))
        };
        frame.render_widget(input_line, input_row);

        if !db_state.history.is_empty() {
            let visible_height = history_area.height as usize;
            let total = db_state.history.len();
            db_state.clamp_scroll(total, visible_height);
            let end = total.saturating_sub(db_state.scroll);
            let start = end.saturating_sub(visible_height);
            let items: Vec<ListItem> = db_state.history[start..end].iter().map(|line| {
                match line {
                    ReplLine::Input(s) => {
                        ListItem::new(Line::from(Span::styled(format!("> {s}"), Style::new().fg(theme::ACCENT))))
                    }
                    ReplLine::Output(s) => {
                        ListItem::new(Line::from(Span::styled(s.as_str(), Style::new().fg(theme::FG))))
                    }
                    ReplLine::Error(s) => {
                        ListItem::new(Line::from(Span::styled(s.as_str(), Style::new().fg(theme::RED))))
                    }
                }
            }).collect();
            frame.render_widget(List::new(items), history_area);
            if total > visible_height {
                let mut scrollbar_state =
                    ScrollbarState::new(total.saturating_sub(visible_height)).position(start);
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::new().fg(theme::MUTED))
                    .track_style(Style::new().fg(theme::SURFACE));
                frame.render_stateful_widget(scrollbar, history_area, &mut scrollbar_state);
            }
        }
    } else {
        let color = panel::by_number(panel::DATABASE).border_color(focused);
        let block = if focused && !db_state.databases.is_empty() {
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(panel_title(panel::DATABASE, focused))
                .title_bottom(Line::from(vec![
                    Span::styled(" p", accent),
                    Span::styled("ull ", muted),
                ]))
                .border_style(Style::new().fg(color))
        } else {
            panel_block(panel::DATABASE, focused)
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if db_state.databases.is_empty() {
            let item = ListItem::new(Line::from(Span::styled("detecting databases…", muted)));
            frame.render_widget(List::new(vec![item]), inner);
        } else {
            let confirming = db_state.confirming_pull.is_some();
            let items: Vec<ListItem> = db_state
                .databases
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let selected = i == db_state.selected_index && focused;
                    if selected && confirming {
                        ListItem::new(Line::from(vec![
                            Span::styled("▸ ", Style::new().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
                            Span::styled(format!("pull {name}? "), Style::new().fg(theme::YELLOW)),
                            Span::styled("enter", Style::new().fg(theme::KEY_HINT)),
                            Span::styled(" confirm  ", Style::new().fg(theme::MUTED)),
                            Span::styled("any key", Style::new().fg(theme::KEY_HINT)),
                            Span::styled(" cancel", Style::new().fg(theme::MUTED)),
                        ]))
                    } else {
                        let style = if selected {
                            Style::new().fg(theme::YELLOW).add_modifier(Modifier::BOLD)
                        } else {
                            Style::new().fg(theme::FG)
                        };
                        let prefix = if selected { "▸ " } else { "  " };
                        ListItem::new(Line::from(Span::styled(format!("{prefix}{name}"), style)))
                    }
                })
                .collect();
            frame.render_widget(List::new(items), inner);
        }
    }
}
