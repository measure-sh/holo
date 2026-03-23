use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Device;
use crate::apps;
use crate::database::DatabaseState;
use crate::files::{FileConfirm, FilesState, ToggleResult};
use crate::logcat_state::LogcatState;
use crate::monitor::MonitorState;
use crate::panel;
use crate::permissions::PermissionsState;
use crate::toolbar::{ToolbarAction, ToolbarState};
use crate::trace::{self, TraceState};

pub const COMMAND_LIST: &[(&str, fn() -> Action)] = &[
    ("open app", || Action::OpenApp),
    ("kill app", || Action::KillApp),
    ("wakeup device", || Action::WakeScreen),
    ("clear data", || Action::ClearData),
    ("take screenshot", || Action::Screenshot),
    ("layout bounds", || Action::ToggleLayoutBounds),
    ("wifi", || Action::ToggleWifi),
    ("airplane mode", || Action::ToggleAirplaneMode),
    ("connect wireless adb", || Action::WirelessAdb),
    ("uninstall app", || Action::UninstallApp),
];

pub enum Action {
    Quit,
    ChangeDevice(Device),
    ChangeApp(String),
    FetchDevices,
    FetchApps,
    Noop,
    Unfocus,
    OpenApp,
    KillApp,
    ClearData,
    ResetLogcat,
    ResetDb,
    CopyDbResult(String),
    RunQuery(String, String),
    PullDb(String),
    UninstallApp,
    WakeScreen,
    ToggleLayoutBounds,
    ToggleAirplaneMode,
    TogglePermission(String, bool),
    CopyLogcat,
    RefreshFiles,
    ExpandDir(String),
    PullFile(String),
    OpenFile(String),
    StartTrace,
    StopTrace,
    Screenshot,
    ToggleWifi,
    WirelessAdb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    EditingQuery,
}

pub struct App {
    commands_visible: bool,
    visible: [bool; 8],
    focused: Option<u8>,
    input_mode: InputMode,
    logcat_state: LogcatState,
    db_state: DatabaseState,
    permissions_state: PermissionsState,
    files_state: FilesState,
    monitor_state: MonitorState,
    toolbar: ToolbarState,
    commands_cursor: usize,
    commands_filter: String,
    package: String,
    layout_bounds: bool,
    airplane_mode: bool,
    wifi_enabled: bool,
    confirming_quit: bool,
    trace_state: TraceState,
}

impl App {
    pub fn new(device: Option<Device>, package: Option<&str>) -> Self {
        let pkg = package.unwrap_or_default();
        let mut toolbar = ToolbarState::new(device);
        toolbar.package = package.map(String::from);
        Self {
            commands_visible: true,
            visible: [true; 8],
            focused: None,
            input_mode: InputMode::Normal,
            logcat_state: LogcatState::new(),
            db_state: DatabaseState::new(),
            permissions_state: PermissionsState::new(),
            files_state: FilesState::new(pkg),
            monitor_state: MonitorState::new(),
            toolbar,
            commands_cursor: 0,
            commands_filter: String::new(),
            package: pkg.to_string(),
            layout_bounds: false,
            airplane_mode: false,
            wifi_enabled: false,
            confirming_quit: false,
            trace_state: TraceState::new(pkg),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        let code = key.code;
        if self.logcat_state.editing.is_some() {
            return self.logcat_state.handle_key(code).unwrap_or(Action::Noop);
        }

        match self.input_mode {
            InputMode::EditingQuery => {
                match code {
                    KeyCode::Enter => {
                        if let Some(db) = self.db_state.selected_db.clone() {
                            if let Some(sql) = self.db_state.submit_query() {
                                return Action::RunQuery(db, sql);
                            }
                        }
                    }
                    KeyCode::Esc => self.input_mode = InputMode::Normal,
                    KeyCode::Up => self.db_state.history_up(),
                    KeyCode::Down => self.db_state.history_down(),
                    _ => { self.db_state.textarea.input(key); }
                }
                return Action::Noop;
            }
            InputMode::Normal => {}
        }

        if self.toolbar.open.is_some() {
            match code {
                KeyCode::F(1) => {
                    self.toolbar.open_devices();
                    return Action::FetchDevices;
                }
                KeyCode::F(2) => {
                    self.toolbar.open_apps();
                    return Action::FetchApps;
                }
                _ => {
                    return match self.toolbar.handle_key(code) {
                        ToolbarAction::SelectDevice(d) => Action::ChangeDevice(d),
                        ToolbarAction::SelectApp(p) => Action::ChangeApp(p),
                        ToolbarAction::Close | ToolbarAction::None => Action::Noop,
                    };
                }
            }
        }

        if self.confirming_quit {
            self.confirming_quit = false;
            return match code {
                KeyCode::Char('q') => Action::Quit,
                _ => Action::Noop,
            };
        }

        if self.focused == Some(panel::COMMANDS) {
            match code {
                KeyCode::Up => {
                    self.commands_cursor = self.commands_cursor.saturating_sub(1);
                    return Action::Noop;
                }
                KeyCode::Down => {
                    let count = self.filtered_commands().len();
                    self.commands_cursor = (self.commands_cursor + 1).min(count.saturating_sub(1));
                    return Action::Noop;
                }
                KeyCode::Enter => {
                    let filtered = self.filtered_commands();
                    if let Some((_, action_fn)) = filtered.get(self.commands_cursor) {
                        return action_fn();
                    }
                    return Action::Noop;
                }
                KeyCode::Esc => {
                    self.commands_filter.clear();
                    self.commands_cursor = 0;
                    self.focused = None;
                    return Action::Noop;
                }
                KeyCode::Backspace => {
                    self.commands_filter.pop();
                    self.commands_cursor = 0;
                    return Action::Noop;
                }
                KeyCode::Char(ch) => {
                    self.commands_filter.push(ch);
                    self.commands_cursor = 0;
                    return Action::Noop;
                }
                _ => { return Action::Noop; }
            }
        }

        if self.db_state.confirming_pull.is_some() {
            return match code {
                KeyCode::Char('p') => {
                    let db = self.db_state.confirming_pull.take().unwrap();
                    Action::PullDb(db)
                }
                _ => {
                    self.db_state.confirming_pull = None;
                    Action::Noop
                }
            };
        }

        if self.focused == Some(panel::DATABASE) {
            match code {
                KeyCode::Up | KeyCode::Down if self.db_state.selected_db.is_none() => {
                    if code == KeyCode::Up {
                        self.db_state.move_up();
                    } else {
                        self.db_state.move_down();
                    }
                    return Action::Noop;
                }
                KeyCode::Char('p') if self.db_state.selected_db.is_none() => {
                    if let Some(db) = self.db_state.databases.get(self.db_state.selected_index).cloned() {
                        self.db_state.confirming_pull = Some(db);
                    }
                    return Action::Noop;
                }
                KeyCode::Enter if self.db_state.selected_db.is_none() => {
                    self.db_state.select_db();
                    if self.db_state.selected_db.is_some() {
                        self.input_mode = InputMode::EditingQuery;
                    }
                    return Action::Noop;
                }
                KeyCode::Char('p') if self.db_state.selected_db.is_some() => {
                    self.db_state.confirming_pull = self.db_state.selected_db.clone();
                    return Action::Noop;
                }
                KeyCode::Char('e') if self.db_state.selected_db.is_some() => {
                    self.input_mode = InputMode::EditingQuery;
                    return Action::Noop;
                }
                KeyCode::Char('c') if self.db_state.selected_db.is_some() => {
                    if let Some(text) = self.db_state.history_text() {
                        self.db_state.copied_at = Some(std::time::Instant::now());
                        return Action::CopyDbResult(text);
                    }
                    return Action::Noop;
                }
                KeyCode::Char('r') => {
                    self.db_state.reset();
                    return Action::ResetDb;
                }
                KeyCode::Up if self.db_state.selected_db.is_some() => {
                    self.db_state.move_up();
                    return Action::Noop;
                }
                KeyCode::Down if self.db_state.selected_db.is_some() => {
                    self.db_state.move_down();
                    return Action::Noop;
                }
                KeyCode::Esc if self.db_state.selected_db.is_some() => {
                    self.db_state.deselect_db();
                    return Action::Noop;
                }
                KeyCode::Esc => {
                    self.focused = None;
                    return Action::Noop;
                }
                _ => {}
            }
        }

        if self.files_state.confirming.is_some() {
            return match code {
                KeyCode::Char('p') if matches!(self.files_state.confirming, Some(FileConfirm::Pull(_))) => {
                    let confirm = self.files_state.confirming.take().unwrap();
                    match confirm {
                        FileConfirm::Pull(path) => Action::PullFile(path),
                        _ => unreachable!(),
                    }
                }
                KeyCode::Char('o') if matches!(self.files_state.confirming, Some(FileConfirm::Open(_))) => {
                    let confirm = self.files_state.confirming.take().unwrap();
                    match confirm {
                        FileConfirm::Open(path) => Action::OpenFile(path),
                        _ => unreachable!(),
                    }
                }
                _ => {
                    self.files_state.confirming = None;
                    Action::Noop
                }
            };
        }

        if self.focused == Some(panel::FILES) {
            match code {
                KeyCode::Up => {
                    self.files_state.move_up();
                    return Action::Noop;
                }
                KeyCode::Down => {
                    let count = self.files_state.flatten_visible().len();
                    self.files_state.move_down(count);
                    return Action::Noop;
                }
                KeyCode::Enter | KeyCode::Right => {
                    if let Some(result) = self.files_state.toggle_selected() {
                        return match result {
                            ToggleResult::Expand(path) => Action::ExpandDir(path),
                            ToggleResult::ExpandCached | ToggleResult::Collapse => Action::Noop,
                        };
                    }
                    return Action::Noop;
                }
                KeyCode::Left => {
                    self.files_state.collapse_selected();
                    return Action::Noop;
                }
                KeyCode::Char('p') => {
                    if !self.files_state.selected_is_dir() {
                        if let Some(path) = self.files_state.selected_path() {
                            self.files_state.confirming = Some(FileConfirm::Pull(path));
                        }
                    }
                    return Action::Noop;
                }
                KeyCode::Char('o') => {
                    if !self.files_state.selected_is_dir() {
                        if let Some(path) = self.files_state.selected_path() {
                            self.files_state.confirming = Some(FileConfirm::Open(path));
                        }
                    }
                    return Action::Noop;
                }
                KeyCode::Char('r') => {
                    self.files_state.error = None;
                    self.files_state.root_children = None;
                    self.files_state.selected_index = 0;
                    return Action::RefreshFiles;
                }
                KeyCode::Esc => {
                    self.focused = None;
                    return Action::Noop;
                }
                _ => {}
            }
        }

        if self.focused == Some(panel::PERMISSIONS) {
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.permissions_state.move_up();
                    return Action::Noop;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.permissions_state.move_down();
                    return Action::Noop;
                }
                KeyCode::Enter => {
                    if let Some((perm, granted)) = self.permissions_state.toggle_selected() {
                        return Action::TogglePermission(perm, granted);
                    }
                    return Action::Noop;
                }
                KeyCode::Esc => {
                    self.focused = None;
                    return Action::Noop;
                }
                _ => {}
            }
        }

        if self.focused == Some(panel::LOGCAT) {
            if let Some(action) = self.logcat_state.handle_key(code) {
                if matches!(action, Action::Unfocus) {
                    self.focused = None;
                    return Action::Noop;
                }
                return action;
            }
        }

        if self.focused == Some(panel::TRACE) {
            match code {
                KeyCode::Char('s') => {
                    if self.trace_state.recording {
                        self.trace_state.recording = false;
                        self.trace_state.started_at = None;
                        return Action::StopTrace;
                    } else {
                        self.trace_state.recording = true;
                        self.trace_state.started_at = Some(std::time::Instant::now());
                        self.trace_state.status_message = None;
                        self.trace_state.message_at = None;
                        return Action::StartTrace;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if !self.trace_state.recording => {
                    if self.trace_state.selected_index > 0 {
                        self.trace_state.selected_index -= 1;
                    }
                    return Action::Noop;
                }
                KeyCode::Down | KeyCode::Char('j') if !self.trace_state.recording => {
                    let max = self.trace_state.pulled_traces.len().saturating_sub(1);
                    if self.trace_state.selected_index < max {
                        self.trace_state.selected_index += 1;
                    }
                    return Action::Noop;
                }
                KeyCode::Enter if !self.trace_state.recording => {
                    if let Some(path) = self.trace_state.selected_path() {
                        trace::open_in_perfetto_ui(path);
                    }
                    return Action::Noop;
                }
                KeyCode::Char('d') if !self.trace_state.recording => {
                    if let Some(path) = self.trace_state.delete_selected() {
                        let _ = std::fs::remove_file(&path);
                    }
                    return Action::Noop;
                }
                KeyCode::Esc => {
                    self.focused = None;
                    return Action::Noop;
                }
                _ => {}
            }
        }

        match code {
            KeyCode::F(1) => {
                self.toolbar.open_devices();
                Action::FetchDevices
            }
            KeyCode::F(2) => {
                self.toolbar.open_apps();
                Action::FetchApps
            }
            KeyCode::Char('o') => Action::OpenApp,
            KeyCode::Char('k') => Action::KillApp,
            KeyCode::Char('q') => {
                self.confirming_quit = true;
                Action::Noop
            }
            KeyCode::Char('0') => {
                self.commands_visible = !self.commands_visible;
                Action::Noop
            }
            KeyCode::Char(c @ '1'..='8') => {
                self.toggle_visibility(c as u8 - b'0');
                Action::Noop
            }
            KeyCode::Char(c) => {
                if let Some(panel) = panel::by_focus_key(c) {
                    self.toggle_focus(panel.number);
                }
                Action::Noop
            }
            _ => Action::Noop,
        }
    }

    fn toggle_visibility(&mut self, n: u8) {
        if !(1..=8).contains(&n) {
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
        self.logcat_state.editing = None;
        self.commands_filter.clear();
        self.commands_cursor = 0;
    }

    pub fn panel_visibility(&self) -> &[bool; 8] {
        &self.visible
    }

    pub fn monitor_state(&self) -> &MonitorState {
        &self.monitor_state
    }

    pub fn monitor_state_mut(&mut self) -> &mut MonitorState {
        &mut self.monitor_state
    }

    pub fn trace_state(&self) -> &TraceState {
        &self.trace_state
    }

    pub fn trace_state_mut(&mut self) -> &mut TraceState {
        &mut self.trace_state
    }

    #[cfg(test)]
    pub fn db_state(&self) -> &DatabaseState {
        &self.db_state
    }

    pub fn db_state_mut(&mut self) -> &mut DatabaseState {
        &mut self.db_state
    }

    pub fn permissions_state(&self) -> &PermissionsState {
        &self.permissions_state
    }

    pub fn permissions_state_mut(&mut self) -> &mut PermissionsState {
        &mut self.permissions_state
    }

    pub fn files_state(&self) -> &FilesState {
        &self.files_state
    }

    pub fn files_state_mut(&mut self) -> &mut FilesState {
        &mut self.files_state
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

    pub fn layout_bounds(&self) -> bool {
        self.layout_bounds
    }

    pub fn set_layout_bounds(&mut self, v: bool) {
        self.layout_bounds = v;
    }

    pub fn airplane_mode(&self) -> bool {
        self.airplane_mode
    }

    pub fn set_airplane_mode(&mut self, v: bool) {
        self.airplane_mode = v;
    }

    pub fn wifi_enabled(&self) -> bool {
        self.wifi_enabled
    }

    pub fn set_wifi_enabled(&mut self, v: bool) {
        self.wifi_enabled = v;
    }

    pub fn confirming_quit(&self) -> bool {
        self.confirming_quit
    }

    pub fn commands_cursor(&self) -> usize {
        self.commands_cursor
    }

    pub fn commands_filter(&self) -> &str {
        &self.commands_filter
    }

    pub fn filtered_commands(&self) -> Vec<(&'static str, fn() -> Action)> {
        COMMAND_LIST
            .iter()
            .filter(|(name, _)| apps::fuzzy_matches(name, &self.commands_filter))
            .copied()
            .collect()
    }

    pub fn commands_visible(&self) -> bool {
        self.commands_visible
    }

    pub fn reset_for_new_app(&mut self, package: &str) {
        self.package = package.to_string();
        self.db_state = DatabaseState::new();
        self.files_state = FilesState::new(package);
        self.permissions_state = PermissionsState::new();
    }

    pub fn toolbar(&self) -> &ToolbarState {
        &self.toolbar
    }

    pub fn toolbar_mut(&mut self) -> &mut ToolbarState {
        &mut self.toolbar
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logcat_state;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn new_app_all_panels_visible() {
        let app = App::new(None, Some("com.test"));
        assert_eq!(app.panel_visibility(), &[true; 8]);
    }

    #[test]
    fn new_app_has_no_focus() {
        let app = App::new(None, Some("com.test"));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn toggle_visibility_hides_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.toggle_visibility(3);
        assert_eq!(
            app.panel_visibility(),
            &[true, true, false, true, true, true, true, true]
        );
    }

    #[test]
    fn toggle_visibility_twice_restores_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.toggle_visibility(3);
        app.toggle_visibility(3);
        assert_eq!(app.panel_visibility(), &[true; 8]);
    }

    #[test]
    fn cannot_hide_last_panel() {
        let mut app = App::new(None, Some("com.test"));
        for n in 2..=8 {
            app.toggle_visibility(n);
        }
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false, false, false]
        );
        app.toggle_visibility(1);
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false, false, false]
        );
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new(None, Some("com.test"));
        app.toggle_visibility(0);
        app.toggle_visibility(9);
        app.toggle_visibility(10);
        assert_eq!(app.panel_visibility(), &[true; 8]);
    }

    #[test]
    fn qq_quits() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(key(KeyCode::Char('q'))), Action::Noop));
        assert!(app.confirming_quit());
        assert!(matches!(app.handle_key(key(KeyCode::Char('q'))), Action::Quit));
    }

    #[test]
    fn q_then_other_cancels_quit() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.confirming_quit());
        assert!(matches!(app.handle_key(key(KeyCode::Esc)), Action::Noop));
        assert!(!app.confirming_quit());
    }

    #[test]
    fn handle_key_digit_toggles_visibility() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('3'))),
            Action::Noop
        ));
        assert_eq!(
            app.panel_visibility(),
            &[true, true, false, true, true, true, true, true]
        );
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('x'))),
            Action::Noop
        ));
        assert_eq!(app.panel_visibility(), &[true; 8]);
    }

    #[test]
    fn focus_toggles_on_same_key() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn o_opens_app() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(key(KeyCode::Char('o'))), Action::OpenApp));
    }

    #[test]
    fn k_kills_app() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(key(KeyCode::Char('k'))), Action::KillApp));
    }

    #[test]
    fn c_focuses_commands_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.focused_panel(), Some(panel::COMMANDS));
    }

    #[test]
    fn commands_panel_enter_executes() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('c')));
        let action = app.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Action::OpenApp));
    }

    #[test]
    fn t_enters_tag_editing_when_focused() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.logcat_state().editing, Some(logcat_state::LogcatEditTarget::Tag));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
    }

    #[test]
    fn s_enters_search_editing_when_focused() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.logcat_state().editing, Some(logcat_state::LogcatEditTarget::Search));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
    }

    #[test]
    fn t_focuses_trace_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.focused_panel(), Some(2));
    }

    #[test]
    fn s_starts_trace_when_focused() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('t')));
        assert!(!app.trace_state().recording);
        let action = app.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(action, Action::StartTrace));
        assert!(app.trace_state().recording);
    }

    #[test]
    fn s_stops_trace_when_recording() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.trace_state().recording);
        let action = app.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(action, Action::StopTrace));
        assert!(!app.trace_state().recording);
    }

    #[test]
    fn esc_unfocuses_trace_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.focused_panel(), Some(2));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn typing_appends_to_tag() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.logcat_state().filter.tag, "ab");
    }

    #[test]
    fn typing_appends_to_search() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.logcat_state().filter.search, "xy");
    }

    #[test]
    fn backspace_removes_from_tag() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.logcat_state().filter.tag, "a");
    }

    #[test]
    fn enter_exits_editing_mode() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.logcat_state().editing, Some(logcat_state::LogcatEditTarget::Search));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.logcat_state().editing, None);
    }

    #[test]
    fn esc_exits_tag_editing_preserving_input() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.input_mode(), InputMode::Normal);
        assert_eq!(app.logcat_state().filter.tag, "ab");
    }

    #[test]
    fn esc_exits_search_editing_preserving_input() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.input_mode(), InputMode::Normal);
        assert_eq!(app.logcat_state().filter.search, "xy");
    }

    #[test]
    fn level_cycles_forward_with_right() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.logcat_state().filter.level, None);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.logcat_state().filter.level, Some('V'));
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.logcat_state().filter.level, Some('D'));
    }

    #[test]
    fn level_cycles_backward_with_left() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.logcat_state().filter.level, Some('F'));
    }

    #[test]
    fn level_wraps_around() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        for _ in 0..7 {
            app.handle_key(key(KeyCode::Right));
        }
        assert_eq!(app.logcat_state().filter.level, None);
    }

    #[test]
    fn scroll_up_increments_offset() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.logcat_state().scroll, 1);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.logcat_state().scroll, 2);
    }

    #[test]
    fn scroll_down_decrements_offset() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.logcat_state().scroll, 1);
    }

    #[test]
    fn j_k_scroll_logcat() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.logcat_state().scroll, 1);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.logcat_state().scroll, 2);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.logcat_state().scroll, 1);
    }

    #[test]
    fn space_scrolls_page() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.logcat_state().scroll, 20);
    }

    #[test]
    fn scroll_does_not_go_below_zero() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.logcat_state().scroll, 0);
    }

    #[test]
    fn esc_resets_scroll_to_zero() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.logcat_state().scroll, 3);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.logcat_state().scroll, 0);
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
    }

    #[test]
    fn esc_unfocuses_logcat_when_at_bottom() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn left_right_ignored_without_focus() {
        let mut app = App::new(None, Some("com.test"));
        assert_eq!(app.focused_panel(), None);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.logcat_state().filter.level, None);
    }

    #[test]
    fn r_resets_logcat_filters_and_scroll() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.logcat_state().filter.tag, "a");
        assert_eq!(app.logcat_state().filter.search, "b");
        assert!(app.logcat_state().filter.level.is_some());
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.logcat_state().scroll, 2);

        assert!(matches!(app.handle_key(key(KeyCode::Char('r'))), Action::ResetLogcat));
        assert_eq!(app.logcat_state().filter.tag, "");
        assert_eq!(app.logcat_state().filter.search, "");
        assert_eq!(app.logcat_state().filter.level, None);
        assert_eq!(app.logcat_state().scroll, 0);
    }

    #[test]
    fn c_in_logcat_returns_copy_action() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        let action = app.handle_key(key(KeyCode::Char('c')));
        assert!(matches!(action, Action::CopyLogcat));
        assert!(app.logcat_state().copied_at.is_some());
    }

    #[test]
    fn r_ignored_without_focus() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(key(KeyCode::Char('r'))), Action::Noop));
    }

    #[test]
    fn scroll_ignored_when_logcat_not_focused() {
        let mut app = App::new(None, Some("com.test"));
        assert_eq!(app.focused_panel(), None);
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.logcat_state().scroll, 0);
    }

    #[test]
    fn d_focuses_database_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.focused_panel(), Some(panel::DATABASE));
    }

    #[test]
    fn db_panel_navigate_and_select() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into(), "b.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.db_state().selected_index, 1);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.db_state().selected_db.as_deref(), Some("b.db"));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
    }

    #[test]
    fn db_panel_esc_deselects_then_unfocuses() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.db_state().selected_db.is_some());
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.input_mode(), InputMode::Normal);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.db_state().selected_db.is_none());
        assert_eq!(app.focused_panel(), Some(panel::DATABASE));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn db_panel_select_auto_enters_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
    }

    #[test]
    fn db_panel_e_enters_query_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.input_mode(), InputMode::Normal);
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
    }

    #[test]
    fn query_editing_enter_returns_run_query_and_appends_history() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["test.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('S')));
        app.handle_key(key(KeyCode::Char('Q')));
        app.handle_key(key(KeyCode::Char('L')));
        let action = app.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Action::RunQuery(_, _)));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        assert!(app.db_state().textarea_text().is_empty());
        assert_eq!(app.db_state().history.len(), 1);
    }

    #[test]
    fn e_ignored_without_db_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.db_state_mut().selected_db = Some("a.db".into());
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn e_enters_editing_when_focused_with_selected_db() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc)); // exit EditingQuery
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        assert_eq!(app.focused_panel(), Some(panel::DATABASE));
    }

    #[test]
    fn up_down_with_selected_db_scrolls_history_when_focused() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc)); // exit EditingQuery
        app.db_state_mut().history = vec![
            crate::database::ReplLine::Input("SELECT 1".into()),
            crate::database::ReplLine::Output("1".into()),
        ];
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.db_state().scroll, 1);
    }

    #[test]
    fn up_down_ignored_without_db_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().selected_db = Some("a.db".into());
        app.db_state_mut().history = vec![
            crate::database::ReplLine::Input("SELECT 1".into()),
        ];
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.db_state().scroll, 0);
    }

    #[test]
    fn toggle_panel_5_visibility() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('5')));
        assert!(!app.panel_visibility()[4]);
        app.handle_key(key(KeyCode::Char('5')));
        assert!(app.panel_visibility()[4]);
    }

    #[test]
    fn p_sets_confirming_pull_from_list_view() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into(), "b.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Down));
        let action = app.handle_key(key(KeyCode::Char('p')));
        assert!(matches!(action, Action::Noop));
        assert_eq!(app.db_state().confirming_pull.as_deref(), Some("b.db"));
    }

    #[test]
    fn pp_confirms_pull() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('p')));
        let action = app.handle_key(key(KeyCode::Char('p')));
        assert!(matches!(action, Action::PullDb(db) if db == "a.db"));
        assert!(app.db_state().confirming_pull.is_none());
    }

    #[test]
    fn any_key_cancels_pull_confirmation() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('p')));
        assert!(app.db_state().confirming_pull.is_some());
        let action = app.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Action::Noop));
        assert!(app.db_state().confirming_pull.is_none());
    }

    #[test]
    fn p_sets_confirming_pull_from_repl_view() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        let action = app.handle_key(key(KeyCode::Char('p')));
        assert!(matches!(action, Action::Noop));
        assert_eq!(app.db_state().confirming_pull.as_deref(), Some("a.db"));
    }

    #[test]
    fn toggle_focus_clears_tag_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.logcat_state().editing, Some(logcat_state::LogcatEditTarget::Tag));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.logcat_state().editing, None);
    }

    #[test]
    fn toggle_focus_clears_search_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.logcat_state().editing, Some(logcat_state::LogcatEditTarget::Search));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.logcat_state().editing, None);
    }

    #[test]
    fn switching_focus_clears_query_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.input_mode(), InputMode::EditingQuery);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn r_resets_db_from_list_view() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        assert!(matches!(app.handle_key(key(KeyCode::Char('r'))), Action::ResetDb));
        assert!(app.db_state().databases.is_empty());
    }

    #[test]
    fn r_resets_db_from_repl_view() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(app.db_state().selected_db.is_some());
        assert!(matches!(app.handle_key(key(KeyCode::Char('r'))), Action::ResetDb));
        assert!(app.db_state().selected_db.is_none());
        assert!(app.db_state().databases.is_empty());
    }

    #[test]
    fn c_copies_full_history() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        app.db_state_mut().push_query("SELECT 1");
        app.db_state_mut().push_result("1");
        let action = app.handle_key(key(KeyCode::Char('c')));
        assert!(matches!(action, Action::CopyDbResult(ref s) if s == "> SELECT 1\n1"));
    }

    #[test]
    fn c_noop_without_output() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(matches!(app.handle_key(key(KeyCode::Char('c'))), Action::Noop));
    }


}
