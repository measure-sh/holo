use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Device;
use crate::commands::CommandsState;
use crate::database::DatabaseState;
use crate::files::FilesState;
use crate::logcat_state::LogcatState;
use crate::monitor::MonitorState;
use crate::panel;
use crate::permissions::PermissionsState;
use crate::toolbar::{ToolbarAction, ToolbarState};
use crate::trace::TraceState;

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

pub struct App {
    visible: [bool; 8],
    focused: Option<u8>,
    commands: CommandsState,
    logcat_state: LogcatState,
    db_state: DatabaseState,
    permissions_state: PermissionsState,
    files_state: FilesState,
    monitor_state: MonitorState,
    toolbar: ToolbarState,
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
            visible: [true; 8],
            focused: None,
            commands: CommandsState::new(),
            logcat_state: LogcatState::new(),
            db_state: DatabaseState::new(),
            permissions_state: PermissionsState::new(),
            files_state: FilesState::new(pkg),
            monitor_state: MonitorState::new(),
            toolbar,
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

        if self.db_state.editing_query {
            return self.db_state.handle_key(key).unwrap_or(Action::Noop);
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
            if let Some(action) = self.commands.handle_key(code) {
                if matches!(action, Action::Unfocus) {
                    self.focused = None;
                    return Action::Noop;
                }
                return action;
            }
        }

        if self.db_state.confirming_pull.is_some() || self.focused == Some(panel::DATABASE) {
            if let Some(action) = self.db_state.handle_key(key) {
                if matches!(action, Action::Unfocus) {
                    self.focused = None;
                    return Action::Noop;
                }
                return action;
            }
        }

        if self.files_state.confirming.is_some() || self.focused == Some(panel::FILES) {
            if let Some(action) = self.files_state.handle_key(code) {
                if matches!(action, Action::Unfocus) {
                    self.focused = None;
                    return Action::Noop;
                }
                return action;
            }
        }

        if self.focused == Some(panel::PERMISSIONS) {
            if let Some(action) = self.permissions_state.handle_key(code) {
                if matches!(action, Action::Unfocus) {
                    self.focused = None;
                    return Action::Noop;
                }
                return action;
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
            if let Some(action) = self.trace_state.handle_key(code) {
                if matches!(action, Action::Unfocus) {
                    self.focused = None;
                    return Action::Noop;
                }
                return action;
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
                self.commands.visible = !self.commands.visible;
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
        self.logcat_state.editing = None;
        self.db_state.editing_query = false;
        self.commands.filter.clear();
        self.commands.cursor = 0;
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

    pub fn commands(&self) -> &CommandsState {
        &self.commands
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
        assert!(!app.db_state().editing_query);
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
        assert!(!app.db_state().editing_query);
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
        assert!(app.db_state().editing_query);
    }

    #[test]
    fn db_panel_esc_deselects_then_unfocuses() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.db_state().selected_db.is_some());
        assert!(app.db_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.db_state().editing_query);
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
        assert!(app.db_state().editing_query);
    }

    #[test]
    fn db_panel_e_enters_query_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.db_state().editing_query);
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.db_state().editing_query);
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
        assert!(app.db_state().editing_query);
        assert!(app.db_state().textarea_text().is_empty());
        assert_eq!(app.db_state().history.len(), 1);
    }

    #[test]
    fn e_ignored_without_db_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.db_state_mut().selected_db = Some("a.db".into());
        app.handle_key(key(KeyCode::Char('e')));
        assert!(!app.db_state().editing_query);
    }

    #[test]
    fn e_enters_editing_when_focused_with_selected_db() {
        let mut app = App::new(None, Some("com.test"));
        app.db_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc)); // exit EditingQuery
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.db_state().editing_query);
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
        assert!(app.db_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.db_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('d')));
        assert!(!app.db_state().editing_query);
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
