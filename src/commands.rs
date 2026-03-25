use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;
use crate::apps;

const COMMAND_LIST: &[(&str, char, fn() -> Action)] = &[
    ("open app", 'o', || Action::OpenApp),
    ("kill app", 'k', || Action::KillApp),
    ("wakeup", 'w', || Action::WakeScreen),
    ("mirror", 'r', || Action::MirrorDevice),
    ("clear data", 'x', || Action::ClearData),
    ("dark mode", 'n', || Action::ToggleDarkMode),
    ("screenshot", 's', || Action::Screenshot),
    ("layout bounds", 'l', || Action::ToggleLayoutBounds),
    ("wifi", 'f', || Action::ToggleWifi),
    ("airplane mode", 'i', || Action::ToggleAirplaneMode),
    ("wireless adb", 'b', || Action::WirelessAdb),
    ("uninstall", 'u', || Action::UninstallApp),
    ("show taps", 't', || Action::ToggleShowTaps),
    ("pointer location", 'p', || Action::TogglePointerLocation),
    ("gpu rendering", 'g', || Action::ToggleGpuRendering),
];

pub struct CommandsState {
    pub visible: bool,
    pub cursor: usize,
    pub filter: String,
    pub is_emulator: bool,
}

impl CommandsState {
    pub fn new() -> Self {
        Self {
            visible: true,
            cursor: 0,
            filter: String::new(),
            is_emulator: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char(ch) = key.code {
                return self.action_for_shortcut(ch);
            }
        }
        match key.code {
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
                if let Some((_, _, action_fn)) = filtered.get(self.cursor) {
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

    pub fn action_for_shortcut(&self, ch: char) -> Option<Action> {
        let ch = ch.to_ascii_lowercase();
        COMMAND_LIST
            .iter()
            .find(|(name, key, _)| {
                *key == ch
                    && !(self.is_emulator && *name == "mirror")
            })
            .map(|(_, _, action_fn)| action_fn())
    }

    pub fn filtered_commands(&self) -> Vec<(&'static str, char, fn() -> Action)> {
        COMMAND_LIST
            .iter()
            .filter(|(name, _, _)| {
                if self.is_emulator && *name == "mirror" {
                    return false;
                }
                apps::fuzzy_matches(name, &self.filter)
            })
            .copied()
            .collect()
    }
}
