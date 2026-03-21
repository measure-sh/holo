use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use crate::app::{App, InputMode, LogcatFilter};
use crate::database::DatabaseState;

const COMMAND_LABELS: [&str; 3] = [
    "open app",
    "kill app",
    "clear data",
];
use crate::battery;
use crate::logcat;
use crate::panel;
use crate::theme;

const SUPERSCRIPT_DIGITS: [char; 5] = [
    '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}',
];

fn panel_title(panel_number: u8, focused: bool) -> Line<'static> {
    let def = panel::by_number(panel_number);
    let color = def.border_color(focused);
    let superscript = SUPERSCRIPT_DIGITS[(panel_number - 1) as usize];

    let mut spans = vec![Span::styled(
        format!(" {}", superscript),
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    )];

    if def.is_focusable() {
        let mut chars = def.name.chars();
        let first = chars.next().unwrap();
        spans.push(Span::styled(
            String::from(first),
            Style::new().fg(theme::KEY_HINT),
        ));
        spans.push(Span::styled(
            format!("{} ", chars.as_str()),
            Style::new().fg(theme::FG),
        ));
    } else {
        spans.push(Span::styled(
            format!("{} ", def.name),
            Style::new().fg(theme::FG),
        ));
    }

    Line::from(spans)
}

fn panel_block(panel_number: u8, focused: bool) -> Block<'static> {
    let color = panel::by_number(panel_number).border_color(focused);
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(panel_number, focused))
        .border_style(Style::new().fg(color))
}

pub fn render_app(
    frame: &mut Frame,
    title: &str,
    time: &str,
    battery_level: Option<u8>,
    app: &mut App,
    logcat_lines: &[String],
    monitored_pid: Option<u32>,
) {
    let area = frame.area();

    let title_line = Line::from(title).style(
        Style::new()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );
    let time_line = Line::from(time)
        .style(Style::new().fg(theme::FG))
        .alignment(Alignment::Center);
    let hint_line = Line::from(vec![
        Span::styled(" q", Style::new().fg(theme::KEY_HINT)),
        Span::styled("uit ", Style::new().fg(theme::MUTED)),
    ]);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title_line)
        .title(time_line);

    if let Some(level) = battery_level {
        block = block.title(battery::battery_bar(level));
    }

    block = block
        .title_bottom(hint_line)
        .border_style(Style::new().fg(theme::SURFACE))
        .style(Style::new().bg(theme::BG).fg(theme::FG));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_panels(frame, inner, app, logcat_lines, monitored_pid);
}

fn is_focused(app: &App, panel_number: u8) -> bool {
    app.focused_panel() == Some(panel_number)
}

fn render_panels(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String], monitored_pid: Option<u32>) {
    let vis = app.panel_visibility();
    let top_visible = vis[0] || vis[1];
    let bot_visible = vis[2] || vis[3] || vis[4];

    match (top_visible, bot_visible) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_top_row(frame, rows[0], app, logcat_lines, monitored_pid);
            render_bottom_section(frame, rows[1], app);
        }
        (true, false) => render_top_row(frame, area, app, logcat_lines, monitored_pid),
        (false, true) => render_bottom_section(frame, area, app),
        (false, false) => {}
    }
}

fn render_top_row(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String], monitored_pid: Option<u32>) {
    let vis = app.panel_visibility();
    match (vis[0], vis[1]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(area);
            render_commands_panel(frame, cols[0]);
            render_logcat_panel(frame, cols[1], is_focused(app, 2), logcat_lines, monitored_pid, app);
        }
        (true, false) => {
            render_commands_panel(frame, area)
        }
        (false, true) => render_logcat_panel(frame, area, is_focused(app, 2), logcat_lines, monitored_pid, app),
        (false, false) => {}
    }
}

fn logcat_filter_bar(filter: &LogcatFilter, input_mode: InputMode, focused: bool) -> Line<'static> {
    let accent = Style::new().fg(theme::KEY_HINT);
    let muted = Style::new().fg(theme::MUTED);
    let border = Style::new().fg(panel::by_number(2).border_color(focused));

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
    spans.push(Span::styled(format!("ag:{} ", tag_display), muted));
    if matches!(input_mode, InputMode::EditingTag) {
        spans.push(Span::styled("↩ ", Style::new().fg(theme::RED)));
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
    spans.push(Span::styled(format!("earch:{} ", search_display), muted));
    if matches!(input_mode, InputMode::EditingSearch) {
        spans.push(Span::styled("↩ ", Style::new().fg(theme::RED)));
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

fn render_logcat_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    logcat_lines: &[String],
    monitored_pid: Option<u32>,
    app: &mut App,
) {
    let filter_tag = app.logcat_filter().tag.clone();
    let filter_search = app.logcat_filter().search.clone();
    let filter_level = app.logcat_filter().level;
    let input_mode = app.input_mode();

    let color = panel::by_number(2).border_color(focused);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(2, focused))
        .title_bottom(logcat_filter_bar(app.logcat_filter(), input_mode, focused))
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
    app.clamp_logcat_scroll(filtered.len(), visible_height);
    let logcat_scroll = app.logcat_scroll();
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

fn render_commands_panel(frame: &mut Frame, area: Rect) {
    let block = panel_block(1, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = COMMAND_LABELS
        .iter()
        .map(|&label| {
            let first = &label[..1];
            let rest = &label[1..];
            ListItem::new(Line::from(vec![
                Span::styled(first, Style::new().fg(theme::KEY_HINT)),
                Span::styled(rest, Style::new().fg(theme::MUTED)),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn render_bottom_section(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let panels: Vec<u8> = [3, 4, 5]
        .iter()
        .copied()
        .filter(|&n| vis[(n - 1) as usize])
        .collect();

    if panels.is_empty() {
        return;
    }

    let pct = 100 / panels.len() as u16;
    let constraints: Vec<Constraint> = panels.iter().map(|_| Constraint::Percentage(pct)).collect();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, &pn) in panels.iter().enumerate() {
        if pn == 5 {
            let im = app.input_mode();
            render_database_panel(frame, cols[i], is_focused(app, 5), app.db_state_mut(), im);
        } else {
            frame.render_widget(panel_block(pn, false), cols[i]);
        }
    }
}

fn render_database_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    db_state: &mut DatabaseState,
    input_mode: InputMode,
) {
    let accent = Style::new().fg(theme::KEY_HINT);
    let muted = Style::new().fg(theme::MUTED);

    if let Some(ref err) = db_state.error {
        let block = panel_block(5, focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let item = ListItem::new(Line::from(Span::styled(err.as_str(), Style::new().fg(theme::RED))));
        frame.render_widget(List::new(vec![item]), inner);
        return;
    }

    if let Some(ref db_name) = db_state.selected_db {
        let title = Line::from(vec![
            Span::styled(
                format!(" {} ", SUPERSCRIPT_DIGITS[4]),
                Style::new().fg(panel::by_number(5).border_color(focused)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("d", Style::new().fg(theme::KEY_HINT)),
            Span::styled(format!("atabase: {} ", db_name), Style::new().fg(theme::FG)),
        ]);

        let bottom_spans = vec![
            Span::styled(" e", accent),
            Span::styled("nter query ", muted),
            Span::styled("───", Style::new().fg(panel::by_number(5).border_color(focused))),
            Span::styled(" esc", accent),
            Span::styled(" back ", muted),
        ];

        let color = panel::by_number(5).border_color(focused);
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
                    crate::database::ReplLine::Input(s) => {
                        ListItem::new(Line::from(Span::styled(format!("> {s}"), Style::new().fg(theme::ACCENT))))
                    }
                    crate::database::ReplLine::Output(s) => {
                        ListItem::new(Line::from(Span::styled(s.as_str(), Style::new().fg(theme::FG))))
                    }
                    crate::database::ReplLine::Error(s) => {
                        ListItem::new(Line::from(Span::styled(s.as_str(), Style::new().fg(theme::RED))))
                    }
                }
            }).collect();
            frame.render_widget(List::new(items), history_area);
        }
    } else {
        let block = panel_block(5, focused);
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
                    let style = if i == db_state.selected_index && focused {
                        Style::new().fg(theme::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(theme::FG)
                    };
                    let prefix = if i == db_state.selected_index && focused { "▸ " } else { "  " };
                    ListItem::new(Line::from(Span::styled(format!("{prefix}{name}"), style)))
                })
                .collect();
            frame.render_widget(List::new(items), inner);
        }
    }
}
