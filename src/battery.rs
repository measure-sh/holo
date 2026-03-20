use std::sync::{mpsc, Arc};
use std::time::Duration;

use ratatui::{
    layout::Alignment,
    style::Style,
    text::{Line, Span},
};

use crate::adb::Adb;
use crate::theme;

fn battery_color(level: u8) -> ratatui::style::Color {
    if level < 10 {
        theme::RED
    } else if level <= 25 {
        theme::YELLOW
    } else {
        theme::FG
    }
}

pub fn battery_bar(level: u8) -> Line<'static> {
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

pub fn spawn_poller(adb: Arc<dyn Adb>, serial: String) -> mpsc::Receiver<u8> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(30);
        loop {
            if let Ok(level) = adb.get_battery_level(&serial) {
                if tx.send(level).is_err() {
                    return;
                }
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_color_red_below_10() {
        assert_eq!(battery_color(0), theme::RED);
        assert_eq!(battery_color(9), theme::RED);
    }

    #[test]
    fn battery_color_yellow_10_to_25() {
        assert_eq!(battery_color(10), theme::YELLOW);
        assert_eq!(battery_color(25), theme::YELLOW);
    }

    #[test]
    fn battery_color_normal_above_25() {
        assert_eq!(battery_color(26), theme::FG);
        assert_eq!(battery_color(100), theme::FG);
    }
}
