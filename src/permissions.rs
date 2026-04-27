use std::sync::{mpsc, Arc};

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Adb;
use crate::app::Action;

pub struct PermissionsState {
    pub permissions: Vec<(String, bool)>,
    pub selected_index: usize,
    pub error: Option<String>,
}

impl PermissionsState {
    pub fn new() -> Self {
        Self {
            permissions: Vec::new(),
            selected_index: 0,
            error: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if let Some((perm, granted)) = self.toggle_selected() {
                    Some(Action::TogglePermission(perm, granted))
                } else {
                    Some(Action::Noop)
                }
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }

    pub fn move_up(&mut self) {
        if !self.permissions.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        if !self.permissions.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.permissions.len() - 1);
        }
    }

    pub fn toggle_selected(&mut self) -> Option<(String, bool)> {
        if let Some(perm) = self.permissions.get_mut(self.selected_index) {
            perm.1 = !perm.1;
            Some((perm.0.clone(), perm.1))
        } else {
            None
        }
    }

    pub fn short_name(permission: &str) -> String {
        permission
            .strip_prefix("android.permission.")
            .unwrap_or(permission)
            .to_lowercase()
    }
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    connectivity: Arc<std::sync::atomic::AtomicBool>,
) -> mpsc::Receiver<Result<Vec<(String, bool)>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(5);
        loop {
            if connectivity.load(std::sync::atomic::Ordering::Relaxed) {
                let result = adb
                    .list_permissions(&serial, &package)
                    .map_err(|e| e.to_string());
                if tx.send(result).is_err() {
                    return;
                }
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
        let mut state = PermissionsState::new();
        state.permissions = vec![
            ("android.permission.CAMERA".into(), true),
            ("android.permission.LOCATION".into(), false),
            ("android.permission.CONTACTS".into(), true),
        ];
        assert_eq!(state.selected_index, 0);
        state.move_down();
        assert_eq!(state.selected_index, 1);
        state.move_down();
        assert_eq!(state.selected_index, 2);
        state.move_down();
        assert_eq!(state.selected_index, 2);
        state.move_up();
        assert_eq!(state.selected_index, 1);
        state.move_up();
        assert_eq!(state.selected_index, 0);
        state.move_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn toggle_selected() {
        let mut state = PermissionsState::new();
        state.permissions = vec![
            ("android.permission.CAMERA".into(), false),
        ];
        let result = state.toggle_selected();
        assert_eq!(result, Some(("android.permission.CAMERA".into(), true)));
        assert!(state.permissions[0].1);
    }

    #[test]
    fn toggle_empty_returns_none() {
        let mut state = PermissionsState::new();
        assert_eq!(state.toggle_selected(), None);
    }

    #[test]
    fn short_name_strips_prefix() {
        assert_eq!(PermissionsState::short_name("android.permission.CAMERA"), "camera");
        assert_eq!(PermissionsState::short_name("com.custom.PERM"), "com.custom.perm");
    }

    #[test]
    fn selected_permission_returns_correct() {
        let mut state = PermissionsState::new();
        state.permissions = vec![
            ("android.permission.CAMERA".into(), true),
            ("android.permission.LOCATION".into(), false),
        ];
        state.selected_index = 1;
        assert_eq!(
            state.permissions.get(state.selected_index),
            Some(&("android.permission.LOCATION".into(), false))
        );
    }

    #[test]
    fn navigate_empty_list() {
        let mut state = PermissionsState::new();
        state.move_up();
        state.move_down();
        assert_eq!(state.selected_index, 0);
    }
}
