use crossterm::event::KeyCode;

use crate::app::Action;
use crate::apps;

const COMMAND_LIST: &[(&str, fn() -> Action)] = &[
    ("open app", || Action::OpenApp),
    ("kill app", || Action::KillApp),
    ("wakeup device", || Action::WakeScreen),
    ("clear data", || Action::ClearData),
    ("dark mode", || Action::ToggleDarkMode),
    ("take screenshot", || Action::Screenshot),
    ("layout bounds", || Action::ToggleLayoutBounds),
    ("wifi", || Action::ToggleWifi),
    ("airplane mode", || Action::ToggleAirplaneMode),
    ("connect wireless adb", || Action::WirelessAdb),
    ("uninstall app", || Action::UninstallApp),
    ("mirror device", || Action::MirrorDevice),
];

pub struct CommandsState {
    pub visible: bool,
    pub cursor: usize,
    pub filter: String,
}

impl CommandsState {
    pub fn new() -> Self {
        Self {
            visible: true,
            cursor: 0,
            filter: String::new(),
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<Action> {
        match code {
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Down => {
                let count = self.filtered_commands().len();
                self.cursor = (self.cursor + 1).min(count.saturating_sub(1));
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                let filtered = self.filtered_commands();
                if let Some((_, action_fn)) = filtered.get(self.cursor) {
                    Some(action_fn())
                } else {
                    Some(Action::Noop)
                }
            }
            KeyCode::Esc => {
                self.filter.clear();
                self.cursor = 0;
                Some(Action::Unfocus)
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.cursor = 0;
                Some(Action::Noop)
            }
            KeyCode::Char(ch) => {
                self.filter.push(ch);
                self.cursor = 0;
                Some(Action::Noop)
            }
            _ => Some(Action::Noop),
        }
    }

    pub fn filtered_commands(&self) -> Vec<(&'static str, fn() -> Action)> {
        COMMAND_LIST
            .iter()
            .filter(|(name, _)| apps::fuzzy_matches(name, &self.filter))
            .copied()
            .collect()
    }
}
