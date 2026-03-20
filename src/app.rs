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

const LEVELS: [Option<char>; 7] = [
    None,
    Some('V'),
    Some('D'),
    Some('I'),
    Some('W'),
    Some('E'),
    Some('F'),
];

pub struct LogcatFilter {
    pub tag: String,
    pub search: String,
    pub level: Option<char>,
}

impl LogcatFilter {
    fn new() -> Self {
        Self {
            tag: String::new(),
            search: String::new(),
            level: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    EditingTag,
    EditingSearch,
}

pub struct App {
    visible: [bool; 6],
    focused: Option<u8>,
    commands_cursor: usize,
    logcat_filter: LogcatFilter,
    input_mode: InputMode,
    logcat_scroll: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 6],
            focused: Some(1),
            commands_cursor: 0,
            logcat_filter: LogcatFilter::new(),
            input_mode: InputMode::Normal,
            logcat_scroll: 0,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match self.input_mode {
            InputMode::EditingTag => {
                match code {
                    KeyCode::Char(c) => self.logcat_filter.tag.push(c),
                    KeyCode::Backspace => { self.logcat_filter.tag.pop(); }
                    KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
                    _ => {}
                }
                return Action::None;
            }
            InputMode::EditingSearch => {
                match code {
                    KeyCode::Char(c) => self.logcat_filter.search.push(c),
                    KeyCode::Backspace => { self.logcat_filter.search.pop(); }
                    KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
                    _ => {}
                }
                return Action::None;
            }
            InputMode::Normal => {}
        }

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

        if self.focused == Some(2) {
            if code == KeyCode::Esc && self.logcat_scroll > 0 {
                self.logcat_scroll = 0;
                return Action::None;
            }
            match code {
                KeyCode::Char('t') => {
                    self.input_mode = InputMode::EditingTag;
                    return Action::None;
                }
                KeyCode::Char('s') => {
                    self.input_mode = InputMode::EditingSearch;
                    return Action::None;
                }
                KeyCode::Up => {
                    self.logcat_scroll += 1;
                    return Action::None;
                }
                KeyCode::Down => {
                    self.logcat_scroll = self.logcat_scroll.saturating_sub(1);
                    return Action::None;
                }
                KeyCode::Right => {
                    self.cycle_level(true);
                    return Action::None;
                }
                KeyCode::Left => {
                    self.cycle_level(false);
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

    fn cycle_level(&mut self, forward: bool) {
        let current = LEVELS.iter().position(|l| *l == self.logcat_filter.level).unwrap_or(0);
        let next = if forward {
            (current + 1) % LEVELS.len()
        } else {
            (current + LEVELS.len() - 1) % LEVELS.len()
        };
        self.logcat_filter.level = LEVELS[next];
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

    pub fn logcat_filter(&self) -> &LogcatFilter {
        &self.logcat_filter
    }

    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    pub fn logcat_scroll(&self) -> usize {
        self.logcat_scroll
    }

    pub fn clamp_logcat_scroll(&mut self, total_lines: usize, visible_height: usize) {
        let max = total_lines.saturating_sub(visible_height);
        self.logcat_scroll = self.logcat_scroll.min(max);
    }

    pub fn adjust_logcat_scroll_for_new_lines(&mut self, count: usize) {
        if self.logcat_scroll > 0 {
            self.logcat_scroll += count;
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
    fn new_app_focuses_commands() {
        let app = App::new();
        assert_eq!(app.focused_panel(), Some(1));
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
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), None);
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), Some(1));
    }

    #[test]
    fn focus_switches_between_panels() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(KeyCode::Char('c'));
        assert_eq!(app.focused_panel(), Some(1));
    }

    #[test]
    fn commands_cursor_moves_when_focused() {
        let mut app = App::new();
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
        app.handle_key(KeyCode::Up);
        assert_eq!(app.commands_cursor(), 0);
    }

    #[test]
    fn commands_cursor_does_not_exceed_max() {
        let mut app = App::new();
        for _ in 0..10 {
            app.handle_key(KeyCode::Down);
        }
        assert_eq!(app.commands_cursor(), COMMAND_COUNT - 1);
    }

    #[test]
    fn enter_on_commands_returns_action() {
        let mut app = App::new();
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
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.commands_cursor(), 0);
    }

    #[test]
    fn t_enters_tag_editing_when_logcat_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        assert_eq!(app.input_mode(), InputMode::EditingTag);
    }

    #[test]
    fn s_enters_search_editing_when_logcat_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.input_mode(), InputMode::EditingSearch);
    }

    #[test]
    fn t_ignored_when_logcat_not_focused() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Char('t'));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn typing_appends_to_tag() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('b'));
        assert_eq!(app.logcat_filter().tag, "ab");
    }

    #[test]
    fn typing_appends_to_search() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Char('x'));
        app.handle_key(KeyCode::Char('y'));
        assert_eq!(app.logcat_filter().search, "xy");
    }

    #[test]
    fn backspace_removes_from_tag() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Backspace);
        assert_eq!(app.logcat_filter().tag, "a");
    }

    #[test]
    fn esc_exits_editing_mode() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        assert_eq!(app.input_mode(), InputMode::EditingTag);
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn enter_exits_editing_mode() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.input_mode(), InputMode::EditingSearch);
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn level_cycles_forward_with_right() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.logcat_filter().level, None);
        app.handle_key(KeyCode::Right);
        assert_eq!(app.logcat_filter().level, Some('V'));
        app.handle_key(KeyCode::Right);
        assert_eq!(app.logcat_filter().level, Some('D'));
    }

    #[test]
    fn level_cycles_backward_with_left() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Left);
        assert_eq!(app.logcat_filter().level, Some('F'));
    }

    #[test]
    fn level_wraps_around() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        for _ in 0..7 {
            app.handle_key(KeyCode::Right);
        }
        assert_eq!(app.logcat_filter().level, None);
    }

    #[test]
    fn scroll_up_increments_offset() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_scroll(), 1);
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_scroll(), 2);
    }

    #[test]
    fn scroll_down_decrements_offset() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.logcat_scroll(), 1);
    }

    #[test]
    fn scroll_does_not_go_below_zero() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.logcat_scroll(), 0);
    }

    #[test]
    fn esc_resets_scroll_to_zero() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_scroll(), 3);
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.logcat_scroll(), 0);
    }

    #[test]
    fn esc_does_nothing_when_already_at_bottom() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.logcat_scroll(), 0);
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.logcat_scroll(), 0);
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn scroll_ignored_when_logcat_not_focused() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), Some(1));
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.logcat_scroll(), 0);
    }
}
