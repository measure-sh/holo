use std::collections::HashSet;

use crossterm::event::KeyCode;

use crate::panel;

pub enum Action {
    Quit,
    None,
    OpenApp,
    KillApp,
    ClearDataAndOpen,
    ClearData,
    CopyLogcat,
}

const COMMAND_COUNT: usize = 4;

pub const COMMAND_LABELS: [&str; COMMAND_COUNT] = [
    "Open app",
    "Kill app",
    "Clear data",
    "Clear data & open",
];

pub struct App {
    visible: [bool; 6],
    focused: Option<u8>,
    commands_cursor: usize,
    logcat_cursor: Option<usize>,
    logcat_selected: HashSet<usize>,
    logcat_len: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 6],
            focused: Some(2),
            commands_cursor: 0,
            logcat_cursor: None,
            logcat_selected: HashSet::new(),
            logcat_len: 0,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        if self.focused == Some(1) {
            match code {
                KeyCode::Up => {
                    self.commands_cursor = self.commands_cursor.saturating_sub(1);
                    return Action::None;
                }
                KeyCode::Down => {
                    self.commands_cursor =
                        (self.commands_cursor + 1).min(COMMAND_COUNT - 1);
                    return Action::None;
                }
                KeyCode::Enter => {
                    return match self.commands_cursor {
                        0 => Action::OpenApp,
                        1 => Action::KillApp,
                        2 => Action::ClearData,
                        3 => Action::ClearDataAndOpen,
                        _ => Action::None,
                    };
                }
                _ => {}
            }
        }

        if self.focused == Some(2) && self.logcat_len > 0 {
            match code {
                KeyCode::Up => {
                    self.logcat_cursor = Some(match self.logcat_cursor {
                        None => self.logcat_len - 1,
                        Some(c) => c.saturating_sub(1),
                    });
                    return Action::None;
                }
                KeyCode::Down => {
                    if let Some(c) = self.logcat_cursor {
                        self.logcat_cursor = Some((c + 1).min(self.logcat_len - 1));
                    }
                    return Action::None;
                }
                KeyCode::Char(' ') => {
                    if let Some(c) = self.logcat_cursor {
                        if !self.logcat_selected.remove(&c) {
                            self.logcat_selected.insert(c);
                        }
                        self.logcat_cursor = Some((c + 1).min(self.logcat_len - 1));
                    }
                    return Action::None;
                }
                KeyCode::Enter => {
                    if self.logcat_cursor.is_some() {
                        return Action::CopyLogcat;
                    }
                }
                KeyCode::Esc => {
                    self.logcat_cursor = None;
                    self.logcat_selected.clear();
                    return Action::None;
                }
                _ => {}
            }
        }

        match code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char(c @ '1'..='6') => {
                self.toggle_visibility(c as u8 - b'0');
                Action::None
            }
            KeyCode::Char(c) => {
                if let Some(panel) = panel::by_focus_key(c) {
                    self.toggle_focus(panel.number);
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn toggle_visibility(&mut self, n: u8) {
        if !(1..=6).contains(&n) {
            return;
        }
        let idx = (n - 1) as usize;
        if self.visible[idx] && self.visible.iter().filter(|&&v| v).count() == 1 {
            return;
        }
        self.visible[idx] = !self.visible[idx];
    }

    fn toggle_focus(&mut self, n: u8) {
        if self.focused == Some(n) {
            self.focused = None;
        } else {
            self.focused = Some(n);
        }
    }

    pub fn panel_visibility(&self) -> &[bool; 6] {
        &self.visible
    }

    pub fn focused_panel(&self) -> Option<u8> {
        self.focused
    }

    pub fn commands_cursor(&self) -> usize {
        self.commands_cursor
    }

    pub fn set_logcat_len(&mut self, len: usize) {
        self.logcat_len = len;
        if let Some(c) = self.logcat_cursor {
            if len == 0 {
                self.logcat_cursor = None;
                self.logcat_selected.clear();
            } else if c >= len {
                self.logcat_cursor = Some(len - 1);
            }
        }
    }

    pub fn logcat_cursor(&self) -> Option<usize> {
        self.logcat_cursor
    }

    pub fn logcat_selected(&self) -> &HashSet<usize> {
        &self.logcat_selected
    }

    pub fn clear_logcat_selection(&mut self) {
        self.logcat_selected.clear();
    }

    pub fn logcat_scroll_offset(&self, visible_height: usize) -> usize {
        let len = self.logcat_len;
        if len <= visible_height {
            return 0;
        }
        match self.logcat_cursor {
            None => len - visible_height,
            Some(cursor) => {
                let half = visible_height / 2;
                if cursor < half {
                    0
                } else if cursor + visible_height - half > len {
                    len - visible_height
                } else {
                    cursor - half
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_app_all_panels_visible() {
        let app = App::new();
        assert_eq!(app.panel_visibility(), &[true; 6]);
    }

    #[test]
    fn new_app_focuses_logcat() {
        let app = App::new();
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn toggle_visibility_hides_panel() {
        let mut app = App::new();
        app.toggle_visibility(3);
        assert_eq!(
            app.panel_visibility(),
            &[true, true, false, true, true, true]
        );
    }

    #[test]
    fn toggle_visibility_twice_restores_panel() {
        let mut app = App::new();
        app.toggle_visibility(3);
        app.toggle_visibility(3);
        assert_eq!(app.panel_visibility(), &[true; 6]);
    }

    #[test]
    fn cannot_hide_last_panel() {
        let mut app = App::new();
        for n in 2..=6 {
            app.toggle_visibility(n);
        }
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false]
        );
        app.toggle_visibility(1);
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false]
        );
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new();
        app.toggle_visibility(0);
        app.toggle_visibility(7);
        app.toggle_visibility(8);
        assert_eq!(app.panel_visibility(), &[true; 6]);
    }

    #[test]
    fn handle_key_q_quits() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('q')), Action::Quit));
    }

    #[test]
    fn handle_key_digit_toggles_visibility() {
        let mut app = App::new();
        assert!(matches!(
            app.handle_key(KeyCode::Char('3')),
            Action::None
        ));
        assert_eq!(
            app.panel_visibility(),
            &[true, true, false, true, true, true]
        );
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new();
        assert!(matches!(
            app.handle_key(KeyCode::Char('x')),
            Action::None
        ));
        assert_eq!(app.panel_visibility(), &[true; 6]);
    }

    #[test]
    fn focus_toggles_on_same_key() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), None);
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn focus_switches_between_panels() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn commands_cursor_moves_when_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.commands_cursor(), 1);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.commands_cursor(), 2);
        app.handle_key(KeyCode::Up);
        assert_eq!(app.commands_cursor(), 1);
    }

    #[test]
    fn commands_cursor_does_not_go_below_zero() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('c'));
        app.handle_key(KeyCode::Up);
        assert_eq!(app.commands_cursor(), 0);
    }

    #[test]
    fn commands_cursor_does_not_exceed_max() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('c'));
        for _ in 0..10 {
            app.handle_key(KeyCode::Down);
        }
        assert_eq!(app.commands_cursor(), COMMAND_COUNT - 1);
    }

    #[test]
    fn enter_on_commands_returns_action() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('c'));
        assert!(matches!(app.handle_key(KeyCode::Enter), Action::OpenApp));
        app.handle_key(KeyCode::Down);
        assert!(matches!(app.handle_key(KeyCode::Enter), Action::KillApp));
        app.handle_key(KeyCode::Down);
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::ClearData
        ));
        app.handle_key(KeyCode::Down);
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::ClearDataAndOpen
        ));
    }

    #[test]
    fn arrows_ignored_when_commands_not_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Down);
        assert_eq!(app.commands_cursor(), 0);
    }
}
