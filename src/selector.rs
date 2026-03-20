use std::io::{self, Write};

use color_eyre::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    style::{self, Attribute, Color, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};

use crate::adb::Device;
use crate::theme;

fn device_label(d: &Device) -> String {
    match (&d.model, &d.device) {
        (Some(model), Some(device)) => format!("{model} ({device})"),
        (Some(model), None) => model.clone(),
        (None, Some(device)) => device.clone(),
        (None, None) => d.serial.clone(),
    }
}

pub fn selector_label(d: &Device) -> String {
    let detail = device_label(d);
    if detail == d.serial {
        d.serial.clone()
    } else {
        format!("{}: {detail}", d.serial)
    }
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

pub fn run(devices: Vec<Device>) -> Result<Device> {
    let mut stderr = io::stderr();
    let mut selected: usize = 0;
    let total = devices.len();
    let total_lines = (total + 2) as u16;

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

    w.queue(SetForegroundColor(accent))?
        .queue(style::Print(" Select Device"))?
        .queue(SetForegroundColor(Color::Reset))?
        .queue(style::Print("\r\n"))?;

    for (i, device) in devices.iter().enumerate() {
        let label = selector_label(device);
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

    w.queue(SetForegroundColor(muted))?
        .queue(style::Print(" j/k to move, Enter to select, q to quit"))?
        .queue(SetForegroundColor(Color::Reset))?
        .queue(style::Print("\r\n"))?;

    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::Device;

    fn make_device(serial: &str, model: Option<&str>, device: Option<&str>) -> Device {
        Device {
            serial: serial.to_string(),
            model: model.map(String::from),
            device: device.map(String::from),
        }
    }

    #[test]
    fn device_label_model_and_device() {
        let d = make_device("ABC", Some("Pixel 7"), Some("panther"));
        assert_eq!(device_label(&d), "Pixel 7 (panther)");
    }

    #[test]
    fn device_label_model_only() {
        let d = make_device("ABC", Some("Pixel 7"), None);
        assert_eq!(device_label(&d), "Pixel 7");
    }

    #[test]
    fn device_label_device_only() {
        let d = make_device("ABC", None, Some("panther"));
        assert_eq!(device_label(&d), "panther");
    }

    #[test]
    fn device_label_serial_fallback() {
        let d = make_device("ABC123", None, None);
        assert_eq!(device_label(&d), "ABC123");
    }

    #[test]
    fn selector_label_with_detail() {
        let d = make_device("ABC123", Some("Pixel 7"), None);
        assert_eq!(selector_label(&d), "ABC123: Pixel 7");
    }

    #[test]
    fn selector_label_serial_only() {
        let d = make_device("ABC123", None, None);
        assert_eq!(selector_label(&d), "ABC123");
    }
}
