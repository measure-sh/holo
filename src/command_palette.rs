use crossterm::event::KeyCode;

use crate::app::Action;
use crate::apps;

pub struct Command {
    pub name: &'static str,
    pub action: Action,
}

const COMMANDS: &[(&str, fn() -> Action)] = &[
    ("open app", || Action::OpenApp),
    ("wake screen", || Action::WakeScreen),
    ("kill app", || Action::KillApp),
    ("clear data", || Action::ClearData),
    ("uninstall app", || Action::UninstallApp),
    ("screenshot", || Action::Screenshot),
    ("toggle layout bounds", || Action::ToggleLayoutBounds),
    ("toggle airplane mode", || Action::ToggleAirplaneMode),
    ("toggle wifi", || Action::ToggleWifi),
    ("wireless adb", || Action::WirelessAdb),
    ("quit", || Action::Quit),
];

pub struct CommandPaletteState {
    pub open: bool,
    pub filter: String,
    pub cursor: usize,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        Self {
            open: false,
            filter: String::new(),
            cursor: 0,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.filter.clear();
        self.cursor = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn filtered_commands(&self) -> Vec<(&'static str, fn() -> Action)> {
        COMMANDS
            .iter()
            .filter(|(name, _)| apps::fuzzy_matches(name, &self.filter))
            .copied()
            .collect()
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<Action> {
        match code {
            KeyCode::Esc => {
                self.close();
                None
            }
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                None
            }
            KeyCode::Down => {
                let count = self.filtered_commands().len();
                self.cursor = (self.cursor + 1).min(count.saturating_sub(1));
                None
            }
            KeyCode::Enter => {
                let filtered = self.filtered_commands();
                let result = filtered.get(self.cursor).map(|(_, action_fn)| action_fn());
                self.close();
                result
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.cursor = 0;
                None
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.cursor = 0;
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_closed() {
        let state = CommandPaletteState::new();
        assert!(!state.open);
        assert!(state.filter.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn open_resets_state() {
        let mut state = CommandPaletteState::new();
        state.filter = "test".to_string();
        state.cursor = 3;
        state.open();
        assert!(state.open);
        assert!(state.filter.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn esc_closes() {
        let mut state = CommandPaletteState::new();
        state.open();
        let result = state.handle_key(KeyCode::Esc);
        assert!(!state.open);
        assert!(result.is_none());
    }

    #[test]
    fn typing_builds_filter() {
        let mut state = CommandPaletteState::new();
        state.open();
        state.handle_key(KeyCode::Char('k'));
        state.handle_key(KeyCode::Char('i'));
        assert_eq!(state.filter, "ki");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn backspace_removes_char() {
        let mut state = CommandPaletteState::new();
        state.open();
        state.filter = "kill".to_string();
        state.handle_key(KeyCode::Backspace);
        assert_eq!(state.filter, "kil");
    }

    #[test]
    fn filter_narrows_results() {
        let mut state = CommandPaletteState::new();
        state.filter = "kill".to_string();
        let filtered = state.filtered_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "kill app");
    }

    #[test]
    fn empty_filter_shows_all() {
        let state = CommandPaletteState::new();
        let filtered = state.filtered_commands();
        assert_eq!(filtered.len(), COMMANDS.len());
    }

    #[test]
    fn cursor_moves_within_bounds() {
        let mut state = CommandPaletteState::new();
        state.open();
        state.handle_key(KeyCode::Down);
        assert_eq!(state.cursor, 1);
        state.handle_key(KeyCode::Up);
        assert_eq!(state.cursor, 0);
        state.handle_key(KeyCode::Up);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn enter_returns_selected_action() {
        let mut state = CommandPaletteState::new();
        state.open();
        state.filter = "quit".to_string();
        let result = state.handle_key(KeyCode::Enter);
        assert!(matches!(result, Some(Action::Quit)));
        assert!(!state.open);
    }

    #[test]
    fn enter_on_empty_returns_none() {
        let mut state = CommandPaletteState::new();
        state.open();
        state.filter = "xyznonexistent".to_string();
        let result = state.handle_key(KeyCode::Enter);
        assert!(result.is_none());
    }
}
