use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, COMMAND_LABELS};
use crate::battery;
use crate::logcat;
use crate::panel;
use crate::theme;

const SUPERSCRIPT_DIGITS: [char; 8] = [
    '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}',
    '\u{2078}',
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
    app: &App,
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

fn render_panels(frame: &mut Frame, area: Rect, app: &App, logcat_lines: &[String], monitored_pid: Option<u32>) {
    let vis = app.panel_visibility();
    let top_visible = vis[0] || vis[1];
    let bot_visible = vis[2] || vis[3] || vis[4] || vis[5];

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

fn render_top_row(frame: &mut Frame, area: Rect, app: &App, logcat_lines: &[String], monitored_pid: Option<u32>) {
    let vis = app.panel_visibility();
    match (vis[0], vis[1]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(area);
            render_commands_panel(frame, cols[0], is_focused(app, 1), app.commands_cursor());
            render_logcat_panel(frame, cols[1], is_focused(app, 2), logcat_lines, monitored_pid);
        }
        (true, false) => {
            render_commands_panel(frame, area, is_focused(app, 1), app.commands_cursor())
        }
        (false, true) => render_logcat_panel(frame, area, is_focused(app, 2), logcat_lines, monitored_pid),
        (false, false) => {}
    }
}

fn render_logcat_panel(frame: &mut Frame, area: Rect, focused: bool, logcat_lines: &[String], monitored_pid: Option<u32>) {
    let block = panel_block(2, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let pid_str = monitored_pid.map(|p| p.to_string());

    let visible_height = inner.height as usize;
    let start = logcat_lines.len().saturating_sub(visible_height);
    let visible: Vec<Line> = logcat_lines[start..]
        .iter()
        .map(|l| style_logcat_line(l, pid_str.as_deref()))
        .collect();

    let paragraph = Paragraph::new(visible).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
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

fn render_commands_panel(frame: &mut Frame, area: Rect, focused: bool, cursor: usize) {
    let block = panel_block(1, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = COMMAND_LABELS
        .iter()
        .map(|&label| ListItem::new(Span::styled(label, Style::new().fg(theme::FG))))
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let selected = if focused { Some(cursor) } else { None };
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, inner, &mut state);
}

fn render_bottom_section(frame: &mut Frame, area: Rect, app: &App) {
    let vis = app.panel_visibility();
    let left_visible = vis[2] || vis[3];
    let right_visible = vis[4] || vis[5];

    match (left_visible, right_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_left_column(frame, cols[0], vis);
            render_right_column(frame, cols[1], vis);
        }
        (true, false) => render_left_column(frame, area, vis),
        (false, true) => render_right_column(frame, area, vis),
        (false, false) => {}
    }
}

fn render_left_column(frame: &mut Frame, area: Rect, vis: &[bool; 6]) {
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

fn render_right_column(frame: &mut Frame, area: Rect, vis: &[bool; 6]) {
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
