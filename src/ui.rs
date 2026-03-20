use std::collections::HashMap;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
};

use crate::app::App;
use crate::apps;
use crate::battery;
use crate::panel;
use crate::theme;

const SUPERSCRIPT_DIGITS: [char; 8] = ['\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}', '\u{2078}'];

fn panel_title(panel_number: u8, focused: bool) -> Line<'static> {
    let def = panel::by_number(panel_number);
    let color = def.border_color(focused);
    let superscript = SUPERSCRIPT_DIGITS[(panel_number - 1) as usize];

    let mut spans = vec![
        Span::styled(
            format!(" {}", superscript),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ),
    ];

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
    app: &App,
    packages: Option<&[String]>,
    processes: Option<&HashMap<String, u32>>,
) {
    let area = frame.area();

    let title_line = Line::from(title).style(
        Style::new()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );
    let time_line =
        Line::from(time)
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
    render_panels(frame, inner, app, packages, processes, app.filter_text(), app.is_filtering());
}

fn is_focused(app: &App, panel_number: u8) -> bool {
    app.focused_panel() == Some(panel_number)
}

/// Vertical split between top row and bottom section.
fn render_panels(frame: &mut Frame, area: Rect, app: &App, packages: Option<&[String]>, processes: Option<&HashMap<String, u32>>, filter: &str, filtering: bool) {
    let vis = app.panel_visibility();
    let top_visible = vis[0] || vis[1];
    let bot_visible = vis[2] || vis[3] || vis[4] || vis[5] || vis[6];

    match (top_visible, bot_visible) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_top_row(frame, rows[0], app, packages, processes, filter, filtering);
            render_bottom_section(frame, rows[1], app);
        }
        (true, false) => render_top_row(frame, area, app, packages, processes, filter, filtering),
        (false, true) => render_bottom_section(frame, area, app),
        (false, false) => {}
    }
}

/// Horizontal split within the top row (panels 1, 2).
fn render_top_row(frame: &mut Frame, area: Rect, app: &App, packages: Option<&[String]>, processes: Option<&HashMap<String, u32>>, filter: &str, filtering: bool) {
    let vis = app.panel_visibility();
    match (vis[0], vis[1]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(area);
            render_apps_panel(frame, cols[0], is_focused(app, 1), packages, app.selected_app(), processes, filter, filtering);
            frame.render_widget(panel_block(2, is_focused(app, 2)), cols[1]);
        }
        (true, false) => render_apps_panel(frame, area, is_focused(app, 1), packages, app.selected_app(), processes, filter, filtering),
        (false, true) => frame.render_widget(panel_block(2, is_focused(app, 2)), area),
        (false, false) => {}
    }
}

fn apps_panel_title(focused: bool, filter: &str, filtering: bool) -> Line<'static> {
    let mut spans = panel_title(1, focused).spans;
    let border_color = panel::by_number(1).border_color(focused);

    if focused {
        spans.push(Span::styled("───", Style::new().fg(border_color)));
        if filtering || !filter.is_empty() {
            spans.push(Span::styled(filter.to_string(), Style::new().fg(theme::FG)));
            if filtering {
                spans.push(Span::styled("█", Style::new().fg(theme::ACCENT)));
                spans.push(Span::styled(" ↵", Style::new().fg(theme::KEY_HINT)));
            } else {
                spans.push(Span::styled(" esc", Style::new().fg(theme::KEY_HINT)));
            }
        } else {
            spans.push(Span::styled(
                "f",
                Style::new().fg(theme::KEY_HINT),
            ));
            spans.push(Span::styled("ilter", Style::new().fg(theme::MUTED)));
        }
    }

    spans.push(Span::styled(" ", Style::default()));
    Line::from(spans)
}

fn render_apps_panel(frame: &mut Frame, area: Rect, focused: bool, packages: Option<&[String]>, selected: Option<usize>, processes: Option<&HashMap<String, u32>>, filter: &str, filtering: bool) {
    let color = panel::by_number(1).border_color(focused);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(apps_panel_title(focused, filter, filtering))
        .border_style(Style::new().fg(color));

    if focused {
        let key = Style::new().fg(theme::KEY_HINT);
        let muted = Style::new().fg(theme::MUTED);
        let border = Style::new().fg(color);
        let hints = vec![
            Span::styled(" o", key),
            Span::styled("pen ", muted),
            Span::styled("───", border),
            Span::styled(" k", key),
            Span::styled("ill ", muted),
            Span::styled("───", border),
            Span::styled(" e", key),
            Span::styled("rase ", muted),
        ];

        block = block.title_bottom(Line::from(hints));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);
    apps::render_apps(frame, inner, packages, selected, processes, filter);
}

/// Horizontal split within the bottom section (3 columns).
fn render_bottom_section(frame: &mut Frame, area: Rect, app: &App) {
    let vis = app.panel_visibility();
    let left_visible = vis[2] || vis[3];
    let mid_visible = vis[4] || vis[5];
    let right_visible = vis[6];

    let mut columns: Vec<(Constraint, BottomColumn)> = Vec::new();
    if left_visible {
        columns.push((Constraint::Ratio(1, 1), BottomColumn::Left));
    }
    if mid_visible {
        columns.push((Constraint::Ratio(1, 1), BottomColumn::Mid));
    }
    if right_visible {
        columns.push((Constraint::Ratio(1, 1), BottomColumn::Right));
    }

    if columns.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = columns.iter().map(|(c, _)| *c).collect();
    let areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, (_, col_type)) in columns.iter().enumerate() {
        match col_type {
            BottomColumn::Left => render_left_column(frame, areas[i], vis),
            BottomColumn::Mid => render_mid_column(frame, areas[i], vis),
            BottomColumn::Right => frame.render_widget(panel_block(7, is_focused(app, 7)), areas[i]),
        }
    }
}

enum BottomColumn {
    Left,
    Mid,
    Right,
}

/// Vertical split within the left column (panels 3, 4).
fn render_left_column(frame: &mut Frame, area: Rect, vis: &[bool; 7]) {
    match (vis[2], vis[3]) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            frame.render_widget(panel_block(3, false), rows[0]);
            frame.render_widget(panel_block(4, false), rows[1]);
        }
        (true, false) => frame.render_widget(panel_block(3, false), area),
        (false, true) => frame.render_widget(panel_block(4, false), area),
        (false, false) => {}
    }
}

/// Vertical split within the mid column (panels 5, 6).
fn render_mid_column(frame: &mut Frame, area: Rect, vis: &[bool; 7]) {
    match (vis[4], vis[5]) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            frame.render_widget(panel_block(5, false), rows[0]);
            frame.render_widget(panel_block(6, false), rows[1]);
        }
        (true, false) => frame.render_widget(panel_block(5, false), area),
        (false, true) => frame.render_widget(panel_block(6, false), area),
        (false, false) => {}
    }
}
