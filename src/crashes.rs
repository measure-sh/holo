use std::sync::{mpsc, Arc};

use crossterm::event::KeyCode;

use crate::adb::Adb;
use crate::app::Action;
use crate::dropbox;

pub struct CrashEntry {
    pub timestamp: String,
    pub exception: String,
    pub full_text: String,
}

pub struct CrashesState {
    pub crashes: Vec<CrashEntry>,
    pub selected: usize,
    pub error: Option<String>,
}

impl CrashesState {
    pub fn new() -> Self {
        Self {
            crashes: Vec::new(),
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
                if !self.crashes.is_empty() {
                    self.selected = (self.selected + 1).min(self.crashes.len() - 1);
                }
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if let Some(entry) = self.crashes.get(self.selected) {
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

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<Vec<CrashEntry>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(5);
        loop {
            let result = adb
                .get_dropbox_crashes(&serial)
                .map(|output| {
                    let mut entries: Vec<CrashEntry> = dropbox::parse_dropbox_entries(&output, "data_app_crash", &package)
                        .into_iter()
                        .map(|(timestamp, body)| {
                            let exception = dropbox::extract_exception(&body);
                            CrashEntry { timestamp, exception, full_text: body }
                        })
                        .collect();
                    entries.reverse();
                    entries
                })
                .map_err(|e| e.to_string());
            if tx.send(result).is_err() {
                return;
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigate_list() {
        let mut state = CrashesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), exception: "e1".into(), full_text: "f1".into() },
            CrashEntry { timestamp: "t2".into(), exception: "e2".into(), full_text: "f2".into() },
        ];
        assert_eq!(state.selected, 0);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.selected, 1);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.selected, 1);
        state.handle_key(KeyCode::Char('k'));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn enter_opens_in_editor() {
        let mut state = CrashesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t".into(), exception: "e".into(), full_text: "full".into() },
        ];
        let action = state.handle_key(KeyCode::Enter);
        assert!(matches!(action, Some(Action::OpenInEditor(ref s)) if s == "full"));
    }

    #[test]
    fn esc_unfocuses() {
        let mut state = CrashesState::new();
        let action = state.handle_key(KeyCode::Esc);
        assert!(matches!(action, Some(Action::Unfocus)));
    }
}
