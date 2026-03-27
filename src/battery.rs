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
    let t = theme::current();
    if level < 10 {
        t.red
    } else if level <= 25 {
        t.yellow
    } else {
        t.fg
    }
}

pub fn battery_bar(level: u8) -> Line<'static> {
    let t = theme::current();
    const BAR_WIDTH: usize = 10;
    let filled = ((level as usize) * BAR_WIDTH / 100).min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;
    let color = battery_color(level);

    Line::from(vec![
        Span::raw(" "),
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled("░".repeat(empty), Style::new().fg(t.surface)),
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
        let t = theme::current();
        assert_eq!(battery_color(0), t.red);
        assert_eq!(battery_color(9), t.red);
    }

    #[test]
    fn battery_color_yellow_10_to_25() {
        let t = theme::current();
        assert_eq!(battery_color(10), t.yellow);
        assert_eq!(battery_color(25), t.yellow);
    }

    #[test]
    fn battery_color_normal_above_25() {
        let t = theme::current();
        assert_eq!(battery_color(26), t.fg);
        assert_eq!(battery_color(100), t.fg);
    }
}
