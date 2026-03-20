use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders},
    Frame,
};

use crate::apps;
use crate::battery;
use crate::theme;

const PANEL_TITLES: [&str; 7] = [
    "1. Installed Apps",
    "2. Logcat",
    "3. Network",
    "4. CPU",
    "5. Memory",
    "6. Disk Usage",
    "7. Commands",
];

fn panel_block(index: u8) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", PANEL_TITLES[(index - 1) as usize]))
        .border_style(Style::new().fg(theme::SURFACE))
}

pub fn render_app(
    frame: &mut Frame,
    title: &str,
    time: &str,
    battery_level: Option<u8>,
    visible: &[bool; 7],
    packages: &[String],
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
    let hint_line =
        Line::from(" 1-7: toggle | q/Esc: exit ").style(Style::new().fg(theme::MUTED));

    let mut block = Block::default()
        .borders(Borders::ALL)
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
    render_panels(frame, inner, visible, packages);
}

/// Level 1: vertical split between top row and bottom section.
fn render_panels(frame: &mut Frame, area: Rect, vis: &[bool; 7], packages: &[String]) {
    let top_visible = vis[0] || vis[1];
    let bot_visible = vis[2] || vis[3] || vis[4] || vis[5] || vis[6];

    match (top_visible, bot_visible) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_top_row(frame, rows[0], vis, packages);
            render_bottom_section(frame, rows[1], vis);
        }
        (true, false) => render_top_row(frame, area, vis, packages),
        (false, true) => render_bottom_section(frame, area, vis),
        (false, false) => {}
    }
}

/// Level 2a: horizontal split within the top row (panels 1, 2).
fn render_top_row(frame: &mut Frame, area: Rect, vis: &[bool; 7], packages: &[String]) {
    match (vis[0], vis[1]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(area);
            render_apps_panel(frame, cols[0], packages);
            frame.render_widget(panel_block(2), cols[1]);
        }
        (true, false) => render_apps_panel(frame, area, packages),
        (false, true) => frame.render_widget(panel_block(2), area),
        (false, false) => {}
    }
}

fn render_apps_panel(frame: &mut Frame, area: Rect, packages: &[String]) {
    let block = panel_block(1);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    apps::render_apps(frame, inner, packages);
}

/// Level 2b: horizontal split within the bottom section (3 columns).
fn render_bottom_section(frame: &mut Frame, area: Rect, vis: &[bool; 7]) {
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
            BottomColumn::Right => frame.render_widget(panel_block(7), areas[i]),
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
            frame.render_widget(panel_block(3), rows[0]);
            frame.render_widget(panel_block(4), rows[1]);
        }
        (true, false) => frame.render_widget(panel_block(3), area),
        (false, true) => frame.render_widget(panel_block(4), area),
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
            frame.render_widget(panel_block(5), rows[0]);
            frame.render_widget(panel_block(6), rows[1]);
        }
        (true, false) => frame.render_widget(panel_block(5), area),
        (false, true) => frame.render_widget(panel_block(6), area),
        (false, false) => {}
    }
}
