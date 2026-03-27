use std::sync::{mpsc, Arc};

use crossterm::event::KeyCode;

use crate::adb::Adb;
use crate::app::Action;
use crate::issues;

pub struct AnrEntry {
    pub timestamp: String,
    pub process: String,
    pub reason: String,
    pub full_text: String,
}

pub struct AnrsState {
    pub anrs: Vec<AnrEntry>,
    pub selected: usize,
    pub error: Option<String>,
}

impl AnrsState {
    pub fn new() -> Self {
        Self {
            anrs: Vec::new(),
            selected: 0,
            error: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<Action> {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.anrs.is_empty() {
                    self.selected = (self.selected + 1).min(self.anrs.len() - 1);
                }
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if let Some(entry) = self.anrs.get(self.selected) {
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

pub fn spawn_loader(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<Vec<AnrEntry>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .get_dropbox_anrs(&serial)
            .map(|output| {
                issues::parse_dropbox_entries(&output, "data_app_anr", &package)
                    .into_iter()
                    .map(|(timestamp, body)| {
                        let process = issues::extract_field(&body, "Process: ").unwrap_or_default();
                        let reason = issues::extract_field(&body, "Subject: ")
                            .or_else(|| issues::extract_field(&body, "Reason: "))
                            .unwrap_or_default();
                        AnrEntry { timestamp, process, reason, full_text: body }
                    })
                    .collect()
            })
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigate_list() {
        let mut state = AnrsState::new();
        state.anrs = vec![
            AnrEntry { timestamp: "t1".into(), process: "p".into(), reason: "r1".into(), full_text: "f1".into() },
            AnrEntry { timestamp: "t2".into(), process: "p".into(), reason: "r2".into(), full_text: "f2".into() },
        ];
        assert_eq!(state.selected, 0);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.selected, 1);
        state.handle_key(KeyCode::Char('k'));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn enter_opens_in_editor() {
        let mut state = AnrsState::new();
        state.anrs = vec![
            AnrEntry { timestamp: "t".into(), process: "p".into(), reason: "r".into(), full_text: "full".into() },
        ];
        let action = state.handle_key(KeyCode::Enter);
        assert!(matches!(action, Some(Action::OpenInEditor(ref s)) if s == "full"));
    }

    #[test]
    fn esc_unfocuses() {
        let mut state = AnrsState::new();
        let action = state.handle_key(KeyCode::Esc);
        assert!(matches!(action, Some(Action::Unfocus)));
    }
}
