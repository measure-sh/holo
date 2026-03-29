use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;

use crate::app::Action;
use crate::apps;
use crate::theme;

type CommandEntry = (&'static str, char, fn() -> Action);

const COMMAND_LIST: &[CommandEntry] = &[
    ("open app", 'o', || Action::OpenApp),
    ("kill app", 'k', || Action::KillApp),
    ("clear data", 'x', || Action::ClearData),
    ("wakeup", 'w', || Action::WakeScreen),
    ("screenshot", 's', || Action::Screenshot),
    ("mirror", 'r', || Action::MirrorDevice),
    ("dark mode", 'n', || Action::ToggleDarkMode),
    ("layout bounds", 'l', || Action::ToggleLayoutBounds),
    ("show taps", 't', || Action::ToggleShowTaps),
    ("pointer location", 'p', || Action::TogglePointerLocation),
    ("gpu rendering", 'g', || Action::ToggleGpuRendering),
    ("wifi", 'f', || Action::ToggleWifi),
    ("airplane mode", 'e', || Action::ToggleAirplaneMode),
    ("wireless adb", 'b', || Action::WirelessAdb),
    ("uninstall", 'u', || Action::UninstallApp),
];

pub struct CommandsState {
    pub visible: bool,
    pub cursor: usize,
    pub filter: String,
    pub is_emulator: bool,
    pub triggered: Option<(char, std::time::Instant)>,
}

impl CommandsState {
    pub fn new() -> Self {
        Self {
            visible: true,
            cursor: 0,
            filter: String::new(),
            is_emulator: false,
            triggered: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && let KeyCode::Char(ch) = key.code
        {
            return self.action_for_shortcut(ch);
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
                if let Some((_, shortcut, action_fn)) = filtered.get(self.cursor) {
                    self.triggered = Some((*shortcut, std::time::Instant::now()));
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

    pub fn action_for_shortcut(&mut self, ch: char) -> Option<Action> {
        let ch = ch.to_ascii_lowercase();
        COMMAND_LIST
            .iter()
            .find(|(name, key, _)| {
                *key == ch
                    && !(self.is_emulator && *name == "mirror")
            })
            .map(|(_, key, action_fn)| {
                self.triggered = Some((*key, std::time::Instant::now()));
                action_fn()
            })
    }

    pub fn triggered_color(&self, shortcut: char) -> Option<Color> {
        const HOLD_MS: u128 = 1400;
        const FADE_MS: u128 = 600;
        const TOTAL_MS: u128 = HOLD_MS + FADE_MS;
        let (ch, t) = self.triggered.as_ref()?;
        if *ch != shortcut {
            return None;
        }
        let elapsed = t.elapsed().as_millis();
        if elapsed >= TOTAL_MS {
            return None;
        }
        let th = theme::current();
        if elapsed < HOLD_MS {
            return Some(th.success);
        }
        let ratio = (elapsed - HOLD_MS) as f32 / FADE_MS as f32;
        let (Color::Rgb(sr, sg, sb), Color::Rgb(er, eg, eb)) = (th.success, th.fg) else {
            return None;
        };
        let lerp = |s: u8, e: u8| -> u8 {
            (s as f32 + (e as f32 - s as f32) * ratio) as u8
        };
        Some(Color::Rgb(lerp(sr, er), lerp(sg, eg), lerp(sb, eb)))
    }

    pub fn has_active_animation(&self) -> bool {
        self.triggered
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_millis() < 2000)
    }

    pub fn filtered_commands(&self) -> Vec<CommandEntry> {
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
