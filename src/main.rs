mod adb;
mod boot;
mod theme;

use std::io::{self, Write};

use color_eyre::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    style::{self, Attribute, Color, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};
use ratatui::{
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders},
    Frame,
};

use adb::{Adb, Device, RealAdb};
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
        BootResult::NeedsSelection(devices) => run_device_selection(devices)?,
    };

    let terminal = ratatui::init();
    let result = run_app(terminal, &device);
    ratatui::restore();
    result
}

fn device_label(d: &Device) -> String {
    match (&d.model, &d.device) {
        (Some(model), Some(device)) => format!("{model} ({device})"),
        (Some(model), None) => model.clone(),
        (None, Some(device)) => device.clone(),
        (None, None) => d.serial.clone(),
    }
}

fn run_app(mut terminal: ratatui::DefaultTerminal, device: &Device) -> Result<()> {
    let title = format!(" {} ", device_label(device));

    loop {
        terminal.draw(|frame| render_app(frame, &title))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                _ => {}
            }
        }
    }
}

fn render_app(frame: &mut Frame, title: &str) {
    let area = frame.area();

    let title_line = Line::from(title).style(
        Style::new()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );
    let hint_line = Line::from(" q/Esc to exit ").style(Style::new().fg(theme::MUTED));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_line)
        .title_bottom(hint_line)
        .border_style(Style::new().fg(theme::SURFACE))
        .style(Style::new().bg(theme::BG).fg(theme::FG));

    frame.render_widget(block, area);
}

fn to_crossterm_color(c: ratatui::style::Color) -> Color {
    match c {
        ratatui::style::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
        _ => Color::Reset,
    }
}

fn move_to_top(w: &mut io::Stderr, total_lines: u16) -> Result<()> {
    w.queue(cursor::MoveUp(total_lines))?
        .queue(terminal::Clear(ClearType::FromCursorDown))?;
    Ok(())
}

fn run_device_selection(devices: Vec<Device>) -> Result<Device> {
    let mut stderr = io::stderr();
    let mut selected: usize = 0;
    let total = devices.len();
    let total_lines = (total + 2) as u16; // header + devices + footer

    terminal::enable_raw_mode()?;
    stderr.execute(cursor::Hide)?;

    render_selector(&mut stderr, &devices, selected)?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    move_to_top(&mut stderr, total_lines)?;
                    stderr.execute(cursor::Show)?;
                    terminal::disable_raw_mode()?;
                    std::process::exit(0);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1).min(total - 1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                }
                KeyCode::Enter => {
                    let device = devices[selected].clone();
                    move_to_top(&mut stderr, total_lines)?;
                    stderr.execute(cursor::Show)?;
                    terminal::disable_raw_mode()?;
                    return Ok(device);
                }
                _ => continue,
            }

            move_to_top(&mut stderr, total_lines)?;
            render_selector(&mut stderr, &devices, selected)?;
        }
    }
}

fn render_selector(w: &mut io::Stderr, devices: &[Device], selected: usize) -> Result<()> {
    let accent = to_crossterm_color(theme::ACCENT);
    let fg = to_crossterm_color(theme::FG);
    let bg = to_crossterm_color(theme::BG);
    let muted = to_crossterm_color(theme::MUTED);

    // Header
    w.queue(SetForegroundColor(accent))?
        .queue(style::Print(" Select Device"))?
        .queue(SetForegroundColor(Color::Reset))?
        .queue(style::Print("\r\n"))?;

    // Device list
    for (i, device) in devices.iter().enumerate() {
        let label = device_label(device);
        if i == selected {
            w.queue(SetBackgroundColor(accent))?
                .queue(SetForegroundColor(bg))?
                .queue(SetAttribute(Attribute::Bold))?
                .queue(style::Print(format!(" ▶ {label}")))?
                .queue(SetAttribute(Attribute::Reset))?
                .queue(SetForegroundColor(Color::Reset))?
                .queue(SetBackgroundColor(Color::Reset))?;
        } else {
            w.queue(SetForegroundColor(fg))?
                .queue(style::Print(format!("   {label}")))?
                .queue(SetForegroundColor(Color::Reset))?;
        }
        w.queue(style::Print("\r\n"))?;
    }

    // Footer hint
    w.queue(SetForegroundColor(muted))?
        .queue(style::Print(" j/k to move, Enter to select, q to quit"))?
        .queue(SetForegroundColor(Color::Reset))?
        .queue(style::Print("\r\n"))?;

    w.flush()?;
    Ok(())
}
