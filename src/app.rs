use crossterm::event::KeyCode;

pub enum Action {
    Quit,
    None,
}

pub struct App {
    visible: [bool; 7],
    focused: Option<u8>,
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 7],
            focused: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char(c @ '1'..='7') => {
                self.toggle_panel(c as u8 - b'0');
                Action::None
            }
            KeyCode::Char('i') => { self.focus_panel(1); Action::None }
            KeyCode::Char('l') => { self.focus_panel(2); Action::None }
            KeyCode::Char('c') => { self.focus_panel(7); Action::None }
            _ => Action::None,
        }
    }

    fn focus_panel(&mut self, n: u8) {
        if self.focused == Some(n) {
            self.focused = None;
        } else {
            self.focused = Some(n);
        }
    }

    pub fn focused_panel(&self) -> Option<u8> {
        self.focused
    }

    fn toggle_panel(&mut self, n: u8) {
        if !(1..=7).contains(&n) {
            return;
        }
        let idx = (n - 1) as usize;
        // Prevent hiding the last visible panel
        if self.visible[idx] && self.visible.iter().filter(|&&v| v).count() == 1 {
            return;
        }
        self.visible[idx] = !self.visible[idx];
    }

    pub fn panel_visibility(&self) -> &[bool; 7] {
        &self.visible
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
    fn toggle_hides_panel() {
        let mut app = App::new();
        app.toggle_panel(3);
        assert_eq!(app.panel_visibility(), &[true, true, false, true, true, true, true]);
    }

    #[test]
    fn toggle_twice_restores_panel() {
        let mut app = App::new();
        app.toggle_panel(3);
        app.toggle_panel(3);
        assert_eq!(app.panel_visibility(), &[true; 7]);
    }

    #[test]
    fn cannot_hide_last_panel() {
        let mut app = App::new();
        for n in 2..=7 {
            app.toggle_panel(n);
        }
        assert_eq!(app.panel_visibility(), &[true, false, false, false, false, false, false]);
        // Try to hide the last one — should be prevented
        app.toggle_panel(1);
        assert_eq!(app.panel_visibility(), &[true, false, false, false, false, false, false]);
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new();
        app.toggle_panel(0);
        app.toggle_panel(8);
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
    fn handle_key_panel_number_toggles() {
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
}
