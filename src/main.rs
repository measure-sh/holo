mod adb;
mod boot;
mod theme;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Text,
    widgets::{Block, List, ListItem},
    DefaultTerminal, Frame,
};

use adb::{Adb, Device, RealAdb};
use boot::{BootResult, DeviceSelector};

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb = RealAdb;
    let devices = adb.list_devices()?;

    let device = match boot::resolve_device(devices) {
        BootResult::NoDevices => {
            eprintln!("No devices connected. Connect a device and try again.");
            std::process::exit(1);
        }
        BootResult::Selected(device) => device,
        BootResult::NeedsSelection(devices) => {
            let terminal = ratatui::init();
            let result = run_device_selection(terminal, devices);
            ratatui::restore();
            result?
        }
    };

    eprintln!("Selected device: {}", device.serial);
    // TODO: enter main app with selected device

    Ok(())
}

fn run_device_selection(mut terminal: DefaultTerminal, devices: Vec<Device>) -> Result<Device> {
    let mut selector = DeviceSelector::new(devices);

    loop {
        terminal.draw(|frame| render_device_selection(frame, &mut selector))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    ratatui::restore();
                    std::process::exit(0);
                }
                KeyCode::Down | KeyCode::Char('j') => selector.select_next(),
                KeyCode::Up | KeyCode::Char('k') => selector.select_previous(),
                KeyCode::Enter => return Ok(selector.confirm()),
                _ => {}
            }
        }
    }
}

fn render_device_selection(frame: &mut Frame, selector: &mut DeviceSelector) {
    let area = frame.area();

    let bg = Block::default().style(Style::new().bg(theme::BG));
    frame.render_widget(bg, area);

    let [_, content, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length((selector.devices.len() as u16) + 4),
        Constraint::Fill(1),
    ])
    .areas(area);

    let [_, center, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Percentage(60),
        Constraint::Fill(1),
    ])
    .areas(content);

    let items: Vec<ListItem> = selector
        .devices
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

    frame.render_stateful_widget(list, center, &mut selector.list_state);
}
