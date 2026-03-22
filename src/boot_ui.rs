use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::apps;
use crate::boot::BootPhase;
use crate::selector::selector_label;
use crate::theme;

pub fn render_boot(frame: &mut Frame, phase: &BootPhase, tick: u8, confirming_quit: bool) {
    let area = frame.area();

    let hint_line = if confirming_quit {
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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(
            Line::from(" msh ")
                .style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
        )
        .title_bottom(hint_line)
        .border_style(Style::new().fg(theme::SURFACE))
        .style(Style::new().bg(theme::BG).fg(theme::FG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    match phase {
        BootPhase::WaitingForDevice => render_waiting(frame, inner, tick),
        BootPhase::SelectDevice { devices, cursor } => {
            render_select_device(frame, inner, devices, *cursor)
        }
        BootPhase::SelectApp {
            packages,
            cursor,
            filter,
            ..
        } => render_select_app(frame, inner, packages.as_deref(), *cursor, filter),
        BootPhase::Done { .. } => {}
    }
}

fn render_waiting(frame: &mut Frame, area: Rect, tick: u8) {
    let dots = ".".repeat((tick as usize % 4) + 1);
    let text = format!("Waiting for device{dots}");
    let paragraph = Paragraph::new(text)
        .style(Style::new().fg(theme::MUTED))
        .alignment(Alignment::Center);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    frame.render_widget(paragraph, vertical[1]);
}

fn render_select_device(frame: &mut Frame, area: Rect, devices: &[crate::adb::Device], cursor: usize) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let title = Paragraph::new(" Select Device")
        .style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD));
    frame.render_widget(title, rows[0]);

    let items: Vec<ListItem> = devices
        .iter()
        .map(|d| {
            ListItem::new(Span::styled(
                format!("  {}", selector_label(d)),
                Style::new().fg(theme::FG),
            ))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ▸");

    let mut state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(list, rows[1], &mut state);
}

fn render_select_app(
    frame: &mut Frame,
    area: Rect,
    packages: Option<&[String]>,
    cursor: usize,
    filter: &str,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let title = Paragraph::new(" Select App")
        .style(Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD));
    frame.render_widget(title, rows[0]);

    let filter_line = Line::from(vec![
        Span::styled("  filter: ", Style::new().fg(theme::MUTED)),
        Span::styled(filter, Style::new().fg(theme::FG)),
        Span::styled("█", Style::new().fg(theme::ACCENT)),
    ]);
    frame.render_widget(Paragraph::new(filter_line), rows[1]);

    let Some(packages) = packages else {
        let loading = Paragraph::new("  Loading packages...")
            .style(Style::new().fg(theme::MUTED));
        frame.render_widget(loading, rows[3]);
        return;
    };

    let filtered = apps::filtered_packages(packages, filter);
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
        .highlight_style(
            Style::new()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ▸");

    let clamped = cursor.min(filtered.len().saturating_sub(1));
    let mut state = ListState::default().with_selected(Some(clamped));
    frame.render_stateful_widget(list, rows[3], &mut state);
}
