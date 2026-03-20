mod adb;
mod app;
mod boot;
mod selector;
mod theme;
mod ui;

use std::sync::mpsc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::Alignment,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
    Frame,
};

use adb::{Adb, Device, RealAdb};
use app::App;
use boot::BootResult;

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
        BootResult::NeedsSelection(devices) => selector::run(devices)?,
    };

    let terminal = ratatui::init();
    let result = run_app(terminal, &adb, &device);
    ratatui::restore();
    result
}

fn spawn_battery_poller(serial: String) -> mpsc::Receiver<u8> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let adb = RealAdb;
        let interval = Duration::from_secs(30);
        loop {
            if let Ok(level) = adb.get_battery_level(&serial) {
                if tx.send(level).is_err() {
                    return; // main thread exited
                }
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

fn run_app(mut terminal: ratatui::DefaultTerminal, _adb: &impl Adb, device: &Device) -> Result<()> {
    let title = format!(" {} ", selector::selector_label(device));
    let mut app = App::new();

    let battery_rx = spawn_battery_poller(device.serial.clone());
    let mut battery_level: Option<u8> = None;

    loop {
        // Drain any battery updates that arrived (non-blocking)
        while let Ok(level) = battery_rx.try_recv() {
            battery_level = Some(level);
        }

        let now = chrono::Local::now();
        let time_str = format!(" {} ", now.format("%H:%M:%S"));
        terminal.draw(|frame| {
            render_app(frame, &title, &time_str, battery_level, app.selected_panel)
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char(c @ '1'..='6') => {
                        app.select_panel(c as u8 - b'0');
                    }
                    _ => {}
                }
            }
        }
    }
}

fn battery_color(level: u8) -> ratatui::style::Color {
    if level < 10 {
        theme::RED
    } else if level <= 25 {
        theme::YELLOW
    } else {
        theme::FG
    }
}

fn battery_bar(level: u8) -> Line<'static> {
    const BAR_WIDTH: usize = 10;
    let filled = ((level as usize) * BAR_WIDTH / 100).min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;
    let color = battery_color(level);

    Line::from(vec![
        Span::raw(" "),
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled("░".repeat(empty), Style::new().fg(theme::SURFACE)),
        Span::styled(format!(" {level}% "), Style::new().fg(color)),
    ])
    .alignment(Alignment::Right)
}

fn render_app(
    frame: &mut Frame,
    title: &str,
    time: &str,
    battery_level: Option<u8>,
    selected_panel: Option<u8>,
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
        Line::from(" 1-6: panel | q/Esc: exit ").style(Style::new().fg(theme::MUTED));

    let mut block = Block::default()
        .borders(Borders::ALL)
        .title(title_line)
        .title(time_line);

    if let Some(level) = battery_level {
        block = block.title(battery_bar(level));
    }

    block = block
        .title_bottom(hint_line)
        .border_style(Style::new().fg(theme::SURFACE))
        .style(Style::new().bg(theme::BG).fg(theme::FG));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    ui::render_panels(frame, inner, selected_panel);
}

