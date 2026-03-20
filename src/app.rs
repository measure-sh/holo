use crossterm::event::KeyCode;

use crate::panel;

pub enum Action {
    Quit,
    None,
}

pub struct App {
    visible: [bool; 7],
    focused: Option<u8>,
    apps_cursor: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 7],
            focused: None,
            apps_cursor: 0,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        if self.focused == Some(1) {
            match code {
                KeyCode::Up => { self.move_apps_cursor(-1); return Action::None; }
                KeyCode::Down => { self.move_apps_cursor(1); return Action::None; }
                _ => {}
            }
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char(c @ '1'..='7') => {
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
        if !(1..=7).contains(&n) {
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

    fn move_apps_cursor(&mut self, delta: isize) {
        let new = self.apps_cursor as isize + delta;
        self.apps_cursor = new.max(0) as usize;
    }

    pub fn selected_app(&self) -> Option<usize> {
        if self.focused == Some(1) {
            Some(self.apps_cursor)
        } else {
            None
        }
    }

    pub fn panel_visibility(&self) -> &[bool; 7] {
        &self.visible
    }

    pub fn focused_panel(&self) -> Option<u8> {
        self.focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_app_all_panels_visible() {
        let app = App::new();
        assert_eq!(app.panel_visibility(), &[true; 7]);
    }

    #[test]
    fn toggle_visibility_hides_panel() {
        let mut app = App::new();
        app.toggle_visibility(3);
        assert_eq!(app.panel_visibility(), &[true, true, false, true, true, true, true]);
    }

    #[test]
    fn toggle_visibility_twice_restores_panel() {
        let mut app = App::new();
        app.toggle_visibility(3);
        app.toggle_visibility(3);
        assert_eq!(app.panel_visibility(), &[true; 7]);
    }

    #[test]
    fn cannot_hide_last_panel() {
        let mut app = App::new();
        for n in 2..=7 {
            app.toggle_visibility(n);
        }
        assert_eq!(app.panel_visibility(), &[true, false, false, false, false, false, false]);
        app.toggle_visibility(1);
        assert_eq!(app.panel_visibility(), &[true, false, false, false, false, false, false]);
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new();
        app.toggle_visibility(0);
        app.toggle_visibility(8);
        assert_eq!(app.panel_visibility(), &[true; 7]);
    }

    #[test]
    fn handle_key_q_quits() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('q')), Action::Quit));
    }

    #[test]
    fn handle_key_esc_quits() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Esc), Action::Quit));
    }

    #[test]
    fn handle_key_digit_toggles_visibility() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('3')), Action::None));
        assert_eq!(app.panel_visibility(), &[true, true, false, true, true, true, true]);
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('x')), Action::None));
        assert_eq!(app.panel_visibility(), &[true; 7]);
    }

    #[test]
    fn focus_toggles_on_same_key() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('i'));
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Char('i'));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn focus_switches_between_panels() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('i'));
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), Some(7));
    }

    #[test]
    fn unfocusable_keys_do_not_focus() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('n'));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn new_app_has_no_focus() {
        let app = App::new();
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn arrow_keys_move_cursor_when_apps_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('i'));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_app(), Some(1));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_app(), Some(2));
        app.handle_key(KeyCode::Up);
        assert_eq!(app.selected_app(), Some(1));
    }

    #[test]
    fn arrow_keys_ignored_when_apps_not_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_app(), None);
        // Focus a different panel
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_app(), None);
    }

    #[test]
    fn cursor_does_not_go_below_zero() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('i'));
        app.handle_key(KeyCode::Up);
        assert_eq!(app.selected_app(), Some(0));
    }

    #[test]
    fn cursor_preserved_when_refocusing() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('i'));
        app.handle_key(KeyCode::Down);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_app(), Some(2));
        // Unfocus
        app.handle_key(KeyCode::Char('i'));
        assert_eq!(app.selected_app(), None);
        // Refocus — cursor remembered
        app.handle_key(KeyCode::Char('i'));
        assert_eq!(app.selected_app(), Some(2));
    }
}
