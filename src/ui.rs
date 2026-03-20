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
    visible_panels: &[u8],
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
        Line::from(" 1-6: toggle | q/Esc: exit ").style(Style::new().fg(theme::MUTED));

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
    render_panels(frame, inner, visible_panels);
}

fn render_panels(frame: &mut Frame, area: Rect, visible_panels: &[u8]) {
    match visible_panels.len() {
        0 => {}
        1 => {
            frame.render_widget(panel_block(visible_panels[0]), area);
        }
        2 => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            for (i, &col) in cols.iter().enumerate() {
                frame.render_widget(panel_block(visible_panels[i]), col);
            }
        }
        3 => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ])
                .split(area);
            for (i, &col) in cols.iter().enumerate() {
                frame.render_widget(panel_block(visible_panels[i]), col);
            }
        }
        4 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);
            let bot = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);
            frame.render_widget(panel_block(visible_panels[0]), top[0]);
            frame.render_widget(panel_block(visible_panels[1]), top[1]);
            frame.render_widget(panel_block(visible_panels[2]), bot[0]);
            frame.render_widget(panel_block(visible_panels[3]), bot[1]);
        }
        5 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ])
                .split(rows[0]);
            let bot = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);
            frame.render_widget(panel_block(visible_panels[0]), top[0]);
            frame.render_widget(panel_block(visible_panels[1]), top[1]);
            frame.render_widget(panel_block(visible_panels[2]), top[2]);
            frame.render_widget(panel_block(visible_panels[3]), bot[0]);
            frame.render_widget(panel_block(visible_panels[4]), bot[1]);
        }
        _ => {
            // 6 panels: 3×2 grid
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ])
                .split(rows[0]);
            let bot = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ])
                .split(rows[1]);
            frame.render_widget(panel_block(visible_panels[0]), top[0]);
            frame.render_widget(panel_block(visible_panels[1]), top[1]);
            frame.render_widget(panel_block(visible_panels[2]), top[2]);
            frame.render_widget(panel_block(visible_panels[3]), bot[0]);
            frame.render_widget(panel_block(visible_panels[4]), bot[1]);
            frame.render_widget(panel_block(visible_panels[5]), bot[2]);
        }
    }
}
