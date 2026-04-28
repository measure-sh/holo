use std::path::PathBuf;

use crossterm::event::KeyCode;

use crate::adb::Device;
use crate::apps;
use crate::selector;
use crate::session;

fn cache_path() -> PathBuf {
    // Shares the single `dirs::cache_dir()` fallback with `session::sessions_root`
    // so the previous `PathBuf::from(".")` cwd fallback (which would land
    // `last_package` in whatever directory holo was launched from) is gone.
    session::cache_root().join("last_package")
}

pub fn load_last_package() -> Option<String> {
    std::fs::read_to_string(cache_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_last_package(package: &str) {
    let path = cache_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(path, package);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropdownKind {
    Device,
    App,
}

pub enum ToolbarAction {
    None,
    Close,
    SelectDevice(Device),
    SelectApp(String),
    LaunchEmulator(String),
}

pub struct ToolbarState {
    pub device: Option<Device>,
    pub package: Option<String>,
    pub last_package: Option<String>,
    pub devices: Vec<Device>,
    pub packages: Vec<String>,
    pub cursor: usize,
    pub filter: String,
    pub loading: bool,
    pub device_connected: bool,
    /// `Some` while a captured session is open — the toolbar renders this
    /// in place of the device/app dropdowns to surface that the user is
    /// looking at history. Cleared on `Action::CloseSession` and on
    /// app/device change.
    pub session_label: Option<String>,
}

impl ToolbarState {
    pub fn new(device: Option<Device>) -> Self {
        Self {
            device,
            package: None,
            last_package: load_last_package(),
            devices: Vec::new(),
            packages: Vec::new(),
            cursor: 0,
            filter: String::new(),
            loading: false,
            device_connected: true,
            session_label: None,
        }
    }

    pub fn device_label(&self) -> String {
        match &self.device {
            Some(d) => selector::selector_label(d),
            None => "no device".to_string(),
        }
    }

    pub fn app_label(&self) -> String {
        match &self.package {
            Some(p) => p.clone(),
            None => "no app".to_string(),
        }
    }

    /// Reset the dropdown's transient UI state. Called by `App::open_*`
    /// setters when the user opens a dropdown — the kind itself lives on
    /// `App::Navigation`, so this struct doesn't need to track it. The
    /// `loading` flag is supplied by the caller because only the App
    /// knows whether fresh data is already cached.
    pub fn prepare_for_dropdown(&mut self, loading: bool) {
        self.cursor = 0;
        self.filter.clear();
        self.loading = loading;
    }

    pub fn receive_devices(&mut self, devices: Vec<Device>) {
        self.devices = devices;
        self.loading = false;
    }

    pub fn receive_packages(&mut self, packages: Vec<String>) -> Option<String> {
        self.packages = packages;
        self.loading = false;
        if self.package.is_none()
            && let Some(last) = &self.last_package
            && self.packages.iter().any(|p| p == last)
        {
            return Some(last.clone());
        }
        None
    }

    pub fn filtered_devices(&self) -> Vec<&Device> {
        self.devices
            .iter()
            .filter(|d| {
                let label = selector::selector_label(d);
                apps::fuzzy_matches(&label, &self.filter)
            })
            .collect()
    }

    pub fn filtered_packages(&self) -> Vec<&str> {
        apps::filtered_packages(&self.packages, &self.filter)
    }

    /// Process a key while the dropdown of the given kind is open. The kind
    /// is passed in (rather than stored on the struct) because `App` owns
    /// the navigation enum that says which dropdown is open.
    pub fn handle_key(&mut self, code: KeyCode, kind: DropdownKind) -> ToolbarAction {
        match code {
            KeyCode::Esc => ToolbarAction::Close,
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                ToolbarAction::None
            }
            KeyCode::Down => {
                let count = self.filtered_count(kind);
                self.cursor = (self.cursor + 1).min(count.saturating_sub(1));
                ToolbarAction::None
            }
            KeyCode::Enter => {
                let result = match kind {
                    DropdownKind::Device => {
                        let filtered = self.filtered_devices();
                        filtered.get(self.cursor).map(|d| {
                            if d.connected {
                                ToolbarAction::SelectDevice((*d).clone())
                            } else {
                                ToolbarAction::LaunchEmulator(d.serial.clone())
                            }
                        })
                    }
                    DropdownKind::App => {
                        let filtered = self.filtered_packages();
                        filtered.get(self.cursor).map(|p| ToolbarAction::SelectApp(p.to_string()))
                    }
                };
                result.unwrap_or(ToolbarAction::None)
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.cursor = 0;
                ToolbarAction::None
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.cursor = 0;
                ToolbarAction::None
            }
            _ => ToolbarAction::None,
        }
    }

    fn filtered_count(&self, kind: DropdownKind) -> usize {
        match kind {
            DropdownKind::Device => self.filtered_devices().len(),
            DropdownKind::App => self.filtered_packages().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_device(serial: &str, model: Option<&str>) -> Device {
        Device {
            serial: serial.to_string(),
            model: model.map(String::from),
            device: None,
            connected: true,
        }
    }

    #[test]
    fn new_state_has_no_selection() {
        let state = ToolbarState::new(None);
        assert!(state.device.is_none());
        assert!(state.package.is_none());
    }

    #[test]
    fn device_label_with_device() {
        let state = ToolbarState::new(Some(make_device("ABC", Some("Pixel 7"))));
        assert_eq!(state.device_label(), "ABC: Pixel 7");
    }

    #[test]
    fn device_label_without_device() {
        let state = ToolbarState::new(None);
        assert_eq!(state.device_label(), "no device");
    }

    #[test]
    fn app_label_with_package() {
        let mut state = ToolbarState::new(None);
        state.package = Some("com.example.app".to_string());
        assert_eq!(state.app_label(), "com.example.app");
    }

    #[test]
    fn app_label_without_package() {
        let state = ToolbarState::new(None);
        assert_eq!(state.app_label(), "no app");
    }

    #[test]
    fn prepare_resets_cursor_and_loading() {
        let mut state = ToolbarState::new(None);
        state.cursor = 5;
        state.filter = "stale".into();
        state.loading = false;
        state.prepare_for_dropdown(true);
        assert_eq!(state.cursor, 0);
        assert!(state.filter.is_empty());
        assert!(state.loading);
    }

    #[test]
    fn prepare_can_keep_loading_off_when_data_is_cached() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(false);
        assert!(!state.loading);
    }

    #[test]
    fn receive_devices_populates_and_clears_loading() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_devices(vec![make_device("A", None), make_device("B", None)]);
        assert!(!state.loading);
        assert_eq!(state.devices.len(), 2);
    }

    #[test]
    fn receive_packages_populates_and_clears_loading() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_packages(vec!["com.a".to_string(), "com.b".to_string()]);
        assert!(!state.loading);
        assert_eq!(state.packages.len(), 2);
    }

    #[test]
    fn typing_builds_filter() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_packages(vec!["com.example".to_string()]);
        state.handle_key(KeyCode::Char('c'), DropdownKind::App);
        state.handle_key(KeyCode::Char('o'), DropdownKind::App);
        assert_eq!(state.filter, "co");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn backspace_removes_char() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.filter = "abc".to_string();
        state.handle_key(KeyCode::Backspace, DropdownKind::App);
        assert_eq!(state.filter, "ab");
    }

    #[test]
    fn esc_returns_close_action() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        let action = state.handle_key(KeyCode::Esc, DropdownKind::App);
        assert!(matches!(action, ToolbarAction::Close));
    }

    #[test]
    fn cursor_moves_within_bounds() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_packages(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        state.handle_key(KeyCode::Down, DropdownKind::App);
        assert_eq!(state.cursor, 1);
        state.handle_key(KeyCode::Down, DropdownKind::App);
        assert_eq!(state.cursor, 2);
        state.handle_key(KeyCode::Down, DropdownKind::App);
        assert_eq!(state.cursor, 2);
        state.handle_key(KeyCode::Up, DropdownKind::App);
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn enter_selects_app() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_packages(vec!["com.alpha".to_string(), "com.beta".to_string()]);
        state.handle_key(KeyCode::Down, DropdownKind::App);
        let action = state.handle_key(KeyCode::Enter, DropdownKind::App);
        assert!(matches!(action, ToolbarAction::SelectApp(ref p) if p == "com.beta"));
    }

    #[test]
    fn enter_selects_device() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_devices(vec![make_device("DEV1", None), make_device("DEV2", None)]);
        state.handle_key(KeyCode::Down, DropdownKind::Device);
        let action = state.handle_key(KeyCode::Enter, DropdownKind::Device);
        assert!(matches!(action, ToolbarAction::SelectDevice(ref d) if d.serial == "DEV2"));
    }

    #[test]
    fn enter_does_nothing_when_empty() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_packages(vec![]);
        let action = state.handle_key(KeyCode::Enter, DropdownKind::App);
        assert!(matches!(action, ToolbarAction::None));
    }

    #[test]
    fn filter_narrows_packages() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_packages(vec![
            "com.spotify.music".to_string(),
            "com.whatsapp".to_string(),
        ]);
        state.filter = "spot".to_string();
        let filtered = state.filtered_packages();
        assert_eq!(filtered, vec!["com.spotify.music"]);
    }

    #[test]
    fn filter_narrows_devices() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_devices(vec![
            make_device("A", Some("Pixel 7")),
            make_device("B", Some("Samsung")),
        ]);
        state.filter = "pix".to_string();
        let filtered = state.filtered_devices();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].serial, "A");
    }

    #[test]
    fn auto_selects_last_package_when_available() {
        let mut state = ToolbarState::new(None);
        state.last_package = Some("com.example.app".to_string());
        let result = state.receive_packages(vec![
            "com.other".to_string(),
            "com.example.app".to_string(),
        ]);
        assert_eq!(result, Some("com.example.app".to_string()));
    }

    #[test]
    fn no_auto_select_when_last_package_missing() {
        let mut state = ToolbarState::new(None);
        state.last_package = Some("com.example.app".to_string());
        let result = state.receive_packages(vec!["com.other".to_string()]);
        assert_eq!(result, None);
    }

    #[test]
    fn no_auto_select_when_package_already_set() {
        let mut state = ToolbarState::new(None);
        state.package = Some("com.current".to_string());
        state.last_package = Some("com.example.app".to_string());
        let result = state.receive_packages(vec!["com.example.app".to_string()]);
        assert_eq!(result, None);
    }

    #[test]
    fn no_auto_select_without_cache() {
        let mut state = ToolbarState::new(None);
        let result = state.receive_packages(vec!["com.example.app".to_string()]);
        assert_eq!(result, None);
    }

    #[test]
    fn enter_launches_offline_emulator() {
        let mut state = ToolbarState::new(None);
        state.prepare_for_dropdown(true);
        state.receive_devices(vec![Device {
            serial: "Pixel_7_API_34".to_string(),
            model: Some("Pixel_7_API_34".to_string()),
            device: None,
            connected: false,
        }]);
        let action = state.handle_key(KeyCode::Enter, DropdownKind::Device);
        assert!(matches!(action, ToolbarAction::LaunchEmulator(ref name) if name == "Pixel_7_API_34"));
    }
}
