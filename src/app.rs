use crossterm::event::KeyCode;

use crate::panel;

pub enum Action {
    Quit,
    None,
    OpenApp,
    KillApp,
    ClearDataAndOpen,
    ClearData,
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
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 6],
            focused: Some(2),
            commands_cursor: 0,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        if self.focused == Some(7) {
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

        match code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char(c @ '2'..='7') => {
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
        if !(2..=7).contains(&n) {
            return;
        }
        let idx = (n - 2) as usize;
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
            &[true, false, true, true, true, true]
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
        for n in 3..=7 {
            app.toggle_visibility(n);
        }
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false]
        );
        app.toggle_visibility(2);
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false]
        );
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new();
        app.toggle_visibility(0);
        app.toggle_visibility(1);
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
            &[true, false, true, true, true, true]
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
        assert_eq!(app.focused_panel(), Some(7));
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn commands_cursor_moves_when_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), Some(7));
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
