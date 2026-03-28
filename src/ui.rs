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
use crate::crashes_ui;
use crate::anrs_ui;
use crate::permissions_ui;
use crate::theme;


pub const SUPERSCRIPT_DIGITS: [char; 9] = [
    '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}', '\u{2078}', '\u{2079}',
];

pub fn panel_title(panel_number: u8, _focused: bool) -> Line<'static> {
    let t = theme::current();
    let def = panel::by_number(panel_number);
    let mut spans = Vec::new();

    let superscript = if panel_number == 0 {
        '\u{2070}'
    } else {
        SUPERSCRIPT_DIGITS[(panel_number - 1) as usize]
    };
    spans.push(Span::styled(
        format!(" {}", superscript),
        Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
    ));

    if let Some(key) = def.focus_key {
        if let Some(pos) = def.name.find(key) {
            let before = &def.name[..pos];
            let after = &def.name[pos + key.len_utf8()..];
            if !before.is_empty() {
                spans.push(Span::styled(before.to_string(), Style::new().fg(t.fg)));
            }
            spans.push(Span::styled(
                String::from(key),
                Style::new().fg(t.danger),
            ));
            spans.push(Span::styled(
                format!("{after} "),
                Style::new().fg(t.fg),
            ));
        } else {
            spans.push(Span::styled(
                format!("{} ", def.name),
                Style::new().fg(t.fg),
            ));
        }
    } else {
        spans.push(Span::styled(
            format!("{} ", def.name),
            Style::new().fg(t.fg),
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
    let t = theme::current();
    let area = frame.area();

    let title_line = Line::from(" msh ").style(
        Style::new()
            .fg(t.accent)
            .add_modifier(Modifier::BOLD),
    );

    let mut hint_spans = if app.confirming_quit() {
        vec![
            Span::styled(" q", Style::new().fg(t.danger)),
            Span::styled(" to confirm ", Style::new().fg(t.fg)),
        ]
    } else if app.confirming_settings() {
        vec![
            Span::styled(" s", Style::new().fg(t.danger)),
            Span::styled(" to confirm ", Style::new().fg(t.fg)),
        ]
    } else {
        vec![
            Span::styled(" qq", Style::new().fg(t.danger)),
            Span::styled("uit ", Style::new().fg(t.muted)),
            Span::styled(" ss", Style::new().fg(t.danger)),
            Span::styled("ettings ", Style::new().fg(t.muted)),
        ]
    };
    if app.focused_panel().is_some() {
        let label = if app.is_zoomed() { "oom out " } else { "oom in " };
        hint_spans.push(Span::styled(" z", Style::new().fg(t.danger)));
        hint_spans.push(Span::styled(label, Style::new().fg(t.muted)));
    }
    let hint_line = Line::from(hint_spans);

    let branding_line = if let Some((msg, is_error)) = app.status_flash_active() {
        let color = if is_error { t.danger } else { t.success };
        Line::from(Span::styled(
            format!(" {} ", msg),
            Style::new().fg(color),
        )).alignment(Alignment::Center)
    } else {
        Line::from(vec![
            Span::styled("made with ", Style::new().fg(t.muted)),
            Span::styled("\u{2665}", Style::new().fg(t.danger)),
            Span::styled(" by ", Style::new().fg(t.muted)),
            Span::styled("measure.sh ", Style::new().fg(t.accent)),
        ]).alignment(Alignment::Center)
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title_line);

    if let Some(level) = battery_level {
        block = block.title(battery::battery_bar(level));
    }

    let info_line = Line::from(Span::styled(
        title,
        Style::new().fg(t.muted),
    ))
    .alignment(Alignment::Right);

    block = block
        .title_bottom(hint_line)
        .title_bottom(branding_line)
        .title_bottom(info_line)
        .border_style(Style::new().fg(t.surface))
        .style(Style::new().bg(t.bg).fg(t.fg));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);

    render_toolbar(frame, chunks[0], app);
    render_panels(frame, chunks[1], app, logcat_lines);

    if app.toolbar().open.is_some() {
        render_dim_overlay(frame, area);
        render_dropdown_overlay(frame, chunks[0], app);
    }

    if app.settings_open {
        render_dim_overlay(frame, area);
        render_settings(frame, area, app);
    }

    if let Some(msg) = &app.dialog {
        render_dim_overlay(frame, area);
        render_dialog(frame, area, msg);
    }
}

fn render_toolbar(frame: &mut Frame, area: Rect, app: &App) {
    let t = theme::current();
    let tb = app.toolbar();
    let has_device = tb.device.is_some();
    let has_app = tb.package.is_some();

    let device_label = tb.device_label();
    let app_label = tb.app_label();

    let (device_dot, device_fg) = if has_device && tb.device_connected {
        (t.info, t.fg)
    } else if has_device {
        (t.danger, t.fg)
    } else {
        (t.muted, t.muted)
    };
    let app_dot = if has_app { t.success } else { t.muted };
    let app_fg = if has_app { t.fg } else { t.muted };

    let device_content = format!(" \u{2022} {device_label} \u{25BE} ");
    let app_content = format!(" \u{2022} {app_label} \u{25BE} ");
    let device_w = device_content.chars().count() as u16 + 2;
    let app_w = app_content.chars().count() as u16 + 2;
    let gap = 4u16;
    let total_w = device_w + gap + app_w;
    let x_start = area.x + area.width.saturating_sub(total_w) / 2;

    let device_area = Rect::new(x_start, area.y, device_w, 3);
    let device_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.surface))
        .title(Span::styled(" ^D ", Style::new().fg(t.danger)));
    let device_inner = device_block.inner(device_area);
    frame.render_widget(device_block, device_area);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" \u{2022}"),
                Style::new().fg(device_dot),
            ),
            Span::styled(
                format!(" {device_label} \u{25BE} "),
                Style::new().fg(device_fg),
            ),
        ])),
        device_inner,
    );

    let app_area = Rect::new(x_start + device_w + gap, area.y, app_w, 3);
    let app_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.surface))
        .title(Span::styled(" ^A ", Style::new().fg(t.danger)));
    let app_inner = app_block.inner(app_area);
    frame.render_widget(app_block, app_area);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" \u{2022}"),
                Style::new().fg(app_dot),
            ),
            Span::styled(
                format!(" {app_label} \u{25BE} "),
                Style::new().fg(app_fg),
            ),
        ])),
        app_inner,
    );
}

fn render_dim_overlay(frame: &mut Frame, area: Rect) {
    let t = theme::current();
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_fg(t.surface);
                cell.set_bg(t.overlay);
            }
        }
    }
}

fn render_dropdown_overlay(frame: &mut Frame, toolbar_area: Rect, app: &App) {
    let t = theme::current();
    let tb = app.toolbar();
    let Some(kind) = tb.open else { return };

    let anchor_y = toolbar_area.y + toolbar_area.height;

    let screen = frame.area();
    let (width, max_items) = match kind {
        DropdownKind::Device | DropdownKind::App => (44u16, 16u16),
    };
    let width = width.min(screen.width.saturating_sub(2));
    let available = screen.height.saturating_sub(anchor_y);
    let height = (max_items + 2).min(available).max(5);
    let x = toolbar_area.x + (toolbar_area.width.saturating_sub(width)) / 2;

    let dropdown_area = Rect::new(x, anchor_y, width, height);

    frame.render_widget(Clear, dropdown_area);

    let title = match kind {
        DropdownKind::Device => " select device ",
        DropdownKind::App => " select app ",
    };

    let mut bottom_spans = vec![
        Span::styled(" /", Style::new().fg(t.danger)),
    ];
    if tb.filter.is_empty() {
        bottom_spans.push(Span::styled("search ", Style::new().fg(t.muted)));
    } else {
        bottom_spans.push(Span::styled(format!("{} ", tb.filter), Style::new().fg(t.fg)));
    }
    bottom_spans.extend([
        Span::styled("↩", Style::new().fg(t.danger)),
        Span::styled(" select ", Style::new().fg(t.muted)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(title, Style::new().fg(t.fg))))
        .title_bottom(Line::from(bottom_spans))
        .border_style(Style::new().fg(t.surface))
        .style(Style::new().bg(t.bg));

    let inner = block.inner(dropdown_area);
    frame.render_widget(block, dropdown_area);

    if tb.loading {
        let loading = ratatui::widgets::Paragraph::new("  Loading...")
            .style(Style::new().fg(t.muted));
        frame.render_widget(loading, inner);
        return;
    }

    match kind {
        DropdownKind::Device => {
            let filtered = tb.filtered_devices();
            let items: Vec<ListItem> = filtered
                .iter()
                .map(|d| {
                    let color = if d.connected { t.fg } else { t.muted };
                    ListItem::new(Line::from(Span::styled(
                        format!("  {}", selector::selector_label(d)),
                        Style::new().fg(color),
                    )))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(Style::new().fg(t.accent).add_modifier(Modifier::BOLD))
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
                        Style::new().fg(t.fg),
                    ))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(Style::new().fg(t.accent).add_modifier(Modifier::BOLD))
                .highlight_symbol(" ▸");
            let clamped = tb.cursor.min(filtered.len().saturating_sub(1));
            let mut state = ListState::default().with_selected(Some(clamped));
            frame.render_stateful_widget(list, inner, &mut state);
        }
    }
}

fn render_settings(frame: &mut Frame, area: Rect, app: &App) {
    let t = theme::current();
    let cursor = app.settings_cursor;

    let width = 44u16.min(area.width.saturating_sub(4));
    let height = 6u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let border_fg = Style::new().fg(t.surface);
    let bottom = Line::from(vec![
        Span::styled(" j/k", Style::new().fg(t.danger)),
        Span::styled(" navigate ", Style::new().fg(t.muted)),
        Span::styled("───", border_fg),
        Span::styled(" \u{2190}\u{2192}", Style::new().fg(t.danger)),
        Span::styled(" change ", Style::new().fg(t.muted)),
        Span::styled("───", border_fg),
        Span::styled(" \u{21b5}", Style::new().fg(t.danger)),
        Span::styled(" open ", Style::new().fg(t.muted)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(" settings ", Style::new().fg(t.fg)))
        .title_bottom(bottom)
        .border_style(border_fg)
        .style(Style::new().bg(t.bg));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let name = theme::current().name;
    let rows = [
        Line::from(vec![
            Span::styled(" Theme            ", Style::new().fg(t.fg)),
            Span::styled(format!("\u{25c2} {name} \u{25b8}"), Style::new().fg(t.accent)),
        ]),
        Line::from(vec![
            Span::styled(" Downloads        ", Style::new().fg(t.fg)),
            Span::styled("open folder", Style::new().fg(t.muted)),
        ]),
        Line::from(vec![
            Span::styled(" Keymap           ", Style::new().fg(t.fg)),
            Span::styled("view shortcuts", Style::new().fg(t.muted)),
        ]),
        Line::from(vec![
            Span::styled(" About            ", Style::new().fg(t.fg)),
            Span::styled("open in browser \u{2197}", Style::new().fg(t.muted)),
        ]),
    ];

    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    let list = List::new(items)
        .highlight_style(Style::new().fg(t.accent).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(list, inner, &mut state);
}

fn render_dialog(frame: &mut Frame, area: Rect, message: &str) {
    let t = theme::current();
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
        Span::styled(" press any key to close ", Style::new().fg(t.muted)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.surface))
        .title_bottom(bottom)
        .style(Style::new().bg(t.bg));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let text: Vec<Line> = lines
        .into_iter()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::new().fg(t.fg))))
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
    let disk_visible = vis[1];
    let system_visible = vis[2];
    let permissions_visible = vis[3];
    let monitor_visible = disk_visible || system_visible || permissions_visible;
    let trace_visible = vis[4];
    let crashes_visible = vis[5];
    let anrs_visible = vis[6];
    let mid_visible = trace_visible || crashes_visible || anrs_visible;
    let bot_visible = vis[7] || vis[8];

    let top_visible = commands_visible || logcat_visible;
    let section_count = top_visible as u8 + monitor_visible as u8 + mid_visible as u8 + bot_visible as u8;
    if section_count == 0 { return; }

    let mut weights: Vec<u32> = Vec::new();
    if top_visible { weights.push(5); }
    if monitor_visible { weights.push(2); }
    if mid_visible { weights.push(2); }
    if bot_visible { weights.push(2); }
    let total: u32 = weights.iter().sum();
    let constraints: Vec<Constraint> = weights
        .iter()
        .map(|&w| Constraint::Ratio(w, total))
        .collect();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    if top_visible {
        render_top_section(frame, rows[idx], app, logcat_lines, commands_visible, logcat_visible);
        idx += 1;
    }
    if monitor_visible {
        render_monitor_section(frame, rows[idx], app, disk_visible, system_visible, permissions_visible);
        idx += 1;
    }
    if mid_visible {
        render_mid_section(frame, rows[idx], app, trace_visible, crashes_visible, anrs_visible);
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

fn render_mid_section(frame: &mut Frame, area: Rect, app: &mut App, trace_visible: bool, crashes_visible: bool, anrs_visible: bool) {
    let panels: Vec<u8> = [
        (trace_visible, panel::TRACE),
        (crashes_visible, panel::CRASHES),
        (anrs_visible, panel::ANRS),
    ]
    .iter()
    .filter(|(v, _)| *v)
    .map(|(_, p)| *p)
    .collect();

    if panels.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = panels.iter().map(|_| Constraint::Ratio(1, panels.len() as u32)).collect();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, &p) in panels.iter().enumerate() {
        match p {
            panel::TRACE => crate::trace_ui::render_trace_panel(frame, cols[i], is_focused(app, panel::TRACE), app.trace_state()),
            panel::CRASHES => crashes_ui::render_crashes_panel(frame, cols[i], is_focused(app, panel::CRASHES), app.crashes_state()),
            panel::ANRS => anrs_ui::render_anrs_panel(frame, cols[i], is_focused(app, panel::ANRS), app.anrs_state()),
            _ => {}
        }
    }
}

fn render_commands_panel(frame: &mut Frame, area: Rect, app: &App) {
    let t = theme::current();
    let focused = is_focused(app, panel::COMMANDS);
    let border_color = panel::by_number(panel::COMMANDS).border_color(focused);

    let mut block = panel_block(panel::COMMANDS, focused);
    if focused {
        let filter = &app.commands().filter;
        let filter_span = if filter.is_empty() {
            Span::styled(" /", Style::new().fg(t.danger))
        } else {
            Span::styled(format!(" /{filter}"), Style::new().fg(t.fg))
        };
        block = block.title_bottom(Line::from(vec![
            filter_span,
            Span::styled(" ", Style::new()),
            Span::styled("───", Style::new().fg(border_color)),
            Span::styled(" ↩", Style::new().fg(t.danger)),
            Span::styled(" run ", Style::new().fg(t.fg)),
        ]));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let filtered = app.commands().filtered_commands();

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|(name, shortcut, _)| {
            let hint = format!("^{}", shortcut);
            let triggered = app.commands().triggered_color(*shortcut);
            let name_color = triggered.unwrap_or(t.fg);
            let hint_color = triggered.unwrap_or(t.muted);
            let spans = vec![
                Span::styled(*name, Style::new().fg(name_color)),
                Span::raw("  "),
                Span::styled(hint, Style::new().fg(hint_color)),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::new().fg(t.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    if focused {
        let cursor = app.commands().cursor.min(filtered.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(cursor));
        frame.render_stateful_widget(list, inner, &mut state);
    } else {
        frame.render_widget(list, inner);
    }
}


fn render_monitor_section(frame: &mut Frame, area: Rect, app: &mut App, disk_visible: bool, system_visible: bool, permissions_visible: bool) {
    let panels: Vec<u8> = [
        (disk_visible, panel::DISK),
        (system_visible, panel::SYSTEM),
        (permissions_visible, panel::PERMISSIONS),
    ]
    .iter()
    .filter(|(v, _)| *v)
    .map(|(_, p)| *p)
    .collect();

    if panels.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = panels.iter().map(|_| Constraint::Ratio(1, panels.len() as u32)).collect();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, &p) in panels.iter().enumerate() {
        match p {
            panel::DISK => monitor_ui::render_disk_panel(frame, cols[i], false, app.monitor_state()),
            panel::SYSTEM => monitor_ui::render_system_panel(frame, cols[i], false, app.monitor_state()),
            panel::PERMISSIONS => permissions_ui::render_permissions_panel(frame, cols[i], is_focused(app, panel::PERMISSIONS), app.permissions_state()),
            _ => {}
        }
    }
}

fn render_bottom_section(frame: &mut Frame, area: Rect, app: &mut App) {
    let vis = app.panel_visibility();
    let files_visible = vis[7];
    let database_visible = vis[8];

    match (files_visible, database_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            files_ui::render_files_panel(frame, cols[0], is_focused(app, panel::FILES), app.files_state());
            database_ui::render_database_panel(frame, cols[1], is_focused(app, panel::DATABASE), app.database_state_mut());
        }
        (true, false) => files_ui::render_files_panel(frame, area, is_focused(app, panel::FILES), app.files_state()),
        (false, true) => database_ui::render_database_panel(frame, area, is_focused(app, panel::DATABASE), app.database_state_mut()),
        (false, false) => {}
    }
}
