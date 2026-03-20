use crossterm::event::KeyCode;

pub enum Action {
    Quit,
    None,
}

pub struct App {
    pub selected_panel: Option<u8>,
}

impl App {
    pub fn new() -> Self {
        Self {
            selected_panel: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char(c @ '1'..='6') => {
                self.select_panel(c as u8 - b'0');
                Action::None
            }
            _ => Action::None,
        }
    }

    fn select_panel(&mut self, n: u8) {
        if !(1..=6).contains(&n) {
            return;
        }
        if self.selected_panel == Some(n) {
            self.selected_panel = None;
        } else {
            self.selected_panel = Some(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_app_has_no_selection() {
        let app = App::new();
        assert_eq!(app.selected_panel, None);
    }

    #[test]
    fn select_panel_sets_selection() {
        let mut app = App::new();
        app.select_panel(3);
        assert_eq!(app.selected_panel, Some(3));
    }

    #[test]
    fn select_same_panel_toggles_off() {
        let mut app = App::new();
        app.select_panel(2);
        app.select_panel(2);
        assert_eq!(app.selected_panel, None);
    }

    #[test]
    fn select_different_panel_switches() {
        let mut app = App::new();
        app.select_panel(1);
        app.select_panel(4);
        assert_eq!(app.selected_panel, Some(4));
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new();
        app.select_panel(0);
        assert_eq!(app.selected_panel, None);
        app.select_panel(7);
        assert_eq!(app.selected_panel, None);
        app.select_panel(3);
        app.select_panel(0);
        assert_eq!(app.selected_panel, Some(3));
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
    fn handle_key_panel_number_selects() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('3')), Action::None));
        assert_eq!(app.selected_panel, Some(3));
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('x')), Action::None));
        assert_eq!(app.selected_panel, None);
    }
}
