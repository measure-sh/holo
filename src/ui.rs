use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
};

use crate::apps;
use crate::battery;
use crate::theme;

const PANEL_INFO: [(&str, ratatui::style::Color); 7] = [
    ("installed apps", theme::DIM_BLUE),
    ("logcat",         theme::DIM_GREEN),
    ("network",        theme::DIM_YELLOW),
    ("cpu",            theme::DIM_CYAN),
    ("memory",         theme::DIM_MAGENTA),
    ("disk usage",     theme::DIM_RED),
    ("commands",       theme::DIM_TEAL),
];

const SUPERSCRIPT_DIGITS: [char; 8] = ['\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}', '\u{2078}'];

const FOCUSABLE_PANELS: [u8; 3] = [1, 2, 7];

fn bright_color(index: u8) -> ratatui::style::Color {
    match index {
        1 => theme::ACCENT,
        2 => theme::GREEN,
        7 => theme::CYAN,
        _ => PANEL_INFO[(index - 1) as usize].1,
    }
}

fn panel_title(index: u8, focused: bool) -> Line<'static> {
    let (name, color) = PANEL_INFO[(index - 1) as usize];
    let border_color = if focused { bright_color(index) } else { color };
    let superscript = SUPERSCRIPT_DIGITS[(index - 1) as usize];
    let focusable = FOCUSABLE_PANELS.contains(&index);

    let mut spans = vec![
        Span::styled(
            format!(" {}", superscript),
            Style::new().fg(border_color).add_modifier(Modifier::BOLD),
        ),
    ];

    if focusable {
        let mut chars = name.chars();
        let first = chars.next().unwrap();
        spans.push(Span::styled(
            String::from(first),
            Style::new().fg(border_color),
        ));
        spans.push(Span::styled(
            format!("{} ", chars.as_str()),
            Style::new().fg(theme::MUTED),
        ));
    } else {
        spans.push(Span::styled(
            format!("{} ", name),
            Style::new().fg(theme::MUTED),
        ));
    }

    Line::from(spans)
}

fn panel_block(index: u8, focused: bool) -> Block<'static> {
    let (_, color) = PANEL_INFO[(index - 1) as usize];
    let border_color = if focused { bright_color(index) } else { color };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(panel_title(index, focused))
        .border_style(Style::new().fg(border_color))
}

pub fn render_app(
    frame: &mut Frame,
    title: &str,
    time: &str,
    battery_level: Option<u8>,
    visible: &[bool; 7],
    focused: Option<u8>,
    packages: Option<&[String]>,
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
    let key_style = Style::new()
        .fg(theme::ACCENT)
        .add_modifier(Modifier::BOLD);
    let hint_style = Style::new().fg(theme::MUTED);
    let hint_line = Line::from(vec![
        Span::styled(" 1", key_style),
        Span::styled("-", hint_style),
        Span::styled("7", key_style),
        Span::styled(" toggle visibility ", hint_style),
        Span::styled("│", hint_style),
        Span::styled(" i", key_style),
        Span::styled("/", hint_style),
        Span::styled("l", key_style),
        Span::styled("/", hint_style),
        Span::styled("c", key_style),
        Span::styled(" focus ", hint_style),
        Span::styled("│", hint_style),
        Span::styled(" q", key_style),
        Span::styled("/", hint_style),
        Span::styled("Esc", key_style),
        Span::styled(" exit ", hint_style),
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
    render_panels(frame, inner, visible, focused, packages);
}

/// Level 1: vertical split between top row and bottom section.
fn render_panels(frame: &mut Frame, area: Rect, vis: &[bool; 7], focused: Option<u8>, packages: Option<&[String]>) {
    let top_visible = vis[0] || vis[1];
    let bot_visible = vis[2] || vis[3] || vis[4] || vis[5] || vis[6];

    match (top_visible, bot_visible) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_top_row(frame, rows[0], vis, focused, packages);
            render_bottom_section(frame, rows[1], vis, focused);
        }
        (true, false) => render_top_row(frame, area, vis, focused, packages),
        (false, true) => render_bottom_section(frame, area, vis, focused),
        (false, false) => {}
    }
}

/// Level 2a: horizontal split within the top row (panels 1, 2).
fn render_top_row(frame: &mut Frame, area: Rect, vis: &[bool; 7], focused: Option<u8>, packages: Option<&[String]>) {
    let f = |n: u8| focused == Some(n);
    match (vis[0], vis[1]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(area);
            render_apps_panel(frame, cols[0], f(1), packages);
            frame.render_widget(panel_block(2, f(2)), cols[1]);
        }
        (true, false) => render_apps_panel(frame, area, f(1), packages),
        (false, true) => frame.render_widget(panel_block(2, f(2)), area),
        (false, false) => {}
    }
}

fn render_apps_panel(frame: &mut Frame, area: Rect, focused: bool, packages: Option<&[String]>) {
    let block = panel_block(1, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    apps::render_apps(frame, inner, packages);
}

/// Level 2b: horizontal split within the bottom section (3 columns).
fn render_bottom_section(frame: &mut Frame, area: Rect, vis: &[bool; 7], focused: Option<u8>) {
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
            BottomColumn::Right => frame.render_widget(panel_block(7, focused == Some(7)), areas[i]),
        }
    }
}

enum BottomColumn {
    Left,
    Mid,
    Right,
}

/// Level 3a: vertical split within the left column (panels 3, 4).
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

/// Level 3b: vertical split within the mid column (panels 5, 6).
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
