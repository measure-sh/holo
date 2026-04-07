use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::adb::Device;
use crate::commands::CommandsState;
use crate::issues::IssuesState;
use crate::database::DatabaseState;
use crate::files::FilesState;
use crate::logcat::LogcatState;
use crate::monitor::MonitorState;
use crate::panel;
use crate::theme;
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
    OpenLogcat,
    RefreshFiles,
    ExpandDir(String),
    OpenFile(String),
    StartTrace,
    StopTrace,
    Screenshot,
    ToggleWifi,
    ToggleDarkMode,
    WirelessAdb,
    LaunchEmulator(String),
    MirrorDevice,
    ToggleShowTaps,
    TogglePointerLocation,
    ToggleGpuRendering,
    OpenInEditor(String),
}

pub struct App {
    panel_visible: [bool; 9],
    focused: Option<u8>,
    commands: CommandsState,
    logcat_state: LogcatState,
    database_state: DatabaseState,
    permissions_state: PermissionsState,
    files_state: FilesState,
    monitor_state: MonitorState,
    toolbar: ToolbarState,
    layout_bounds: bool,
    airplane_mode: bool,
    wifi_enabled: bool,
    dark_mode: bool,
    show_taps: bool,
    pointer_location: bool,
    gpu_rendering: bool,
    confirming_quit: bool,
    confirming_settings: bool,
    pub settings_open: bool,
    pub settings_cursor: usize,
    trace_state: TraceState,
    issues_state: IssuesState,
    pub dialog: Option<String>,
    saved_visibility: Option<([bool; 9], bool)>,
    status_flash: Option<(String, std::time::Instant, bool)>,
}

impl App {
    pub fn new(device: Option<Device>, package: Option<&str>) -> Self {
        let pkg = package.unwrap_or_default();
        let mut toolbar = ToolbarState::new(device);
        toolbar.package = package.map(String::from);
        Self {
            panel_visible: [true; 9],
            focused: None,
            commands: CommandsState::new(),
            logcat_state: LogcatState::new(),
            database_state: DatabaseState::new(),
            permissions_state: PermissionsState::new(),
            files_state: FilesState::new(pkg),
            monitor_state: MonitorState::new(),
            toolbar,
            layout_bounds: false,
            airplane_mode: false,
            wifi_enabled: false,
            dark_mode: false,
            show_taps: false,
            pointer_location: false,
            gpu_rendering: false,
            confirming_quit: false,
            confirming_settings: false,
            settings_open: false,
            settings_cursor: 0,
            trace_state: TraceState::new(pkg),
            issues_state: IssuesState::new(),
            dialog: None,
            saved_visibility: None,
            status_flash: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        let code = key.code;
        if self.dialog.is_some() {
            self.dialog = None;
            return Action::Noop;
        }
        if self.settings_open {
            const SELECTABLE_COUNT: usize = 4;
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.settings_cursor = self.settings_cursor.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.settings_cursor = (self.settings_cursor + 1).min(SELECTABLE_COUNT - 1);
                }
                KeyCode::Right | KeyCode::Char('l') if self.settings_cursor == 0 => {
                    let next = (theme::current_index() + 1) % theme::theme_count();
                    theme::set_theme(next);
                }
                KeyCode::Left | KeyCode::Char('h') if self.settings_cursor == 0 => {
                    let cur = theme::current_index();
                    let prev = if cur == 0 { theme::theme_count() - 1 } else { cur - 1 };
                    theme::set_theme(prev);
                }
                KeyCode::Char('c') if self.settings_cursor == 1 => {
                    let dir = std::env::temp_dir().join("holo");
                    let _ = std::fs::create_dir_all(&dir);
                    crate::clipboard::copy_to_clipboard(&dir.to_string_lossy());
                    self.settings_open = false;
                    self.set_status_flash("Path copied!".into(), false);
                }
                KeyCode::Enter => {
                    match self.settings_cursor {
                        1 => {
                            let dir = std::env::temp_dir().join("holo");
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = open::that(&dir);
                            self.settings_open = false;
                        }
                        2 => {
                            let _ = open::that("https://github.com/Genymobile/scrcpy/tree/master?tab=readme-ov-file");
                            self.settings_open = false;
                        }
                        3 => {
                            let _ = open::that("https://github.com/measure-sh/holo");
                            self.settings_open = false;
                        }
                        _ => {
                            self.settings_open = false;
                        }
                    }
                }
                _ => {
                    self.settings_open = false;
                }
            }
            return Action::Noop;
        }
        if self.commands.editing {
            return self.commands.handle_key(key).unwrap_or(Action::Noop);
        }
        if self.logcat_state.editing.is_some() {
            return self.logcat_state.handle_key(key).unwrap_or(Action::Noop);
        }

        if self.database_state.editing_query {
            return self.database_state.handle_key(key).unwrap_or(Action::Noop);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match code {
                KeyCode::Char('d') => {
                    self.toolbar.open_devices();
                    return Action::FetchDevices;
                }
                KeyCode::Char('a') => {
                    self.toolbar.open_apps();
                    return Action::FetchApps;
                }
                _ => {}
            }
        }

        if self.toolbar.open.is_some() {
            return match self.toolbar.handle_key(code) {
                ToolbarAction::SelectDevice(d) => Action::ChangeDevice(d),
                ToolbarAction::SelectApp(p) => Action::ChangeApp(p),
                ToolbarAction::LaunchEmulator(name) => Action::LaunchEmulator(name),
                ToolbarAction::Close | ToolbarAction::None => Action::Noop,
            };
        }

        if self.focused.is_some() && code == KeyCode::Char('z') {
            self.toggle_zoom();
            return Action::Noop;
        }

        if self.confirming_quit {
            self.confirming_quit = false;
            return match code {
                KeyCode::Char('q') => Action::Quit,
                _ => Action::Noop,
            };
        }

        if self.confirming_settings {
            self.confirming_settings = false;
            if code == KeyCode::Char('s') {
                self.settings_open = true;
            }
            return Action::Noop;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && let KeyCode::Char(ch) = code
            && let Some(action) = self.commands.action_for_shortcut(ch)
        {
            return action;
        }

        if let Some(action) = self.delegate_to_focused(panel::COMMANDS, key) {
            return action;
        }
        if self.database_state.confirming_pull.is_some() || self.focused == Some(panel::DATABASE) {
            let result = self.database_state.handle_key(key);
            if let Some(action) = self.dispatch_panel_key(result) {
                return action;
            }
        }
        if let Some(action) = self.delegate_to_focused(panel::FILES, key) {
            return action;
        }
        if let Some(action) = self.delegate_to_focused(panel::PERMISSIONS, key) {
            return action;
        }
        if let Some(action) = self.delegate_to_focused(panel::LOGCAT, key) {
            return action;
        }
        if let Some(action) = self.delegate_to_focused(panel::TRACE, key) {
            return action;
        }
        if let Some(action) = self.delegate_to_focused(panel::ISSUES, key) {
            return action;
        }

        match code {
            KeyCode::Char('q') => {
                self.confirming_quit = true;
                Action::Noop
            }
            KeyCode::Char('s') => {
                self.confirming_settings = true;
                Action::Noop
            }
            KeyCode::Char('0') => {
                self.saved_visibility = None;
                self.commands.visible = !self.commands.visible;
                if !self.commands.visible && self.focused == Some(panel::COMMANDS) {
                    self.focused = None;
                }
                Action::Noop
            }
            KeyCode::Char(c @ '1'..='9') => {
                self.saved_visibility = None;
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

    fn delegate_to_focused(&mut self, panel_id: u8, key: KeyEvent) -> Option<Action> {
        if self.focused != Some(panel_id) {
            return None;
        }
        let result = match panel_id {
            panel::COMMANDS => self.commands.handle_key(key),
            panel::LOGCAT => self.logcat_state.handle_key(key),
            panel::FILES => self.files_state.handle_key(key),
            panel::PERMISSIONS => self.permissions_state.handle_key(key),
            panel::TRACE => self.trace_state.handle_key(key),
            panel::ISSUES => self.issues_state.handle_key(key),
            _ => None,
        };
        if let Some(action) = self.dispatch_panel_key(result) {
            return Some(action);
        }
        match key.code {
            KeyCode::Char(c) if panel::by_focus_key(c).is_some_and(|p| p.number == panel_id) => {
                self.restore_zoom();
                self.focused = None;
                Some(Action::Noop)
            }
            KeyCode::Char('0'..='9') => None,
            _ => Some(Action::Noop),
        }
    }

    fn dispatch_panel_key(&mut self, result: Option<Action>) -> Option<Action> {
        let action = result?;
        if matches!(action, Action::Unfocus) {
            self.restore_zoom();
            self.focused = None;
            return Some(Action::Noop);
        }
        Some(action)
    }

    fn is_panel_visible(&self, n: u8) -> bool {
        if n == panel::COMMANDS {
            return self.commands.visible;
        }
        if !(1..=9).contains(&n) {
            return false;
        }
        self.panel_visible[(n - 1) as usize]
    }

    fn toggle_visibility(&mut self, n: u8) {
        if !(1..=9).contains(&n) {
            return;
        }
        let idx = (n - 1) as usize;
        let visible_count = self.panel_visible.iter().filter(|&&v| v).count() + self.commands.visible as usize;
        if self.panel_visible[idx] && visible_count == 1 {
            return;
        }
        self.panel_visible[idx] = !self.panel_visible[idx];
        if !self.panel_visible[idx] && self.focused == Some(n) {
            self.focused = None;
        }
    }

    fn toggle_zoom(&mut self) {
        if self.saved_visibility.is_some() {
            self.restore_zoom();
        } else if let Some(focused) = self.focused {
            self.saved_visibility = Some((self.panel_visible, self.commands.visible));
            if focused == panel::COMMANDS {
                self.panel_visible = [false; 9];
                self.commands.visible = true;
            } else {
                self.panel_visible = [false; 9];
                self.commands.visible = false;
                let idx = (focused - 1) as usize;
                self.panel_visible[idx] = true;
            }
        }
    }

    fn restore_zoom(&mut self) {
        if let Some((vis, cmd_vis)) = self.saved_visibility.take() {
            self.panel_visible = vis;
            self.commands.visible = cmd_vis;
        }
    }

    pub fn is_zoomed(&self) -> bool {
        self.saved_visibility.is_some()
    }

    fn toggle_focus(&mut self, n: u8) {
        if self.focused == Some(n) {
            self.focused = None;
        } else if self.is_panel_visible(n) {
            self.focused = Some(n);
        } else {
            return;
        }
        self.logcat_state.editing = None;
        self.database_state.editing_query = false;
        self.commands.filter.clear();
        self.commands.cursor = 0;
    }

    pub fn panel_visibility(&self) -> &[bool; 9] {
        &self.panel_visible
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
    pub fn database_state(&self) -> &DatabaseState {
        &self.database_state
    }

    pub fn database_state_mut(&mut self) -> &mut DatabaseState {
        &mut self.database_state
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

    pub fn issues_state(&self) -> &IssuesState {
        &self.issues_state
    }

    pub fn issues_state_mut(&mut self) -> &mut IssuesState {
        &mut self.issues_state
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

    pub fn dark_mode(&self) -> bool {
        self.dark_mode
    }

    pub fn set_dark_mode(&mut self, v: bool) {
        self.dark_mode = v;
    }

    pub fn show_taps(&self) -> bool {
        self.show_taps
    }

    pub fn set_show_taps(&mut self, v: bool) {
        self.show_taps = v;
    }

    pub fn pointer_location(&self) -> bool {
        self.pointer_location
    }

    pub fn set_pointer_location(&mut self, v: bool) {
        self.pointer_location = v;
    }

    pub fn gpu_rendering(&self) -> bool {
        self.gpu_rendering
    }

    pub fn set_gpu_rendering(&mut self, v: bool) {
        self.gpu_rendering = v;
    }

    pub fn confirming_quit(&self) -> bool {
        self.confirming_quit
    }

    pub fn confirming_settings(&self) -> bool {
        self.confirming_settings
    }

    pub fn commands(&self) -> &CommandsState {
        &self.commands
    }

    pub fn commands_mut(&mut self) -> &mut CommandsState {
        &mut self.commands
    }

    pub fn reset_for_new_app(&mut self, package: &str) {
        self.logcat_state = LogcatState::new();
        self.database_state = DatabaseState::new();
        self.files_state = FilesState::new(package);
        self.permissions_state = PermissionsState::new();
        self.monitor_state = MonitorState::new();
        self.trace_state = TraceState::new(package);
        self.issues_state = IssuesState::new();
        self.focused = None;
    }

    pub fn toolbar(&self) -> &ToolbarState {
        &self.toolbar
    }

    pub fn toolbar_mut(&mut self) -> &mut ToolbarState {
        &mut self.toolbar
    }

    pub fn set_status_flash(&mut self, msg: String, is_error: bool) {
        self.status_flash = Some((msg, std::time::Instant::now(), is_error));
    }

    pub fn status_flash_active(&self) -> Option<(&str, bool)> {
        let (msg, t, is_error) = self.status_flash.as_ref()?;
        if t.elapsed() < std::time::Duration::from_secs(3) {
            Some((msg.as_str(), *is_error))
        } else {
            None
        }
    }

    pub fn has_active_animation(&self) -> bool {
        self.commands.has_active_animation()
            || self.status_flash_active().is_some()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logcat;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn new_app_all_panels_visible() {
        let app = App::new(None, Some("com.test"));
        assert_eq!(app.panel_visibility(), &[true; 9]);
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
            &[true, true, false, true, true, true, true, true, true]
        );
    }

    #[test]
    fn toggle_visibility_twice_restores_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.toggle_visibility(3);
        app.toggle_visibility(3);
        assert_eq!(app.panel_visibility(), &[true; 9]);
    }

    #[test]
    fn cannot_hide_last_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.commands.visible = false;
        for n in 2..=9 {
            app.toggle_visibility(n);
        }
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false, false, false, false]
        );
        app.toggle_visibility(1);
        assert_eq!(
            app.panel_visibility(),
            &[true, false, false, false, false, false, false, false, false]
        );
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut app = App::new(None, Some("com.test"));
        app.toggle_visibility(0);
        app.toggle_visibility(10);
        app.toggle_visibility(11);
        assert_eq!(app.panel_visibility(), &[true; 9]);
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
            &[true, true, false, true, true, true, true, true, true]
        );
    }

    #[test]
    fn handle_key_unknown_is_none() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(
            app.handle_key(key(KeyCode::Tab)),
            Action::Noop
        ));
        assert_eq!(app.panel_visibility(), &[true; 9]);
    }

    #[test]
    fn i_focuses_issues_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('i')));
        assert_eq!(app.focused_panel(), Some(panel::ISSUES));
    }

    #[test]
    fn focus_toggles_on_same_key() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), None);
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn ctrl_o_opens_app() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('o')), Action::OpenApp));
    }

    #[test]
    fn ctrl_k_kills_app() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('k')), Action::KillApp));
    }

    #[test]
    fn ctrl_d_opens_device_selector() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('d')), Action::FetchDevices));
    }

    #[test]
    fn ctrl_a_opens_app_selector() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('a')), Action::FetchApps));
    }

    #[test]
    fn ctrl_n_toggles_dark_mode() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('n')), Action::ToggleDarkMode));
    }

    #[test]
    fn ctrl_e_toggles_airplane_mode() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('e')), Action::ToggleAirplaneMode));
    }

    #[test]
    fn ctrl_t_toggles_show_taps() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('t')), Action::ToggleShowTaps));
    }

    #[test]
    fn ctrl_p_toggles_pointer_location() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('p')), Action::TogglePointerLocation));
    }

    #[test]
    fn ctrl_g_toggles_gpu_rendering() {
        let mut app = App::new(None, Some("com.test"));
        assert!(matches!(app.handle_key(ctrl('g')), Action::ToggleGpuRendering));
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
        assert_eq!(app.logcat_state().editing, Some(logcat::LogcatEditTarget::Tag));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
    }

    #[test]
    fn slash_enters_search_editing_when_focused() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.logcat_state().editing, Some(logcat::LogcatEditTarget::Search));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
    }

    #[test]
    fn t_focuses_trace_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.focused_panel(), Some(panel::TRACE));
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
        assert_eq!(app.focused_panel(), Some(panel::TRACE));
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
        app.handle_key(key(KeyCode::Char('/')));
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
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.logcat_state().editing, Some(logcat::LogcatEditTarget::Search));
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
        assert!(!app.database_state().editing_query);
        assert_eq!(app.logcat_state().filter.tag, "ab");
    }

    #[test]
    fn esc_exits_search_editing_preserving_input() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.database_state().editing_query);
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
        app.handle_key(key(KeyCode::Char('/')));
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
    fn o_in_logcat_returns_open_action() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        let action = app.handle_key(key(KeyCode::Char('o')));
        assert!(matches!(action, Action::OpenLogcat));
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
        app.database_state_mut().databases = vec!["a.db".into(), "b.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.database_state().selected_index, 1);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.database_state().selected_db.as_deref(), Some("b.db"));
        assert!(app.database_state().editing_query);
    }

    #[test]
    fn db_panel_esc_deselects_then_unfocuses() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.database_state().selected_db.is_some());
        assert!(app.database_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.database_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.database_state().selected_db.is_none());
        assert_eq!(app.focused_panel(), Some(panel::DATABASE));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn db_panel_select_auto_enters_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.database_state().editing_query);
    }

    #[test]
    fn db_panel_e_enters_query_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.database_state().editing_query);
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.database_state().editing_query);
    }

    #[test]
    fn query_editing_enter_returns_run_query_and_appends_history() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["test.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('S')));
        app.handle_key(key(KeyCode::Char('Q')));
        app.handle_key(key(KeyCode::Char('L')));
        let action = app.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Action::RunQuery(_, _)));
        assert!(app.database_state().editing_query);
        assert!(app.database_state().textarea_text().is_empty());
        assert_eq!(app.database_state().history.len(), 1);
    }

    #[test]
    fn e_ignored_without_db_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.database_state_mut().selected_db = Some("a.db".into());
        app.handle_key(key(KeyCode::Char('e')));
        assert!(!app.database_state().editing_query);
    }

    #[test]
    fn e_enters_editing_when_focused_with_selected_db() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc)); // exit EditingQuery
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.database_state().editing_query);
        assert_eq!(app.focused_panel(), Some(panel::DATABASE));
    }

    #[test]
    fn up_down_with_selected_db_scrolls_history_when_focused() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc)); // exit EditingQuery
        app.database_state_mut().history = vec![
            crate::database::ReplLine::Input("SELECT 1".into()),
            crate::database::ReplLine::Output("1".into()),
        ];
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.database_state().scroll, 1);
    }

    #[test]
    fn up_down_ignored_without_db_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().selected_db = Some("a.db".into());
        app.database_state_mut().history = vec![
            crate::database::ReplLine::Input("SELECT 1".into()),
        ];
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.database_state().scroll, 0);
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
        app.database_state_mut().databases = vec!["a.db".into(), "b.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Down));
        let action = app.handle_key(key(KeyCode::Char('p')));
        assert!(matches!(action, Action::Noop));
        assert_eq!(app.database_state().confirming_pull.as_deref(), Some("b.db"));
    }

    #[test]
    fn pp_confirms_pull() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('p')));
        let action = app.handle_key(key(KeyCode::Char('p')));
        assert!(matches!(action, Action::PullDb(db) if db == "a.db"));
        assert!(app.database_state().confirming_pull.is_none());
    }

    #[test]
    fn any_key_cancels_pull_confirmation() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('p')));
        assert!(app.database_state().confirming_pull.is_some());
        let action = app.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Action::Noop));
        assert!(app.database_state().confirming_pull.is_none());
    }

    #[test]
    fn p_sets_confirming_pull_from_repl_view() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        let action = app.handle_key(key(KeyCode::Char('p')));
        assert!(matches!(action, Action::Noop));
        assert_eq!(app.database_state().confirming_pull.as_deref(), Some("a.db"));
    }

    #[test]
    fn toggle_focus_clears_tag_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.logcat_state().editing, Some(logcat::LogcatEditTarget::Tag));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.logcat_state().editing, None);
    }

    #[test]
    fn toggle_focus_clears_search_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.logcat_state().editing, Some(logcat::LogcatEditTarget::Search));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.logcat_state().editing, None);
    }

    #[test]
    fn switching_focus_clears_query_editing() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.database_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.database_state().editing_query);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('d')));
        assert!(!app.database_state().editing_query);
    }

    #[test]
    fn r_resets_db_from_list_view() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        assert!(matches!(app.handle_key(key(KeyCode::Char('r'))), Action::ResetDb));
        assert!(app.database_state().databases.is_empty());
    }

    #[test]
    fn r_resets_db_from_repl_view() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(app.database_state().selected_db.is_some());
        assert!(matches!(app.handle_key(key(KeyCode::Char('r'))), Action::ResetDb));
        assert!(app.database_state().selected_db.is_none());
        assert!(app.database_state().databases.is_empty());
    }

    #[test]
    fn c_copies_full_history() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        app.database_state_mut().push_query("SELECT 1");
        app.database_state_mut().push_result("1");
        let action = app.handle_key(key(KeyCode::Char('c')));
        assert!(matches!(action, Action::CopyDbResult(ref s) if s == "> SELECT 1\n1"));
    }

    #[test]
    fn c_noop_without_output() {
        let mut app = App::new(None, Some("com.test"));
        app.database_state_mut().databases = vec!["a.db".into()];
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(matches!(app.handle_key(key(KeyCode::Char('c'))), Action::Noop));
    }

    #[test]
    fn z_toggles_zoom_for_logcat() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));

        let original_vis = *app.panel_visibility();
        let original_cmd = app.commands().visible;

        app.handle_key(key(KeyCode::Char('z')));
        assert!(app.is_zoomed());
        assert_eq!(app.panel_visibility(), &[true, false, false, false, false, false, false, false, false]);
        assert!(!app.commands().visible);

        app.handle_key(key(KeyCode::Char('z')));
        assert!(!app.is_zoomed());
        assert_eq!(app.panel_visibility(), &original_vis);
        assert_eq!(app.commands().visible, original_cmd);
    }

    #[test]
    fn z_toggles_zoom_for_commands() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.focused_panel(), Some(panel::COMMANDS));

        app.handle_key(key(KeyCode::Char('z')));
        assert!(app.is_zoomed());
        assert_eq!(app.panel_visibility(), &[false; 9]);
        assert!(app.commands().visible);

        app.handle_key(key(KeyCode::Char('z')));
        assert!(!app.is_zoomed());
        assert_eq!(app.panel_visibility(), &[true; 9]);
    }

    #[test]
    fn esc_exits_zoom() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        let original_vis = *app.panel_visibility();

        app.handle_key(key(KeyCode::Char('z')));
        assert!(app.is_zoomed());

        app.handle_key(key(KeyCode::Esc));
        assert!(!app.is_zoomed());
        assert_eq!(app.panel_visibility(), &original_vis);
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn visibility_toggle_clears_zoom() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('z')));
        assert!(app.is_zoomed());

        app.handle_key(key(KeyCode::Char('2')));
        assert!(!app.is_zoomed());
    }

    #[test]
    fn z_noop_without_focus() {
        let mut app = App::new(None, Some("com.test"));
        let original_vis = *app.panel_visibility();
        app.handle_key(key(KeyCode::Char('z')));
        assert!(!app.is_zoomed());
        assert_eq!(app.panel_visibility(), &original_vis);
    }

    #[test]
    fn focus_key_ignored_for_hidden_panel() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('1')));
        assert!(!app.panel_visibility()[0]);
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn hiding_focused_panel_clears_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focused_panel(), Some(panel::LOGCAT));
        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.focused_panel(), None);
    }

    #[test]
    fn hiding_commands_clears_focus() {
        let mut app = App::new(None, Some("com.test"));
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.focused_panel(), Some(panel::COMMANDS));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focused_panel(), None);
        app.handle_key(key(KeyCode::Char('0')));
        assert!(!app.commands().visible);
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.focused_panel(), None);
    }
}
