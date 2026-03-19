use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders},
    Frame,
};

use crate::theme;

const PANEL_TITLES: [&str; 6] = [
    "1. Installed Apps",
    "2. Logcat",
    "3. Network",
    "4. CPU",
    "5. Memory",
    "6. Disk Usage",
];

fn panel_block(index: u8, selected_panel: Option<u8>) -> Block<'static> {
    let border_color = if selected_panel == Some(index) {
        theme::ACCENT
    } else {
        theme::SURFACE
    };

    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", PANEL_TITLES[(index - 1) as usize]))
        .border_style(Style::new().fg(border_color))
}

pub fn render_panels(frame: &mut Frame, area: Rect, selected_panel: Option<u8>) {
    // Vertical split: 50/50 → top_row, bottom_row
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Top row: horizontal 40/60 → panel 1, panel 2
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(rows[0]);

    // Bottom row: horizontal 40/30/30 → panel 3, mid_right, panel 6
    let bot_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

    // mid_right: vertical 50/50 → panel 4, panel 5
    let mid_right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(bot_cols[1]);

    frame.render_widget(panel_block(1, selected_panel), top_cols[0]);
    frame.render_widget(panel_block(2, selected_panel), top_cols[1]);
    frame.render_widget(panel_block(3, selected_panel), bot_cols[0]);
    frame.render_widget(panel_block(4, selected_panel), mid_right[0]);
    frame.render_widget(panel_block(5, selected_panel), mid_right[1]);
    frame.render_widget(panel_block(6, selected_panel), bot_cols[2]);
}
