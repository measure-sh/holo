use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState},
    Frame,
};

use crate::app::App;
use crate::selector;
use crate::toolbar::DropdownKind;
use crate::battery;
use crate::database_ui;
use crate::files_ui;
use crate::logcat_ui;
use crate::monitor_ui;
use crate::panel;
use crate::permissions_ui;
use crate::theme;


pub const SUPERSCRIPT_DIGITS: [char; 8] = [
    '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}', '\u{2078}',
];

pub fn panel_title(panel_number: u8, focused: bool) -> Line<'static> {
    let def = panel::by_number(panel_number);
    let color = def.border_color(focused);

    let mut spans = Vec::new();

    let superscript = if panel_number == 0 {
        '\u{2070}'
    } else {
        SUPERSCRIPT_DIGITS[(panel_number - 1) as usize]
    };
    let digit_color = if focused { color } else { theme::MUTED };
    spans.push(Span::styled(
        format!(" {}", superscript),
        Style::new().fg(digit_color).add_modifier(Modifier::BOLD),
    ));

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

    let mut hint_spans = if app.confirming_quit() {
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
    if app.focused_panel().is_some() {
        let label = if app.is_zoomed() { "oom out " } else { "oom in " };
        hint_spans.push(Span::styled(" z", Style::new().fg(theme::KEY_HINT)));
        hint_spans.push(Span::styled(label, Style::new().fg(theme::MUTED)));
    }
    let hint_line = Line::from(hint_spans);

    let branding_line = Line::from(vec![
        Span::styled("made with ", Style::new().fg(theme::MUTED)),
        Span::styled("\u{2665}", Style::new().fg(theme::RED)),
        Span::styled(" by ", Style::new().fg(theme::MUTED)),
        Span::styled("measure.sh ", Style::new().fg(theme::ACCENT)),
    ]).alignment(Alignment::Center);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title_line);

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
        .title_bottom(branding_line)
        .title_bottom(info_line)
        .border_style(Style::new().fg(theme::SURFACE))
        .style(Style::new().bg(theme::BG).fg(theme::FG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    render_toolbar(frame, chunks[0], app);
    render_panels(frame, chunks[1], app, logcat_lines);

    if app.toolbar().open.is_some() {
        render_dim_overlay(frame, area);
        render_dropdown_overlay(frame, chunks[0], app);
    }

    if let Some(msg) = &app.dialog {
        render_dim_overlay(frame, area);
        render_dialog(frame, area, msg);
    }
}

fn render_toolbar(frame: &mut Frame, area: Rect, app: &App) {
    let tb = app.toolbar();
    let has_device = tb.device.is_some();
    let has_app = tb.package.is_some();

    let device_label = tb.device_label();
    let app_label = tb.app_label();

    let (device_dot, device_fg, device_bg) = if has_device && tb.device_connected {
        (theme::CYAN, theme::FG, theme::DIM_CYAN)
    } else if has_device {
        (theme::RED, theme::FG, theme::DIM_RED)
    } else {
        (theme::MUTED, theme::MUTED, theme::SURFACE)
    };
    let app_dot = if has_app { theme::GREEN } else { theme::MUTED };
    let app_fg = if has_app { theme::FG } else { theme::MUTED };
    let app_bg = if has_app { theme::DIM_GREEN } else { theme::SURFACE };

    let line = Line::from(vec![
        Span::styled("F1 ", Style::new().fg(theme::KEY_HINT)),
        Span::styled(" \u{2022} ", Style::new().fg(device_dot).bg(device_bg)),
        Span::styled(
            format!("{device_label} \u{25BE} "),
            Style::new().fg(device_fg).bg(device_bg),
        ),
        Span::styled("      ", Style::new()),
        Span::styled("F2 ", Style::new().fg(theme::KEY_HINT)),
        Span::styled(" \u{2022} ", Style::new().fg(app_dot).bg(app_bg)),
        Span::styled(
            format!("{app_label} \u{25BE} "),
            Style::new().fg(app_fg).bg(app_bg),
        ),
    ]);

    frame.render_widget(
        ratatui::widgets::Paragraph::new(line).alignment(Alignment::Center),
        area,
    );
}

fn render_dim_overlay(frame: &mut Frame, area: Rect) {
    let dim = Block::default().style(Style::new().bg(theme::BG).add_modifier(Modifier::DIM));
    frame.render_widget(dim, area);
}

fn render_dropdown_overlay(frame: &mut Frame, toolbar_area: Rect, app: &App) {
    let tb = app.toolbar();
    let Some(kind) = tb.open else { return };

    let anchor_y = toolbar_area.y + 1;

    let screen = frame.area();
    let (width, max_items) = match kind {
        DropdownKind::Device => (40u16, 8u16),
        DropdownKind::App => (44u16, 16u16),
    };
    let width = width.min(screen.width.saturating_sub(2));
    let available = screen.height.saturating_sub(anchor_y);
    let height = (max_items + 2).min(available).max(5);
    let x = toolbar_area.x + (toolbar_area.width.saturating_sub(width)) / 2;

    let dropdown_area = Rect::new(x, anchor_y, width, height);

    frame.render_widget(Clear, dropdown_area);

    let accent_color = theme::ACCENT;

    let title = match kind {
        DropdownKind::Device => " select device ",
        DropdownKind::App => " select app ",
    };

    let mut bottom_spans = vec![
        Span::styled(" /", Style::new().fg(accent_color)),
    ];
    if tb.filter.is_empty() {
        bottom_spans.push(Span::styled("search ", Style::new().fg(theme::MUTED)));
    } else {
        bottom_spans.push(Span::styled(format!("{} ", tb.filter), Style::new().fg(theme::FG)));
    }
    bottom_spans.extend([
        Span::styled("↩", Style::new().fg(accent_color)),
        Span::styled(" select ", Style::new().fg(theme::MUTED)),
        Span::styled("esc", Style::new().fg(accent_color)),
        Span::styled(" close ", Style::new().fg(theme::MUTED)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(title, Style::new().fg(accent_color).add_modifier(Modifier::BOLD))))
        .title_bottom(Line::from(bottom_spans))
        .border_style(Style::new().fg(accent_color))
        .style(Style::new().bg(theme::POPUP_BG));

    let inner = block.inner(dropdown_area);
    frame.render_widget(block, dropdown_area);

    if tb.loading {
        let loading = ratatui::widgets::Paragraph::new("  Loading...")
            .style(Style::new().fg(theme::MUTED));
        frame.render_widget(loading, inner);
        return;
    }

    match kind {
        DropdownKind::Device => {
            let filtered = tb.filtered_devices();
            let items: Vec<ListItem> = filtered
                .iter()
                .map(|d| {
                    let color = if d.connected { theme::FG } else { theme::MUTED };
                    ListItem::new(Line::from(Span::styled(
                        format!("  {}", selector::selector_label(d)),
                        Style::new().fg(color),
                    )))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(Style::new().fg(accent_color).add_modifier(Modifier::BOLD))
                .highlight_symbol(" ▸");
            let clamped = tb.cursor.min(filtered.len().saturating_sub(1));
            let mut state = ListState::default().with_selected(Some(clamped));
            frame.render_stateful_widget(list, inner, &mut state);
        }
        DropdownKind::App => {
            let filtered = tb.filtered_packages();
            let items: Vec<ListItem> = filtered
                .iter()
                .map(|&name| {
                    ListItem::new(Span::styled(
                        format!("  {name}"),
                        Style::new().fg(theme::FG),
                    ))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(Style::new().fg(accent_color).add_modifier(Modifier::BOLD))
                .highlight_symbol(" ▸");
            let clamped = tb.cursor.min(filtered.len().saturating_sub(1));
            let mut state = ListState::default().with_selected(Some(clamped));
            frame.render_stateful_widget(list, inner, &mut state);
        }
    }
}

fn render_dialog(frame: &mut Frame, area: Rect, message: &str) {
    let lines: Vec<&str> = message.lines().collect();
    let width = lines.iter().map(|l| l.len()).max().unwrap_or(20) as u16 + 4;
    let height = lines.len() as u16 + 4;
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let bottom = Line::from(vec![
        Span::styled(" press any key to close ", Style::new().fg(theme::MUTED)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .title_bottom(bottom)
        .style(Style::new().bg(theme::POPUP_BG));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let text: Vec<Line> = lines
        .into_iter()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::new().fg(theme::FG))))
        .collect();
    let paragraph = ratatui::widgets::Paragraph::new(text);
    frame.render_widget(paragraph, inner);
}

fn is_focused(app: &App, panel_number: u8) -> bool {
    app.focused_panel() == Some(panel_number)
}

fn render_panels(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String]) {
    let vis = app.panel_visibility();
    let commands_visible = app.commands().visible;
    let logcat_visible = vis[0];
    let trace_visible = vis[1];
    let permissions_visible = vis[5];
    let mid_visible = trace_visible || permissions_visible;
    let bot_left_visible = vis[2] || vis[3] || vis[4];
    let bot_right_visible = vis[6] || vis[7];
    let bot_visible = bot_left_visible || bot_right_visible;

    let top_visible = commands_visible || logcat_visible;
    let section_count = top_visible as u8 + mid_visible as u8 + bot_visible as u8;
    if section_count == 0 { return; }

    let mut constraints = Vec::new();
    if top_visible {
        if section_count == 1 { constraints.push(Constraint::Min(0)); }
        else if mid_visible { constraints.push(Constraint::Percentage(40)); }
        else { constraints.push(Constraint::Percentage(50)); }
    }
    if mid_visible {
        if !bot_visible && !top_visible { constraints.push(Constraint::Min(0)); }
        else { constraints.push(Constraint::Percentage(15)); }
    }
    if bot_visible { constraints.push(Constraint::Min(0)); }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    if top_visible {
        render_top_section(frame, rows[idx], app, logcat_lines, commands_visible, logcat_visible);
        idx += 1;
    }
    if mid_visible {
        render_mid_section(frame, rows[idx], app, trace_visible, permissions_visible);
        idx += 1;
    }
    if bot_visible {
        render_bottom_section(frame, rows[idx], app);
    }
}

fn render_top_section(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String], commands_visible: bool, logcat_visible: bool) {
    match (commands_visible, logcat_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(28), Constraint::Min(0)])
                .split(area);
            render_commands_panel(frame, cols[0], app);
            logcat_ui::render_logcat_panel(frame, cols[1], is_focused(app, panel::LOGCAT), logcat_lines, app);
        }
        (true, false) => render_commands_panel(frame, area, app),
        (false, true) => logcat_ui::render_logcat_panel(frame, area, is_focused(app, panel::LOGCAT), logcat_lines, app),
        (false, false) => {}
    }
}

fn render_mid_section(frame: &mut Frame, area: Rect, app: &mut App, trace_visible: bool, permissions_visible: bool) {
    match (trace_visible, permissions_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            crate::trace_ui::render_trace_panel(frame, cols[0], is_focused(app, panel::TRACE), app.trace_state());
            permissions_ui::render_permissions_panel(frame, cols[1], is_focused(app, panel::PERMISSIONS), app.permissions_state());
        }
        (true, false) => crate::trace_ui::render_trace_panel(frame, area, is_focused(app, panel::TRACE), app.trace_state()),
        (false, true) => permissions_ui::render_permissions_panel(frame, area, is_focused(app, panel::PERMISSIONS), app.permissions_state()),
        (false, false) => {}
    }
}

fn render_commands_panel(frame: &mut Frame, area: Rect, app: &App) {
    let focused = is_focused(app, panel::COMMANDS);
    let accent = panel::by_number(panel::COMMANDS).bright_color;
    let border_color = panel::by_number(panel::COMMANDS).border_color(focused);

    let mut block = panel_block(panel::COMMANDS, focused);
    if focused {
        let filter = &app.commands().filter;
        let filter_span = if filter.is_empty() {
            Span::styled(" /", Style::new().fg(accent))
        } else {
            Span::styled(format!(" /{filter}"), Style::new().fg(theme::FG))
        };
        block = block.title_bottom(Line::from(vec![
            filter_span,
            Span::styled(" ", Style::new()),
            Span::styled("───", Style::new().fg(border_color)),
            Span::styled(" ↩", Style::new().fg(accent)),
            Span::styled(" run ", Style::new().fg(theme::FG)),
        ]));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let filtered = app.commands().filtered_commands();

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|(name, _)| {
            let is_toggle_on = (*name == "layout bounds" && app.layout_bounds())
                || (*name == "airplane mode" && app.airplane_mode())
                || (*name == "wifi" && app.wifi_enabled())
                || (*name == "dark mode" && app.dark_mode());
            let is_global = *name == "open app" || *name == "kill app";
            let display_name = if *name == "layout bounds" || *name == "wifi" || *name == "airplane mode" || *name == "dark mode" {
                let verb = if is_toggle_on { "disable" } else { "enable" };
                format!("{} {}", verb, name)
            } else {
                name.to_string()
            };
            let mut spans = vec![];
            if is_global {
                let (first, rest) = display_name.split_at(1);
                spans.push(Span::styled(first.to_string(), Style::new().fg(theme::KEY_HINT)));
                spans.push(Span::styled(rest.to_string(), Style::new().fg(theme::FG)));
            } else {
                spans.push(Span::styled(display_name, Style::new().fg(theme::FG)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::new().fg(accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    if focused {
        let cursor = app.commands().cursor.min(filtered.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(cursor));
        frame.render_stateful_widget(list, inner, &mut state);
    } else {
        frame.render_widget(list, inner);
    }
}


fn render_bottom_section(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let left_visible = vis[2] || vis[3] || vis[4];
    let right_visible = vis[6] || vis[7];

    match (left_visible, right_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_bottom_left(frame, cols[0], app);
            render_right_column(frame, cols[1], app);
        }
        (true, false) => render_bottom_left(frame, area, app),
        (false, true) => render_right_column(frame, area, app),
        (false, false) => {}
    }
}

fn render_bottom_left(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let panels: Vec<u8> = [
        (vis[2], panel::FRAMES),
        (vis[3], panel::DISK),
        (vis[4], panel::SYSTEM),
    ]
    .iter()
    .filter(|(v, _)| *v)
    .map(|(_, p)| *p)
    .collect();

    if panels.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = panels.iter().map(|_| Constraint::Ratio(1, panels.len() as u32)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, &p) in panels.iter().enumerate() {
        match p {
            panel::FRAMES => monitor_ui::render_frames_panel(frame, rows[i], false, app.monitor_state()),
            panel::DISK => monitor_ui::render_disk_panel(frame, rows[i], false, app.monitor_state()),
            panel::SYSTEM => monitor_ui::render_system_panel(frame, rows[i], false, app.monitor_state()),
            _ => {}
        }
    }
}

fn render_right_column(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let panels: Vec<u8> = [
        (vis[6], panel::FILES),
        (vis[7], panel::DATABASE),
    ]
    .iter()
    .filter(|(v, _)| *v)
    .map(|(_, p)| *p)
    .collect();

    if panels.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = panels.iter().map(|_| Constraint::Ratio(1, panels.len() as u32)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, &p) in panels.iter().enumerate() {
        match p {
            panel::FILES => {
                files_ui::render_files_panel(frame, rows[i], is_focused(app, panel::FILES), app.files_state());
            }
            panel::DATABASE => {
                database_ui::render_database_panel(frame, rows[i], is_focused(app, panel::DATABASE), app.database_state_mut());
            }
            _ => {}
        }
    }
}

