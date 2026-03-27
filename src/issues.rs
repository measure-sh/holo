use crossterm::event::KeyCode;

use crate::app::Action;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IssuesTab {
    Crashes,
    Anrs,
}

pub struct CrashEntry {
    pub timestamp: String,
    pub process: String,
    pub exception: String,
    pub full_text: String,
}

pub struct AnrEntry {
    pub timestamp: String,
    pub process: String,
    pub reason: String,
    pub full_text: String,
}

pub struct IssuesState {
    pub active_tab: IssuesTab,
    pub crashes: Vec<CrashEntry>,
    pub anrs: Vec<AnrEntry>,
    pub crash_selected: usize,
    pub anr_selected: usize,
    pub viewing_detail: bool,
    pub error: Option<String>,
}

impl IssuesState {
    pub fn new() -> Self {
        Self {
            active_tab: IssuesTab::Crashes,
            crashes: Vec::new(),
            anrs: Vec::new(),
            crash_selected: 0,
            anr_selected: 0,
            viewing_detail: false,
            error: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<Action> {
        match code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.active_tab = IssuesTab::Crashes;
                Some(Action::Noop)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.active_tab = IssuesTab::Anrs;
                Some(Action::Noop)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if self.has_entries() {
                    self.viewing_detail = !self.viewing_detail;
                }
                Some(Action::Noop)
            }
            KeyCode::Esc => {
                if self.viewing_detail {
                    self.viewing_detail = false;
                    Some(Action::Noop)
                } else {
                    Some(Action::Unfocus)
                }
            }
            _ => None,
        }
    }

    fn move_up(&mut self) {
        match self.active_tab {
            IssuesTab::Crashes => {
                self.crash_selected = self.crash_selected.saturating_sub(1);
            }
            IssuesTab::Anrs => {
                self.anr_selected = self.anr_selected.saturating_sub(1);
            }
        }
        self.viewing_detail = false;
    }

    fn move_down(&mut self) {
        match self.active_tab {
            IssuesTab::Crashes => {
                if !self.crashes.is_empty() {
                    self.crash_selected = (self.crash_selected + 1).min(self.crashes.len() - 1);
                }
            }
            IssuesTab::Anrs => {
                if !self.anrs.is_empty() {
                    self.anr_selected = (self.anr_selected + 1).min(self.anrs.len() - 1);
                }
            }
        }
        self.viewing_detail = false;
    }

    fn has_entries(&self) -> bool {
        match self.active_tab {
            IssuesTab::Crashes => !self.crashes.is_empty(),
            IssuesTab::Anrs => !self.anrs.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_defaults() {
        let state = IssuesState::new();
        assert_eq!(state.active_tab, IssuesTab::Crashes);
        assert!(state.crashes.is_empty());
        assert!(state.anrs.is_empty());
        assert!(!state.viewing_detail);
    }

    #[test]
    fn h_l_switches_tabs() {
        let mut state = IssuesState::new();
        state.handle_key(KeyCode::Char('l'));
        assert_eq!(state.active_tab, IssuesTab::Anrs);
        state.handle_key(KeyCode::Char('h'));
        assert_eq!(state.active_tab, IssuesTab::Crashes);
    }

    #[test]
    fn navigate_crashes() {
        let mut state = IssuesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), process: "p".into(), exception: "e1".into(), full_text: "".into() },
            CrashEntry { timestamp: "t2".into(), process: "p".into(), exception: "e2".into(), full_text: "".into() },
        ];
        assert_eq!(state.crash_selected, 0);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.crash_selected, 1);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.crash_selected, 1);
        state.handle_key(KeyCode::Char('k'));
        assert_eq!(state.crash_selected, 0);
    }

    #[test]
    fn enter_toggles_detail() {
        let mut state = IssuesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), process: "p".into(), exception: "e".into(), full_text: "full".into() },
        ];
        assert!(!state.viewing_detail);
        state.handle_key(KeyCode::Enter);
        assert!(state.viewing_detail);
        state.handle_key(KeyCode::Enter);
        assert!(!state.viewing_detail);
    }

    #[test]
    fn esc_closes_detail_first() {
        let mut state = IssuesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), process: "p".into(), exception: "e".into(), full_text: "full".into() },
        ];
        state.handle_key(KeyCode::Enter);
        assert!(state.viewing_detail);
        let action = state.handle_key(KeyCode::Esc);
        assert!(!state.viewing_detail);
        assert!(matches!(action, Some(Action::Noop)));
    }

    #[test]
    fn esc_unfocuses_when_no_detail() {
        let mut state = IssuesState::new();
        let action = state.handle_key(KeyCode::Esc);
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn enter_noop_on_empty() {
        let mut state = IssuesState::new();
        state.handle_key(KeyCode::Enter);
        assert!(!state.viewing_detail);
    }
}
