use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};

use crate::app::{App, SettingsAction};
use crate::battery;
use crate::database_ui;
use crate::files_ui;
use crate::logcat_ui;
use crate::monitor_ui;
use crate::panel;
use crate::permissions_ui;
use crate::theme;

const COMMAND_LABELS: [&str; 3] = [
    "open app",
    "kill app",
    "wake screen",
];

pub const SUPERSCRIPT_DIGITS: [char; 7] = [
    '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}',
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
    let quit_spans = if app.confirming_quit() {
        vec![
            Span::styled(" q", Style::new().fg(theme::KEY_HINT)),
            Span::styled(" to confirm ", Style::new().fg(theme::FG)),
        ]
    } else {
        vec![
            Span::styled(" qq", Style::new().fg(theme::KEY_HINT)),
            Span::styled("uit ", Style::new().fg(theme::MUTED)),
        ]
    };
    let mut hint_spans = vec![
        Span::styled(" s", Style::new().fg(theme::KEY_HINT)),
        Span::styled("ettings ", Style::new().fg(theme::MUTED)),
        Span::styled("───", Style::new().fg(theme::SURFACE)),
    ];
    hint_spans.extend(quit_spans);
    let hint_line = Line::from(hint_spans);

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

    if app.show_settings() {
        render_settings_dialog(frame, area, app);
    }
}

fn is_focused(app: &App, panel_number: u8) -> bool {
    app.focused_panel() == Some(panel_number)
}

fn render_panels(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String]) {
    let vis = app.panel_visibility();
    let left_visible = vis[0] || vis[1] || vis[3];
    let right_visible = vis[2];
    let bot_visible = vis[4] || vis[5] || vis[6];

    let top_visible = left_visible || right_visible;

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

fn render_left_column(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let panels: Vec<u8> = [panel::COMMANDS, panel::TRACE, panel::PERMISSIONS]
        .iter()
        .copied()
        .filter(|&n| vis[(n - 1) as usize])
        .collect();

    if panels.is_empty() {
        return;
    }

    let pct = 100 / panels.len() as u16;
    let constraints: Vec<Constraint> = panels.iter().map(|_| Constraint::Percentage(pct)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, &pn) in panels.iter().enumerate() {
        if pn == panel::COMMANDS {
            render_commands_panel(frame, rows[i], app);
        } else if pn == panel::TRACE {
            render_trace_panel(frame, rows[i], app);
        } else if pn == panel::PERMISSIONS {
            permissions_ui::render_permissions_panel(frame, rows[i], is_focused(app, panel::PERMISSIONS), app.permissions_state());
        }
    }
}

fn render_top_row(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String]) {
    let vis = app.panel_visibility();
    let left_visible = vis[0] || vis[1] || vis[3];
    match (left_visible, vis[2]) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(area);
            render_left_column(frame, cols[0], app);
            logcat_ui::render_logcat_panel(frame, cols[1], is_focused(app, panel::LOGCAT), logcat_lines, app);
        }
        (true, false) => render_left_column(frame, area, app),
        (false, true) => logcat_ui::render_logcat_panel(frame, area, is_focused(app, panel::LOGCAT), logcat_lines, app),
        (false, false) => {}
    }
}

fn render_commands_panel(frame: &mut Frame, area: Rect, app: &mut App) {
    let flash_idx = match app.command_flash {
        Some((idx, t)) if t.elapsed() < std::time::Duration::from_secs(1) => Some(idx),
        Some(_) => { app.command_flash = None; None }
        None => None,
    };

    let block = panel_block(panel::COMMANDS, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut items: Vec<ListItem> = COMMAND_LABELS
        .iter()
        .enumerate()
        .map(|(i, &label)| {
            if flash_idx == Some(i) {
                ListItem::new(Line::from(Span::styled("done!", Style::new().fg(theme::GREEN))))
            } else {
                let first = &label[..1];
                let rest = &label[1..];
                ListItem::new(Line::from(vec![
                    Span::styled(first, Style::new().fg(theme::KEY_HINT)),
                    Span::styled(rest, Style::new().fg(theme::MUTED)),
                ]))
            }
        })
        .collect();

    let accent = panel::by_number(panel::COMMANDS).bright_color;

    let mut bounds_spans: Vec<Span> = Vec::new();
    if app.layout_bounds() {
        bounds_spans.push(Span::styled("• ", Style::new().fg(accent)));
    }
    bounds_spans.push(Span::styled("b", Style::new().fg(theme::KEY_HINT)));
    bounds_spans.push(Span::styled("ounds", Style::new().fg(theme::MUTED)));
    items.push(ListItem::new(Line::from(bounds_spans)));

    let mut airplane_spans: Vec<Span> = Vec::new();
    if app.airplane_mode() {
        airplane_spans.push(Span::styled("• ", Style::new().fg(accent)));
    }
    airplane_spans.push(Span::styled("a", Style::new().fg(theme::KEY_HINT)));
    airplane_spans.push(Span::styled("irplane", Style::new().fg(theme::MUTED)));
    items.push(ListItem::new(Line::from(airplane_spans)));

    let clear_flash = app.clear_flash
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1));
    if !clear_flash { app.clear_flash = None; }
    let clear_item = if clear_flash {
        ListItem::new(Line::from(Span::styled("done!", Style::new().fg(theme::GREEN))))
    } else if app.confirming_clear() {
        ListItem::new(Line::from(vec![
            Span::styled("c", Style::new().fg(theme::KEY_HINT)),
            Span::styled(" to confirm ", Style::new().fg(theme::FG)),
        ]))
    } else {
        ListItem::new(Line::from(vec![
            Span::styled("cc", Style::new().fg(theme::KEY_HINT)),
            Span::styled("lear data", Style::new().fg(theme::MUTED)),
        ]))
    };
    items.push(clear_item);

    let uninstall_flash = app.uninstall_flash
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1));
    if !uninstall_flash { app.uninstall_flash = None; }
    let uninstall_item = if uninstall_flash {
        ListItem::new(Line::from(Span::styled("done!", Style::new().fg(theme::GREEN))))
    } else if app.confirming_uninstall() {
        ListItem::new(Line::from(vec![
            Span::styled("u", Style::new().fg(theme::KEY_HINT)),
            Span::styled(" to confirm ", Style::new().fg(theme::FG)),
        ]))
    } else {
        ListItem::new(Line::from(vec![
            Span::styled("uu", Style::new().fg(theme::KEY_HINT)),
            Span::styled("ninstall app", Style::new().fg(theme::MUTED)),
        ]))
    };
    items.push(uninstall_item);

    let list = List::new(items);
    frame.render_widget(list, inner);
}

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn render_trace_panel(frame: &mut Frame, area: Rect, app: &App) {
    let focused = is_focused(app, panel::TRACE);
    let state = app.trace_state();

    let mut block = panel_block(panel::TRACE, focused);

    if focused {
        let accent = Style::new().fg(theme::KEY_HINT);
        let muted = Style::new().fg(theme::MUTED);
        let border = Style::new().fg(panel::by_number(panel::TRACE).border_color(true));
        let mut spans = Vec::new();
        if state.recording {
            spans.extend([
                Span::styled(" s", accent),
                Span::styled("top ", muted),
            ]);
        } else {
            spans.extend([
                Span::styled(" s", accent),
                Span::styled("tart ", muted),
            ]);
            if !state.pulled_traces.is_empty() {
                spans.extend([
                    Span::styled("───", border),
                    Span::styled(" ↩", accent),
                    Span::styled(" open ", muted),
                    Span::styled("───", border),
                    Span::styled(" d", accent),
                    Span::styled("elete ", muted),
                ]);
            }
        }
        spans.extend([
            Span::styled("───", border),
            Span::styled(" esc", accent),
            Span::styled(" close ", muted),
        ]);
        block = block.title_bottom(Line::from(spans));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let accent_color = panel::by_number(panel::TRACE).bright_color;

    let flash_active = state.message_at
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1));

    let mut items: Vec<ListItem> = Vec::new();

    if state.recording {
        let elapsed = state.started_at
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        let mins = elapsed / 60;
        let secs = elapsed % 60;
        let spinner_idx = state.started_at
            .map(|t| (t.elapsed().as_millis() / 80) as usize % SPINNER.len())
            .unwrap_or(0);
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{} ", SPINNER[spinner_idx]),
                Style::new().fg(accent_color),
            ),
            Span::styled(
                format!("tracing {:02}:{:02}", mins, secs),
                Style::new().fg(accent_color),
            ),
        ])));
    } else if flash_active {
        if let Some(msg) = &state.status_message {
            items.push(ListItem::new(Line::from(
                Span::styled(msg.clone(), Style::new().fg(theme::GREEN)),
            )));
        }
    } else if state.pulled_traces.is_empty() {
        items.push(ListItem::new(Line::from(
            Span::styled("no traces yet", Style::new().fg(theme::MUTED)),
        )));
    } else {
        for (i, path) in state.pulled_traces.iter().enumerate() {
            let name = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let selected = focused && i == state.selected_index;
            let style = if selected {
                Style::new().fg(accent_color).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::MUTED)
            };
            let prefix = if selected { "▸ " } else { "  " };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(name, style),
            ])));
        }
    }

    frame.render_widget(List::new(items), inner);
}

fn render_bottom_section(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let monitor_visible = vis[4];
    let right_visible = vis[5] || vis[6];

    match (monitor_visible, right_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            monitor_ui::render_monitor_panel(frame, cols[0], is_focused(app, panel::MONITOR), app.monitor_state());
            render_right_column(frame, cols[1], app);
        }
        (true, false) => {
            monitor_ui::render_monitor_panel(frame, area, is_focused(app, panel::MONITOR), app.monitor_state());
        }
        (false, true) => {
            render_right_column(frame, area, app);
        }
        (false, false) => {}
    }
}

fn render_right_column(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    match (vis[5], vis[6]) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            files_ui::render_files_panel(frame, rows[0], is_focused(app, panel::FILES), app.files_state());
            let im = app.input_mode();
            database_ui::render_database_panel(frame, rows[1], is_focused(app, panel::DATABASE), app.db_state_mut(), im);
        }
        (true, false) => {
            files_ui::render_files_panel(frame, area, is_focused(app, panel::FILES), app.files_state());
        }
        (false, true) => {
            let im = app.input_mode();
            database_ui::render_database_panel(frame, area, is_focused(app, panel::DATABASE), app.db_state_mut(), im);
        }
        (false, false) => {}
    }
}

fn render_settings_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let items = app.settings_items();
    let width = area.width.saturating_sub(10).max(40);
    let height = (items.len() as u16 + 4).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    let accent = Style::new().fg(theme::ACCENT);
    let hint = Style::new().fg(theme::FG);

    let dim = ratatui::widgets::Block::default().style(Style::new().bg(theme::BG).fg(theme::MUTED));
    frame.render_widget(dim, area);

    frame.render_widget(ratatui::widgets::Clear, dialog_area);

    let selected_item = items.get(app.settings_index());
    let mut bottom_spans = Vec::new();
    if let Some(item) = selected_item {
        match item.action {
            SettingsAction::Copy => {
                bottom_spans.extend([
                    Span::styled(" ↩", accent),
                    Span::styled(" copy ", hint),
                    Span::styled("───", Style::new().fg(theme::MUTED)),
                ]);
            }
            _ => {
                bottom_spans.extend([
                    Span::styled(" ↩", accent),
                    Span::styled(" select ", hint),
                    Span::styled("───", Style::new().fg(theme::MUTED)),
                ]);
            }
        }
    }
    bottom_spans.extend([
        Span::styled(" esc", accent),
        Span::styled(" close ", hint),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(" settings ", accent)))
        .title_bottom(Line::from(bottom_spans))
        .border_style(Style::new().fg(theme::MUTED))
        .style(Style::new().bg(theme::SURFACE));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let selected = i == app.settings_index();
            let style = if selected {
                Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::FG)
            };
            let prefix = if selected { "▸ " } else { "  " };
            let mut spans = vec![
                Span::styled(prefix, style),
                Span::styled(item.label.to_string(), style),
            ];
            if !item.value.is_empty() {
                spans.push(Span::styled(format!(": {}", item.value), Style::new().fg(theme::FG)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    frame.render_widget(List::new(list_items), inner);
}
