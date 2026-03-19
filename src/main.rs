mod adb;
mod theme;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Text,
    widgets::{Block, List, ListItem, ListState},
    DefaultTerminal, Frame,
};

use adb::{Adb, Device, RealAdb};

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb = RealAdb;
    let devices = adb.list_devices()?;

    let device = match devices.len() {
        0 => {
            eprintln!("No devices connected. Connect a device and try again.");
            std::process::exit(1);
        }
        1 => devices.into_iter().next().unwrap(),
        _ => {
            let terminal = ratatui::init();
            let result = select_device(terminal, &devices);
            ratatui::restore();
            result?
        }
    };

    eprintln!("Selected device: {}", device.serial);
    // TODO: enter main app with selected device

    Ok(())
}

fn select_device(mut terminal: DefaultTerminal, devices: &[Device]) -> Result<Device> {
    let mut list_state = ListState::default();
    list_state.select(Some(0));

    loop {
        terminal.draw(|frame| render_device_selection(frame, devices, &mut list_state))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    ratatui::restore();
                    std::process::exit(0);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let i = list_state.selected().unwrap_or(0);
                    list_state.select(Some((i + 1).min(devices.len() - 1)));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = list_state.selected().unwrap_or(0);
                    list_state.select(Some(i.saturating_sub(1)));
                }
                KeyCode::Enter => {
                    let i = list_state.selected().unwrap_or(0);
                    return Ok(devices[i].clone());
                }
                _ => {}
            }
        }
    }
}

fn render_device_selection(frame: &mut Frame, devices: &[Device], list_state: &mut ListState) {
    let area = frame.area();

    let bg = Block::default().style(Style::new().bg(theme::BG));
    frame.render_widget(bg, area);

    let [_, content, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length((devices.len() as u16) + 4),
        Constraint::Fill(1),
    ])
    .areas(area);

    let [_, center, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Percentage(60),
        Constraint::Fill(1),
    ])
    .areas(content);

    let items: Vec<ListItem> = devices
        .iter()
        .map(|d| {
            let label = if d.description.is_empty() {
                d.serial.clone()
            } else {
                format!("{} ({})", d.serial, d.description)
            };
            ListItem::new(Text::from(label))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Select Device ")
                .title_style(Style::new().fg(theme::ACCENT))
                .border_style(Style::new().fg(theme::SURFACE))
                .style(Style::new().bg(theme::BG)),
        )
        .style(Style::new().fg(theme::FG))
        .highlight_style(
            Style::new()
                .fg(theme::BG)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, center, list_state);
}
