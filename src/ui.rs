use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders},
    Frame,
};

use crate::battery;
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

pub fn render_app(
    frame: &mut Frame,
    title: &str,
    time: &str,
    battery_level: Option<u8>,
    selected_panel: Option<u8>,
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
        Line::from(" 1-6: panel | q/Esc: exit ").style(Style::new().fg(theme::MUTED));

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
    render_panels(frame, inner, selected_panel);
}

fn render_panels(frame: &mut Frame, area: Rect, selected_panel: Option<u8>) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(rows[0]);

    let bot_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

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
