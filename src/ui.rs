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

    if panel_number >= 1 {
        let superscript = SUPERSCRIPT_DIGITS[(panel_number - 1) as usize];
        let digit_color = if focused { color } else { theme::MUTED };
        spans.push(Span::styled(
            format!(" {}", superscript),
            Style::new().fg(digit_color).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(" ", Style::new()));
    }

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
    _time: &str,
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
    let mut hint_spans = Vec::new();
    hint_spans.extend(quit_spans);
    hint_spans.push(Span::styled("───", Style::new().fg(theme::SURFACE)));
    hint_spans.push(Span::styled(" /", Style::new().fg(theme::KEY_HINT)));
    hint_spans.push(Span::styled("commands ", Style::new().fg(theme::MUTED)));
    let hint_line = Line::from(hint_spans);

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
        render_dropdown_overlay(frame, chunks[0], app);
    } else if app.palette().open {
        render_command_palette(frame, area, app);
    }
}

fn render_toolbar(frame: &mut Frame, area: Rect, app: &App) {
    let tb = app.toolbar();
    let has_device = tb.device.is_some();
    let has_app = tb.package.is_some();

    let device_label = tb.device_label();
    let app_label = tb.app_label();

    let device_dot = if has_device { theme::CYAN } else { theme::MUTED };
    let device_fg = if has_device { theme::FG } else { theme::MUTED };
    let device_bg = if has_device { theme::DIM_CYAN } else { theme::SURFACE };
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

fn render_dropdown_overlay(frame: &mut Frame, toolbar_area: Rect, app: &App) {
    let tb = app.toolbar();
    let Some(kind) = tb.open else { return };

    // "F1 " = 3, " ● " = 3, "{label} ▾ " = len+3, "      " = 6, "F2 " = 3, " ● " = 3, "{label} ▾ " = len+3
    let device_label = tb.device_label();
    let app_label = tb.app_label();
    let device_pill_width: u16 = 3 + device_label.len() as u16 + 3;
    let app_pill_width: u16 = 3 + app_label.len() as u16 + 3;
    let total_width: u16 = 3 + device_pill_width + 6 + 3 + app_pill_width;
    let left_pad = toolbar_area.x + (toolbar_area.width.saturating_sub(total_width)) / 2;

    let anchor_x = match kind {
        DropdownKind::Device => left_pad + 3,
        DropdownKind::App => left_pad + 3 + device_pill_width + 6 + 3,
    };
    let anchor_y = toolbar_area.y + 1;

    let screen = frame.area();
    let width = 50.min(screen.width.saturating_sub(2));
    let max_height = screen.height.saturating_sub(anchor_y).min(20);
    let height = max_height.max(5);

    let dropdown_area = Rect::new(
        anchor_x.min(screen.width.saturating_sub(width)),
        anchor_y,
        width,
        height,
    );

    frame.render_widget(Clear, dropdown_area);

    let (title, accent_color, border_color) = match kind {
        DropdownKind::Device => (" devices ", theme::CYAN, theme::DIM_CYAN),
        DropdownKind::App => (" apps ", theme::GREEN, theme::DIM_GREEN),
    };

    let filter_span = if !tb.filter.is_empty() {
        format!(" /{}", tb.filter)
    } else {
        String::new()
    };

    let bottom_spans = vec![
        Span::styled(&filter_span, Style::new().fg(theme::YELLOW)),
        Span::styled(" ↩", Style::new().fg(accent_color)),
        Span::styled(" select ", Style::new().fg(theme::FG)),
        Span::styled("───", Style::new().fg(border_color)),
        Span::styled(" esc", Style::new().fg(accent_color)),
        Span::styled(" close ", Style::new().fg(theme::FG)),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(title, Style::new().fg(accent_color))))
        .title_bottom(Line::from(bottom_spans))
        .border_style(Style::new().fg(border_color))
        .style(Style::new().bg(theme::SURFACE));

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
                    ListItem::new(Line::from(Span::styled(
                        format!("  {}", selector::selector_label(d)),
                        Style::new().fg(theme::FG),
                    )))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(Style::new().fg(theme::CYAN).add_modifier(Modifier::BOLD))
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
                .highlight_style(Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD))
                .highlight_symbol(" ▸");
            let clamped = tb.cursor.min(filtered.len().saturating_sub(1));
            let mut state = ListState::default().with_selected(Some(clamped));
            frame.render_stateful_widget(list, inner, &mut state);
        }
    }
}

fn is_focused(app: &App, panel_number: u8) -> bool {
    app.focused_panel() == Some(panel_number)
}

fn render_panels(frame: &mut Frame, area: Rect, app: &mut App, logcat_lines: &[String]) {
    let vis = app.panel_visibility();
    let logcat_visible = vis[0];
    let trace_visible = vis[1];
    let permissions_visible = vis[5];
    let mid_visible = trace_visible || permissions_visible;
    let bot_left_visible = vis[2] || vis[3] || vis[4];
    let bot_right_visible = vis[6] || vis[7];
    let bot_visible = bot_left_visible || bot_right_visible;

    let section_count = logcat_visible as u8 + mid_visible as u8 + bot_visible as u8;
    if section_count == 0 { return; }

    let mut constraints = Vec::new();
    if logcat_visible {
        if section_count == 1 { constraints.push(Constraint::Min(0)); }
        else { constraints.push(Constraint::Percentage(40)); }
    }
    if mid_visible {
        if !bot_visible { constraints.push(Constraint::Min(0)); }
        else { constraints.push(Constraint::Percentage(15)); }
    }
    if bot_visible { constraints.push(Constraint::Min(0)); }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    if logcat_visible {
        logcat_ui::render_logcat_panel(frame, rows[idx], is_focused(app, panel::LOGCAT), logcat_lines, app);
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

fn render_mid_section(frame: &mut Frame, area: Rect, app: &mut App, trace_visible: bool, permissions_visible: bool) {
    match (trace_visible, permissions_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_trace_panel(frame, cols[0], app);
            permissions_ui::render_permissions_panel(frame, cols[1], is_focused(app, panel::PERMISSIONS), app.permissions_state());
        }
        (true, false) => render_trace_panel(frame, area, app),
        (false, true) => permissions_ui::render_permissions_panel(frame, area, is_focused(app, panel::PERMISSIONS), app.permissions_state()),
        (false, false) => {}
    }
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
                let im = app.input_mode();
                database_ui::render_database_panel(frame, rows[i], is_focused(app, panel::DATABASE), app.db_state_mut(), im);
            }
            _ => {}
        }
    }
}

fn render_command_palette(frame: &mut Frame, area: Rect, app: &App) {
    let palette = app.palette();
    let filtered = palette.filtered_commands();

    let width = area.width.saturating_sub(10).max(40);
    let height = (filtered.len() as u16 + 5).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    let dim = ratatui::widgets::Block::default().style(Style::new().bg(theme::BG).fg(theme::MUTED));
    frame.render_widget(dim, area);
    frame.render_widget(Clear, dialog_area);

    let title = Line::from(Span::styled(" commands ", Style::new().fg(theme::ACCENT)));

    let filter_span = if palette.filter.is_empty() {
        Span::styled(" /", Style::new().fg(theme::ACCENT))
    } else {
        Span::styled(format!(" /{}", palette.filter), Style::new().fg(theme::YELLOW))
    };

    let bottom_spans = vec![
        filter_span,
        Span::styled(" ", Style::new()),
        Span::styled("───", Style::new().fg(theme::MUTED)),
        Span::styled(" ↩", Style::new().fg(theme::ACCENT)),
        Span::styled(" run ", Style::new().fg(theme::FG)),
        Span::styled("───", Style::new().fg(theme::MUTED)),
        Span::styled(" esc", Style::new().fg(theme::ACCENT)),
        Span::styled(" close ", Style::new().fg(theme::FG)),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_bottom(Line::from(bottom_spans))
        .border_style(Style::new().fg(theme::MUTED))
        .style(Style::new().bg(theme::SURFACE));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|(name, _)| {
            let is_toggle_on = (*name == "toggle layout bounds" && app.layout_bounds())
                || (*name == "toggle airplane mode" && app.airplane_mode())
                || (*name == "toggle wifi" && app.wifi_enabled());
            let prefix = if is_toggle_on { "• " } else { "  " };
            let prefix_color = if is_toggle_on { theme::CYAN } else { theme::FG };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::new().fg(prefix_color)),
                Span::styled(*name, Style::new().fg(theme::FG)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol(" ▸");

    let clamped = palette.cursor.min(filtered.len().saturating_sub(1));
    let mut state = ListState::default().with_selected(Some(clamped));
    frame.render_stateful_widget(list, inner, &mut state);
}
