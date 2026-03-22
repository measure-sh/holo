use crossterm::event::KeyCode;

use crate::database::DatabaseState;
use crate::logcat_state::LogcatState;
use crate::panel;

pub enum Action {
    Quit,
    None,
    OpenApp,
    KillApp,
    ClearData,
    ResetLogcat,
    ResetDb,
    CopyDbResult(String),
    RunQuery(String, String),
    PullDb(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    EditingTag,
    EditingSearch,
    EditingQuery,
}

pub struct App {
    visible: [bool; 5],
    focused: Option<u8>,
    input_mode: InputMode,
    logcat_state: LogcatState,
    db_state: DatabaseState,
}

impl App {
    pub fn new() -> Self {
        Self {
            visible: [true; 5],
            focused: None,
            input_mode: InputMode::Normal,
            logcat_state: LogcatState::new(),
            db_state: DatabaseState::new(),
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Action {
        match self.input_mode {
            InputMode::EditingTag => {
                match code {
                    KeyCode::Char(c) => self.logcat_state.filter.tag.push(c),
                    KeyCode::Backspace => { self.logcat_state.filter.tag.pop(); }
                    KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
                    _ => {}
                }
                return Action::None;
            }
            InputMode::EditingSearch => {
                match code {
                    KeyCode::Char(c) => self.logcat_state.filter.search.push(c),
                    KeyCode::Backspace => { self.logcat_state.filter.search.pop(); }
                    KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
                    _ => {}
                }
                return Action::None;
            }
            InputMode::EditingQuery => {
                match code {
                    KeyCode::Char(c) => self.db_state.input.push(c),
                    KeyCode::Backspace => { self.db_state.input.pop(); }
                    KeyCode::Enter => {
                        if let Some(db) = self.db_state.selected_db.clone() {
                            let sql = self.db_state.input.clone();
                            if !sql.is_empty() {
                                self.db_state.push_query(&sql);
                                self.db_state.input.clear();
                                self.db_state.scroll = 0;
                                self.input_mode = InputMode::Normal;
                                return Action::RunQuery(db, sql);
                            }
                        }
                    }
                    KeyCode::Esc => self.input_mode = InputMode::Normal,
                    _ => {}
                }
                return Action::None;
            }
            InputMode::Normal => {}
        }

        if self.db_state.confirming_pull.is_some() {
            return match code {
                KeyCode::Enter => {
                    let db = self.db_state.confirming_pull.take().unwrap();
                    Action::PullDb(db)
                }
                _ => {
                    self.db_state.confirming_pull = None;
                    Action::None
                }
            };
        }

        if self.focused == Some(panel::DATABASE) {
            self.db_state.copied_flash = false;
            match code {
                KeyCode::Up | KeyCode::Down if self.db_state.selected_db.is_none() => {
                    if code == KeyCode::Up {
                        self.db_state.move_up();
                    } else {
                        self.db_state.move_down();
                    }
                    return Action::None;
                }
                KeyCode::Char('p') if self.db_state.selected_db.is_none() => {
                    if let Some(db) = self.db_state.databases.get(self.db_state.selected_index).cloned() {
                        self.db_state.confirming_pull = Some(db);
                    }
                    return Action::None;
                }
                KeyCode::Enter if self.db_state.selected_db.is_none() => {
                    self.db_state.select_db();
                    if self.db_state.selected_db.is_some() {
                        self.input_mode = InputMode::EditingQuery;
                    }
                    return Action::None;
                }
                KeyCode::Char('p') if self.db_state.selected_db.is_some() => {
                    self.db_state.confirming_pull = self.db_state.selected_db.clone();
                    return Action::None;
                }
                KeyCode::Char('e') if self.db_state.selected_db.is_some() => {
                    self.input_mode = InputMode::EditingQuery;
                    return Action::None;
                }
                KeyCode::Char('c') if self.db_state.selected_db.is_some() => {
                    if let Some(text) = self.db_state.history_text() {
                        self.db_state.copied_flash = true;
                        return Action::CopyDbResult(text);
                    }
                    return Action::None;
                }
                KeyCode::Char('r') if self.db_state.selected_db.is_some() => {
                    self.db_state.reset();
                    return Action::ResetDb;
                }
                KeyCode::Up if self.db_state.selected_db.is_some() => {
                    self.db_state.move_up();
                    return Action::None;
                }
                KeyCode::Down if self.db_state.selected_db.is_some() => {
                    self.db_state.move_down();
                    return Action::None;
                }
                KeyCode::Esc if self.db_state.selected_db.is_some() => {
                    self.db_state.deselect_db();
                    return Action::None;
                }
                KeyCode::Esc => {
                    self.focused = None;
                    return Action::None;
                }
                _ => {}
            }
        }

        if self.focused == Some(panel::LOGCAT) {
            match code {
                KeyCode::Up => {
                    self.logcat_state.scroll += 1;
                    return Action::None;
                }
                KeyCode::Down => {
                    self.logcat_state.scroll = self.logcat_state.scroll.saturating_sub(1);
                    return Action::None;
                }
                KeyCode::Esc if self.logcat_state.scroll > 0 => {
                    self.logcat_state.scroll = 0;
                    return Action::None;
                }
                KeyCode::Esc => {
                    self.focused = None;
                    return Action::None;
                }
                KeyCode::Char('t') => {
                    self.input_mode = InputMode::EditingTag;
                    return Action::None;
                }
                KeyCode::Char('s') => {
                    self.input_mode = InputMode::EditingSearch;
                    return Action::None;
                }
                KeyCode::Right => {
                    self.logcat_state.cycle_level(true);
                    return Action::None;
                }
                KeyCode::Left => {
                    self.logcat_state.cycle_level(false);
                    return Action::None;
                }
                KeyCode::Char('r') => {
                    self.logcat_state.reset();
                    return Action::ResetLogcat;
                }
                _ => {}
            }
        }

        match code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('o') => Action::OpenApp,
            KeyCode::Char('k') => Action::KillApp,
            KeyCode::Char('c') => Action::ClearData,
            KeyCode::Char(c @ '1'..='5') => {
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
        if !(1..=5).contains(&n) {
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
        self.input_mode = InputMode::Normal;
    }

    pub fn panel_visibility(&self) -> &[bool; 5] {
        &self.visible
    }

    #[cfg(test)]
    pub fn db_state(&self) -> &DatabaseState {
        &self.db_state
    }

    pub fn db_state_mut(&mut self) -> &mut DatabaseState {
        &mut self.db_state
    }

    pub fn focused_panel(&self) -> Option<u8> {
        self.focused
    }

    pub fn logcat_state(&self) -> &LogcatState {
        &self.logcat_state
    }

    pub fn logcat_state_mut(&mut self) -> &mut LogcatState {
        &mut self.logcat_state
    }

    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_app_all_panels_visible() {
        let app = App::new();
        assert_eq!(app.panel_visibility(), &[true; 5]);
    }

    #[test]
    fn new_app_has_no_focus() {
        let app = App::new();
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn toggle_visibility_hides_panel() {
        let mut app = App::new();
        app.toggle_visibility(3);
        assert_eq!(
            app.panel_visibility(),
            &[true, true, false, true, true]
        );
    }

    #[test]
    fn toggle_visibility_twice_restores_panel() {
        let mut app = App::new();
        app.toggle_visibility(3);
        app.toggle_visibility(3);
        assert_eq!(app.panel_visibility(), &[true; 5]);
    }

    #[test]
    fn cannot_hide_last_panel() {
        let mut app = App::new();
        for n in 2..=5 {
            app.toggle_visibility(n);
        }
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false]
        );
        app.toggle_visibility(1);
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false]
        );
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new();
        app.toggle_visibility(0);
        app.toggle_visibility(8);
        app.toggle_visibility(9);
        assert_eq!(app.panel_visibility(), &[true; 5]);
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
            &[true, true, false, true, true]
        );
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new();
        assert!(matches!(
            app.handle_key(KeyCode::Char('x')),
            Action::None
        ));
        assert_eq!(app.panel_visibility(), &[true; 5]);
    }

    #[test]
    fn focus_toggles_on_same_key() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn o_opens_app() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('o')), Action::OpenApp));
    }

    #[test]
    fn k_kills_app() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('k')), Action::KillApp));
    }

    #[test]
    fn c_clears_data() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('c')), Action::ClearData));
    }

    #[test]
    fn t_enters_tag_editing_when_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        assert_eq!(app.input_mode(), InputMode::EditingTag);
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn s_enters_search_editing_when_focused() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.input_mode(), InputMode::EditingSearch);
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn t_ignored_without_focus() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), None);
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
        assert_eq!(app.logcat_state().filter.tag, "ab");
    }

    #[test]
    fn typing_appends_to_search() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Char('x'));
        app.handle_key(KeyCode::Char('y'));
        assert_eq!(app.logcat_state().filter.search, "xy");
    }

    #[test]
    fn backspace_removes_from_tag() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Backspace);
        assert_eq!(app.logcat_state().filter.tag, "a");
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
    fn esc_exits_tag_editing_preserving_input() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.input_mode(), InputMode::Normal);
        assert_eq!(app.logcat_state().filter.tag, "ab");
    }

    #[test]
    fn esc_exits_search_editing_preserving_input() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Char('x'));
        app.handle_key(KeyCode::Char('y'));
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.input_mode(), InputMode::Normal);
        assert_eq!(app.logcat_state().filter.search, "xy");
    }

    #[test]
    fn level_cycles_forward_with_right() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.logcat_state().filter.level, None);
        app.handle_key(KeyCode::Right);
        assert_eq!(app.logcat_state().filter.level, Some('V'));
        app.handle_key(KeyCode::Right);
        assert_eq!(app.logcat_state().filter.level, Some('D'));
    }

    #[test]
    fn level_cycles_backward_with_left() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Left);
        assert_eq!(app.logcat_state().filter.level, Some('F'));
    }

    #[test]
    fn level_wraps_around() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        for _ in 0..7 {
            app.handle_key(KeyCode::Right);
        }
        assert_eq!(app.logcat_state().filter.level, None);
    }

    #[test]
    fn scroll_up_increments_offset() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_state().scroll, 1);
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_state().scroll, 2);
    }

    #[test]
    fn scroll_down_decrements_offset() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.logcat_state().scroll, 1);
    }

    #[test]
    fn scroll_does_not_go_below_zero() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.logcat_state().scroll, 0);
    }

    #[test]
    fn esc_resets_scroll_to_zero() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_state().scroll, 3);
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.logcat_state().scroll, 0);
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn esc_unfocuses_logcat_when_at_bottom() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn left_right_ignored_without_focus() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), None);
        app.handle_key(KeyCode::Right);
        assert_eq!(app.logcat_state().filter.level, None);
    }

    #[test]
    fn r_resets_logcat_filters_and_scroll() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Right);
        assert_eq!(app.logcat_state().filter.tag, "a");
        assert_eq!(app.logcat_state().filter.search, "b");
        assert!(app.logcat_state().filter.level.is_some());
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Up);
        assert_eq!(app.logcat_state().scroll, 2);

        assert!(matches!(app.handle_key(KeyCode::Char('r')), Action::ResetLogcat));
        assert_eq!(app.logcat_state().filter.tag, "");
        assert_eq!(app.logcat_state().filter.search, "");
        assert_eq!(app.logcat_state().filter.level, None);
        assert_eq!(app.logcat_state().scroll, 0);
    }

    #[test]
    fn r_ignored_without_focus() {
        let mut app = App::new();
        assert!(matches!(app.handle_key(KeyCode::Char('r')), Action::None));
    }

    #[test]
    fn scroll_ignored_when_logcat_not_focused() {
        let mut app = App::new();
        assert_eq!(app.focused_panel(), None);
        app.handle_key(KeyCode::Up);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.logcat_state().scroll, 0);
    }

    #[test]
    fn d_focuses_database_panel() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('d'));
        assert_eq!(app.focused_panel(), Some(5));
    }

    #[test]
    fn db_panel_navigate_and_select() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into(), "b.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Down);
        assert_eq!(app.db_state().selected_index, 1);
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.db_state().selected_db.as_deref(), Some("b.db"));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
    }

    #[test]
    fn db_panel_esc_deselects_then_unfocuses() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        assert!(app.db_state().selected_db.is_some());
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.input_mode(), InputMode::Normal);
        app.handle_key(KeyCode::Esc);
        assert!(app.db_state().selected_db.is_none());
        assert_eq!(app.focused_panel(), Some(5));
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn db_panel_select_auto_enters_editing() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
    }

    #[test]
    fn db_panel_e_enters_query_editing() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.input_mode(), InputMode::Normal);
        app.handle_key(KeyCode::Char('e'));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
    }

    #[test]
    fn query_editing_enter_returns_run_query_and_appends_history() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["test.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Char('S'));
        app.handle_key(KeyCode::Char('Q'));
        app.handle_key(KeyCode::Char('L'));
        let action = app.handle_key(KeyCode::Enter);
        assert!(matches!(action, Action::RunQuery(_, _)));
        assert_eq!(app.input_mode(), InputMode::Normal);
        assert!(app.db_state().input.is_empty());
        assert_eq!(app.db_state().history.len(), 1);
    }

    #[test]
    fn e_ignored_without_db_focus() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.db_state_mut().selected_db = Some("a.db".into());
        app.handle_key(KeyCode::Char('e'));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn e_enters_editing_when_focused_with_selected_db() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc); // exit EditingQuery
        app.handle_key(KeyCode::Char('e'));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        assert_eq!(app.focused_panel(), Some(5));
    }

    #[test]
    fn up_down_with_selected_db_scrolls_history_when_focused() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc); // exit EditingQuery
        app.db_state_mut().history = vec![
            crate::database::ReplLine::Input("SELECT 1".into()),
            crate::database::ReplLine::Output("1".into()),
        ];
        app.handle_key(KeyCode::Up);
        assert_eq!(app.db_state().scroll, 1);
    }

    #[test]
    fn up_down_ignored_without_db_focus() {
        let mut app = App::new();
        app.db_state_mut().selected_db = Some("a.db".into());
        app.db_state_mut().history = vec![
            crate::database::ReplLine::Input("SELECT 1".into()),
        ];
        app.handle_key(KeyCode::Up);
        assert_eq!(app.db_state().scroll, 0);
    }

    #[test]
    fn toggle_panel_5_visibility() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('5'));
        assert!(!app.panel_visibility()[4]);
        app.handle_key(KeyCode::Char('5'));
        assert!(app.panel_visibility()[4]);
    }

    #[test]
    fn p_sets_confirming_pull_from_list_view() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into(), "b.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Down);
        let action = app.handle_key(KeyCode::Char('p'));
        assert!(matches!(action, Action::None));
        assert_eq!(app.db_state().confirming_pull.as_deref(), Some("b.db"));
    }

    #[test]
    fn enter_confirms_pull() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Char('p'));
        let action = app.handle_key(KeyCode::Enter);
        assert!(matches!(action, Action::PullDb(db) if db == "a.db"));
        assert!(app.db_state().confirming_pull.is_none());
    }

    #[test]
    fn any_key_cancels_pull_confirmation() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Char('p'));
        assert!(app.db_state().confirming_pull.is_some());
        let action = app.handle_key(KeyCode::Esc);
        assert!(matches!(action, Action::None));
        assert!(app.db_state().confirming_pull.is_none());
    }

    #[test]
    fn p_sets_confirming_pull_from_repl_view() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc);
        let action = app.handle_key(KeyCode::Char('p'));
        assert!(matches!(action, Action::None));
        assert_eq!(app.db_state().confirming_pull.as_deref(), Some("a.db"));
    }

    #[test]
    fn toggle_focus_clears_tag_editing() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('t'));
        assert_eq!(app.input_mode(), InputMode::EditingTag);
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn toggle_focus_clears_search_editing() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('l'));
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.input_mode(), InputMode::EditingSearch);
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn switching_focus_clears_query_editing() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('e'));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('d'));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn r_ignored_in_db_list_view() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        assert!(matches!(app.handle_key(KeyCode::Char('r')), Action::None));
        assert!(!app.db_state().databases.is_empty());
    }

    #[test]
    fn r_resets_db_from_repl_view() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc);
        assert!(app.db_state().selected_db.is_some());
        assert!(matches!(app.handle_key(KeyCode::Char('r')), Action::ResetDb));
        assert!(app.db_state().selected_db.is_none());
        assert!(app.db_state().databases.is_empty());
    }

    #[test]
    fn c_copies_full_history() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc);
        app.db_state_mut().push_query("SELECT 1");
        app.db_state_mut().push_result("1");
        let action = app.handle_key(KeyCode::Char('c'));
        assert!(matches!(action, Action::CopyDbResult(ref s) if s == "> SELECT 1\n1"));
    }

    #[test]
    fn c_noop_without_output() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.handle_key(KeyCode::Char('c')), Action::None));
    }

    #[test]
    fn c_falls_through_to_clear_data_in_list_view() {
        let mut app = App::new();
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(KeyCode::Char('d'));
        assert!(matches!(app.handle_key(KeyCode::Char('c')), Action::ClearData));
    }
}
