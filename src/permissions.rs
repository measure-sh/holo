use std::sync::{mpsc, Arc};

use crate::adb::Adb;

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

    pub fn selected_permission(&self) -> Option<&(String, bool)> {
        self.permissions.get(self.selected_index)
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

pub fn spawn_permissions_loader(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<Vec<(String, bool)>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .list_permissions(&serial, &package)
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
            state.selected_permission(),
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
