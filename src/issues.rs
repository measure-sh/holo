use crossterm::event::{KeyCode, KeyEvent};

use crate::anrs::AnrEntry;
use crate::app::Action;
use crate::crashes::CrashEntry;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IssueKind {
    Crash,
    Anr,
}

pub struct IssueEntry {
    pub kind: IssueKind,
    pub timestamp: String,
    pub description: String,
    pub full_text: String,
}

pub struct IssuesState {
    crashes: Vec<CrashEntry>,
    anrs: Vec<AnrEntry>,
    pub issues: Vec<IssueEntry>,
    pub selected: usize,
    pub crash_error: Option<String>,
    pub anr_error: Option<String>,
}

impl IssuesState {
    pub fn new() -> Self {
        Self {
            crashes: Vec::new(),
            anrs: Vec::new(),
            issues: Vec::new(),
            selected: 0,
            crash_error: None,
            anr_error: None,
        }
    }

    pub fn set_crashes(&mut self, crashes: Vec<CrashEntry>) {
        self.crashes = crashes;
        self.crash_error = None;
        self.rebuild();
    }

    pub fn set_crash_error(&mut self, e: String) {
        self.crash_error = Some(e);
    }

    pub fn set_anrs(&mut self, anrs: Vec<AnrEntry>) {
        self.anrs = anrs;
        self.anr_error = None;
        self.rebuild();
    }

    pub fn set_anr_error(&mut self, e: String) {
        self.anr_error = Some(e);
    }

    pub fn error(&self) -> Option<&str> {
        self.crash_error
            .as_deref()
            .or(self.anr_error.as_deref())
    }

    fn rebuild(&mut self) {
        let mut merged: Vec<IssueEntry> =
            Vec::with_capacity(self.crashes.len() + self.anrs.len());
        for c in &self.crashes {
            merged.push(IssueEntry {
                kind: IssueKind::Crash,
                timestamp: c.timestamp.clone(),
                description: c.exception.clone(),
                full_text: c.full_text.clone(),
            });
        }
        for a in &self.anrs {
            merged.push(IssueEntry {
                kind: IssueKind::Anr,
                timestamp: a.timestamp.clone(),
                description: a.reason.clone(),
                full_text: a.full_text.clone(),
            });
        }
        merged.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        self.issues = merged;
        if self.issues.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.issues.len() - 1);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.issues.is_empty() {
                    self.selected = (self.selected + 1).min(self.issues.len() - 1);
                }
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if let Some(entry) = self.issues.get(self.selected) {
                    Some(Action::OpenInEditor(entry.full_text.clone()))
                } else {
                    Some(Action::Noop)
                }
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn merges_and_sorts_by_timestamp() {
        let mut state = IssuesState::new();
        state.set_crashes(vec![
            CrashEntry { timestamp: "2024-01-15 10:00:00".into(), exception: "NPE".into(), full_text: "c1".into() },
            CrashEntry { timestamp: "2024-01-15 12:00:00".into(), exception: "OOM".into(), full_text: "c2".into() },
        ]);
        state.set_anrs(vec![
            AnrEntry { timestamp: "2024-01-15 11:00:00".into(), reason: "Input timeout".into(), full_text: "a1".into() },
        ]);
        assert_eq!(state.issues.len(), 3);
        assert_eq!(state.issues[0].timestamp, "2024-01-15 12:00:00");
        assert_eq!(state.issues[0].kind, IssueKind::Crash);
        assert_eq!(state.issues[1].timestamp, "2024-01-15 11:00:00");
        assert_eq!(state.issues[1].kind, IssueKind::Anr);
        assert_eq!(state.issues[2].timestamp, "2024-01-15 10:00:00");
        assert_eq!(state.issues[2].kind, IssueKind::Crash);
    }

    #[test]
    fn navigate_list() {
        let mut state = IssuesState::new();
        state.set_crashes(vec![
            CrashEntry { timestamp: "t1".into(), exception: "e1".into(), full_text: "f1".into() },
            CrashEntry { timestamp: "t2".into(), exception: "e2".into(), full_text: "f2".into() },
        ]);
        assert_eq!(state.selected, 0);
        state.handle_key(key(KeyCode::Char('j')));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Char('j')));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Char('k')));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn enter_opens_in_editor() {
        let mut state = IssuesState::new();
        state.set_crashes(vec![
            CrashEntry { timestamp: "t".into(), exception: "e".into(), full_text: "full".into() },
        ]);
        let action = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Some(Action::OpenInEditor(ref s)) if s == "full"));
    }

    #[test]
    fn esc_unfocuses() {
        let state = &mut IssuesState::new();
        let action = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Unfocus)));
    }
}
