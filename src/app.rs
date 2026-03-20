use crossterm::event::KeyCode;

use crate::panel;

pub enum Action {
    Quit,
    None,
    OpenApp(usize),
    KillApp(usize),
    ClearDataAndOpen(usize),
    ClearData(usize),
}

pub struct App {
    visible: [bool; 7],
    focused: Option<u8>,
    apps_cursor: usize,
    filter_active: bool,
    filter_text: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 7],
            focused: Some(1),
            apps_cursor: 0,
            filter_active: false,
            filter_text: String::new(),
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, app_count: usize) -> Action {
        if self.focused == Some(1) && self.filter_active {
            match code {
                KeyCode::Enter => {
                    self.filter_active = false;
                    return Action::None;
                }
                KeyCode::Esc => {
                    self.filter_active = false;
                    self.filter_text.clear();
                    self.apps_cursor = 0;
                    return Action::None;
                }
                KeyCode::Backspace => { self.filter_text.pop(); self.apps_cursor = 0; return Action::None; }
                KeyCode::Up => { self.move_apps_cursor(-1, app_count); return Action::None; }
                KeyCode::Down => { self.move_apps_cursor(1, app_count); return Action::None; }
                KeyCode::Char(c) => { self.filter_text.push(c); self.apps_cursor = 0; return Action::None; }
                _ => return Action::None,
            }
        }

        if self.focused == Some(1) {
            match code {
                KeyCode::Up => { self.move_apps_cursor(-1, app_count); return Action::None; }
                KeyCode::Down => { self.move_apps_cursor(1, app_count); return Action::None; }
                KeyCode::Char('f') => { self.filter_active = true; return Action::None; }
                KeyCode::Char('o') => return Action::OpenApp(self.apps_cursor),
                KeyCode::Char('k') => return Action::KillApp(self.apps_cursor),
                KeyCode::Char('r') => return Action::ClearDataAndOpen(self.apps_cursor),
                KeyCode::Char('e') => return Action::ClearData(self.apps_cursor),
                KeyCode::Esc => {
                    self.filter_text.clear();
                    self.apps_cursor = 0;
                    return Action::None;
                }
                _ => {}
            }
        }

        match code {
            KeyCode::Char('q') => Action::Quit,
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

    fn move_apps_cursor(&mut self, delta: isize, app_count: usize) {
        let new = self.apps_cursor as isize + delta;
        let max = app_count.saturating_sub(1);
        self.apps_cursor = (new.max(0) as usize).min(max);
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

    pub fn filter_text(&self) -> &str {
        &self.filter_text
    }

    pub fn is_filtering(&self) -> bool {
        self.filter_active
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
        assert!(matches!(app.handle_key(KeyCode::Char('q'), 10), Action::Quit));
    }

    #[test]
    fn handle_key_esc_does_not_quit() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Esc, 10), Action::None));
    }

    #[test]
    fn handle_key_digit_toggles_visibility() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('3'), 10), Action::None));
        assert_eq!(app.panel_visibility(), &[true, true, false, true, true, true, true]);
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('x'), 10), Action::None));
        assert_eq!(app.panel_visibility(), &[true; 7]);
    }

    #[test]
    fn focus_toggles_on_same_key() {
        let mut app = App::new();
        // Panel 1 starts focused; pressing 'i' unfocuses
        app.handle_key(KeyCode::Char('i'), 10);
        assert_eq!(app.focused_panel(), None);
        // Pressing again refocuses
        app.handle_key(KeyCode::Char('i'), 10);
        assert_eq!(app.focused_panel(), Some(1));
    }

    #[test]
    fn focus_switches_between_panels() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Char('l'), 10);
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(KeyCode::Char('c'), 10);
        assert_eq!(app.focused_panel(), Some(7));
    }

    #[test]
    fn unfocusable_keys_do_not_change_focus() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('n'), 10);
        assert_eq!(app.focused_panel(), Some(1));
    }

    #[test]
    fn new_app_focuses_apps_panel() {
        let app = App::new();
        assert_eq!(app.focused_panel(), Some(1));
    }

    #[test]
    fn arrow_keys_move_cursor_when_apps_focused() {
        let mut app = App::new();
        // Panel 1 starts focused
        app.handle_key(KeyCode::Down, 10);
        assert_eq!(app.selected_app(), Some(1));
        app.handle_key(KeyCode::Down, 10);
        assert_eq!(app.selected_app(), Some(2));
        app.handle_key(KeyCode::Up, 10);
        assert_eq!(app.selected_app(), Some(1));
    }

    #[test]
    fn arrow_keys_ignored_when_apps_not_focused() {
        let mut app = App::new();
        // Focus a different panel
        app.handle_key(KeyCode::Char('l'), 10);
        app.handle_key(KeyCode::Down, 10);
        assert_eq!(app.selected_app(), None);
    }

    #[test]
    fn cursor_does_not_go_below_zero() {
        let mut app = App::new();
        // Panel 1 starts focused
        app.handle_key(KeyCode::Up, 10);
        assert_eq!(app.selected_app(), Some(0));
    }

    #[test]
    fn cursor_does_not_exceed_app_count() {
        let mut app = App::new();
        for _ in 0..20 {
            app.handle_key(KeyCode::Down, 5);
        }
        assert_eq!(app.selected_app(), Some(4));
        // Moving up once should immediately go to index 3
        app.handle_key(KeyCode::Up, 5);
        assert_eq!(app.selected_app(), Some(3));
    }

    #[test]
    fn cursor_preserved_when_refocusing() {
        let mut app = App::new();
        // Panel 1 starts focused
        app.handle_key(KeyCode::Down, 10);
        app.handle_key(KeyCode::Down, 10);
        assert_eq!(app.selected_app(), Some(2));
        // Unfocus
        app.handle_key(KeyCode::Char('i'), 10);
        assert_eq!(app.selected_app(), None);
        // Refocus — cursor remembered
        app.handle_key(KeyCode::Char('i'), 10);
        assert_eq!(app.selected_app(), Some(2));
    }

    #[test]
    fn o_opens_app_when_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Down, 10);
        assert!(matches!(app.handle_key(KeyCode::Char('o'), 10), Action::OpenApp(1)));
    }

    #[test]
    fn k_kills_app_when_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Down, 10);
        app.handle_key(KeyCode::Down, 10);
        assert!(matches!(app.handle_key(KeyCode::Char('k'), 10), Action::KillApp(2)));
    }

    #[test]
    fn r_clears_and_opens_when_focused() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('r'), 10), Action::ClearDataAndOpen(0)));
    }

    #[test]
    fn e_clears_data_when_focused() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('e'), 10), Action::ClearData(0)));
    }

    #[test]
    fn app_actions_ignored_when_not_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'), 10); // focus panel 2
        assert!(matches!(app.handle_key(KeyCode::Char('o'), 10), Action::None));
        assert!(matches!(app.handle_key(KeyCode::Char('k'), 10), Action::None));
        assert!(matches!(app.handle_key(KeyCode::Char('r'), 10), Action::None));
        assert!(matches!(app.handle_key(KeyCode::Char('e'), 10), Action::None));
    }

    #[test]
    fn f_enters_filter_mode() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        assert!(app.is_filtering());
        assert_eq!(app.filter_text(), "");
    }

    #[test]
    fn typing_in_filter_mode_builds_query() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        app.handle_key(KeyCode::Char('c'), 10);
        app.handle_key(KeyCode::Char('o'), 10);
        app.handle_key(KeyCode::Char('m'), 10);
        assert_eq!(app.filter_text(), "com");
    }

    #[test]
    fn backspace_in_filter_removes_char() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        app.handle_key(KeyCode::Char('a'), 10);
        app.handle_key(KeyCode::Char('b'), 10);
        app.handle_key(KeyCode::Backspace, 10);
        assert_eq!(app.filter_text(), "a");
    }

    #[test]
    fn escape_clears_filter_and_exits_filter_mode() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        app.handle_key(KeyCode::Char('x'), 10);
        app.handle_key(KeyCode::Esc, 10);
        assert!(!app.is_filtering());
        assert_eq!(app.filter_text(), "");
    }

    #[test]
    fn arrows_work_in_filter_mode() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        app.handle_key(KeyCode::Down, 5);
        assert_eq!(app.selected_app(), Some(1));
        app.handle_key(KeyCode::Up, 5);
        assert_eq!(app.selected_app(), Some(0));
    }

    #[test]
    fn actions_not_dispatched_in_filter_mode() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        // 'o' should be treated as filter input, not open action
        assert!(matches!(app.handle_key(KeyCode::Char('o'), 10), Action::None));
        assert_eq!(app.filter_text(), "o");
    }

    #[test]
    fn filter_cursor_resets_on_new_input() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 5);
        app.handle_key(KeyCode::Down, 5);
        app.handle_key(KeyCode::Down, 5);
        assert_eq!(app.selected_app(), Some(2));
        // Typing resets cursor to 0
        app.handle_key(KeyCode::Char('x'), 5);
        assert_eq!(app.selected_app(), Some(0));
    }

    #[test]
    fn enter_exits_filter_mode_but_preserves_text() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('f'), 10);
        app.handle_key(KeyCode::Char('c'), 10);
        app.handle_key(KeyCode::Char('o'), 10);
        app.handle_key(KeyCode::Enter, 10);
        assert!(!app.is_filtering());
        assert_eq!(app.filter_text(), "co");
    }

    #[test]
    fn esc_clears_filter_when_not_in_filter_mode() {
        let mut app = App::new();
        // Enter filter, type, exit with Esc
        app.handle_key(KeyCode::Char('f'), 10);
        app.handle_key(KeyCode::Char('x'), 10);
        app.handle_key(KeyCode::Esc, 10);
        // Now not filtering, filter_text is empty
        // Pressing Esc again on focused apps panel is a no-op
        app.handle_key(KeyCode::Esc, 10);
        assert!(!app.is_filtering());
        assert_eq!(app.filter_text(), "");
    }
}
