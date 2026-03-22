use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};

use crate::app::App;
use crate::battery;
use crate::database_ui;
use crate::logcat_ui;
use crate::panel;
use crate::theme;

const COMMAND_LABELS: [&str; 4] = [
    "open app",
    "kill app",
    "clear data",
    "wake screen",
];

pub const SUPERSCRIPT_DIGITS: [char; 5] = [
    '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}',
];

pub fn panel_title(panel_number: u8, focused: bool) -> Line<'static> {
    let def = panel::by_number(panel_number);
    let color = def.border_color(focused);
    let superscript = SUPERSCRIPT_DIGITS[(panel_number - 1) as usize];

    let digit_color = if focused { color } else { theme::MUTED };
    let mut spans = vec![Span::styled(
        format!(" {}", superscript),
        Style::new().fg(digit_color).add_modifier(Modifier::BOLD),
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

pub fn panel_block(panel_number: u8, focused: bool) -> Block<'static> {
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
) {
    let area = frame.area();

    let title_line = Line::from(" msh ").style(
        Style::new()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );
    let time_line = Line::from(time)
        .style(Style::new().fg(theme::FG))
        .alignment(Alignment::Center);
    let hint_line = if app.confirming_quit() {
        Line::from(vec![
            Span::styled(" q", Style::new().fg(theme::KEY_HINT)),
            Span::styled(" to confirm ", Style::new().fg(theme::FG)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" qq", Style::new().fg(theme::KEY_HINT)),
            Span::styled("uit ", Style::new().fg(theme::MUTED)),
        ])
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title_line)
        .title(time_line);

    if let Some(level) = battery_level {
        block = block.title(battery::battery_bar(level));
    }

    let info_line = Line::from(Span::styled(
        title,
        Style::new().fg(theme::MUTED),
    ))
    .alignment(Alignment::Right);

    block = block
        .title_bottom(hint_line)
        .title_bottom(info_line)
        .border_style(Style::new().fg(theme::SURFACE))
        .style(Style::new().bg(theme::BG).fg(theme::FG));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_panels(frame, inner, app, logcat_lines);
}

fn is_focused(app: &App, panel_number: u8) -> bool {
    app.focused_panel() == Some(panel_number)
}

fn render_panels(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String]) {
    let vis = app.panel_visibility();
    let top_visible = vis[0] || vis[1];
    let bot_visible = vis[2] || vis[3] || vis[4];

    match (top_visible, bot_visible) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_top_row(frame, rows[0], app, logcat_lines);
            render_bottom_section(frame, rows[1], app);
        }
        (true, false) => render_top_row(frame, area, app, logcat_lines),
        (false, true) => render_bottom_section(frame, area, app),
        (false, false) => {}
    }
}

fn render_top_row(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String]) {
    let vis = app.panel_visibility();
    match (vis[0], vis[1]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(area);
            render_commands_panel(frame, cols[0], app);
            logcat_ui::render_logcat_panel(frame, cols[1], is_focused(app, panel::LOGCAT), logcat_lines, app);
        }
        (true, false) => {
            render_commands_panel(frame, area, app)
        }
        (false, true) => logcat_ui::render_logcat_panel(frame, area, is_focused(app, panel::LOGCAT), logcat_lines, app),
        (false, false) => {}
    }
}

fn render_commands_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel_block(panel::COMMANDS, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut items: Vec<ListItem> = COMMAND_LABELS
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

    let (indicator, indicator_color) = if app.layout_bounds() {
        ("●", theme::GREEN)
    } else {
        ("·", theme::MUTED)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled("b", Style::new().fg(theme::KEY_HINT)),
        Span::styled(format!("ounds {indicator}"), Style::new().fg(indicator_color)),
    ])));

    let (indicator, indicator_color) = if app.airplane_mode() {
        ("●", theme::GREEN)
    } else {
        ("·", theme::MUTED)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled("a", Style::new().fg(theme::KEY_HINT)),
        Span::styled(format!("irplane {indicator}"), Style::new().fg(indicator_color)),
    ])));

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn render_bottom_section(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let panels: Vec<u8> = [panel::NETWORK, panel::SYSTEM, panel::DATABASE]
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
        if pn == panel::DATABASE {
            let im = app.input_mode();
            database_ui::render_database_panel(frame, cols[i], is_focused(app, panel::DATABASE), app.db_state_mut(), im);
        } else {
            frame.render_widget(panel_block(pn, false), cols[i]);
        }
    }
}
