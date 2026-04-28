use std::sync::{mpsc, Arc};

use color_eyre::Result;

use crate::adb::{Adb, AdbStatus, Device};
use crate::app::{Action, App, Navigation, SessionView};
use crate::data::DataSources;
use crate::replay::{self, ReplaySession};
use crate::session::{self, SessionId};
use crate::toolbar;

// Vocabulary: `replay.rs` is the *loader* (one-shot read off disk).
// `crate::app::SessionView` is the *runtime* state held by App while the
// user keeps a loaded session on screen. Dispatch only owns the in-flight
// load receiver (`session_view_rx`); the view itself lives on App. User-
// facing wording is "open session" / "viewing session" / "live".

type Rollback = Box<dyn FnOnce(&mut App) + Send>;

pub struct CommandResult {
    pub error_msg: String,
    pub result: Result<()>,
    pub rollback: Option<Rollback>,
}

pub struct DispatchContext {
    pub adb: Arc<dyn Adb>,
    pub data: Option<DataSources>,
    pub title: String,
    pub devices_rx: Option<mpsc::Receiver<Vec<Device>>>,
    pub packages_rx: Option<mpsc::Receiver<Vec<String>>>,
    pub pending_emulator_rx: Option<mpsc::Receiver<Device>>,
    pub pending_build_rx: Option<mpsc::Receiver<PendingBuild>>,
    pub command_tx: mpsc::Sender<CommandResult>,
    pub command_rx: mpsc::Receiver<CommandResult>,
    pub adb_status_rx: mpsc::Receiver<AdbStatus>,
    pub pending_redraw: bool,
    /// Pending session load triggered by `Action::OpenSession`. Resolves
    /// on the next `poll_receivers` tick by handing the loaded data to
    /// `App::open_session_view`. Once installed there, dispatch does not
    /// retain any view state of its own — `app.is_viewing_session()` is
    /// the only source of truth.
    pub session_view_rx: Option<mpsc::Receiver<Result<ReplaySession, String>>>,
}

pub struct PendingBuild {
    pub device: Device,
    /// Populated when we just (re)selected a device and ran `list_packages`.
    pub packages: Option<Vec<String>>,
    /// The built `DataSources` + package, present once `DataSources::new` returned.
    pub built: Option<AutoBuild>,
}

pub struct AutoBuild {
    pub package: String,
    pub data: DataSources,
    pub title: String,
}

impl DispatchContext {
    pub fn poll_receivers(&mut self, app: &mut App) {
        if let Some(rx) = &self.devices_rx
            && let Ok(devices) = rx.try_recv()
        {
            app.toolbar_mut().receive_devices(devices);
            self.devices_rx = None;
        }
        if let Some(rx) = &self.packages_rx
            && let Ok(packages) = rx.try_recv()
        {
            if let Some(auto_pkg) = app.toolbar_mut().receive_packages(packages) {
                // Worker callback: only touch *data* state. The user's
                // popup (Settings / Dialog / Dropdown) must survive an
                // auto-package match completing in the background.
                app.toolbar_mut().package = Some(auto_pkg.clone());
                app.reset_panel_data_for_app(&auto_pkg);
                if let Some(device) = app.toolbar().device.clone() {
                    self.pending_build_rx = Some(spawn_build(
                        self.adb.clone(),
                        device,
                        auto_pkg,
                        *app.panel_visibility(),
                    ));
                }
            }
            self.packages_rx = None;
        }
        if let Some(rx) = &self.pending_emulator_rx
            && let Ok(device) = rx.try_recv()
        {
            self.pending_emulator_rx = None;
            self.dispatch(Action::ChangeDevice(device), app);
        }
        if let Some(rx) = &self.pending_build_rx
            && let Ok(pb) = rx.try_recv()
        {
            self.pending_build_rx = None;
            let no_auto_package = pb.built.is_none();
            apply_pending_build(self, app, pb);
            // After a build that found no last-package match, prompt the user
            // to pick one — but only if they haven't already navigated
            // elsewhere. This is the *only* place a worker callback can
            // influence navigation, and the `Panels` guard makes the
            // influence well-defined: the popup never lands on top of an
            // existing one. The packages list was already cached by
            // `apply_pending_build`, so the dropdown opens with data, no
            // refetch needed.
            if no_auto_package
                && matches!(app.nav(), Navigation::Panels)
                && app.toolbar().package.is_none()
            {
                app.open_apps_popup();
            }
        }
        while let Ok(cr) = self.command_rx.try_recv() {
            if cr.result.is_err() {
                app.set_status_flash(cr.error_msg, true);
                if let Some(rollback) = cr.rollback {
                    rollback(app);
                }
            }
        }
        while let Ok(status) = self.adb_status_rx.try_recv() {
            match status {
                AdbStatus::Timeout { args } => {
                    app.set_status_flash(format!("adb timed out: {args}"), true);
                }
            }
        }
        if let Some(rx) = &self.session_view_rx
            && let Ok(result) = rx.try_recv()
        {
            self.session_view_rx = None;
            match result {
                Ok(session) => self.apply_session_view(app, session),
                Err(e) => {
                    app.set_status_flash(format!("session load failed: {e}"), true);
                    app.close_session_view();
                    app.toolbar_mut().session_label = None;
                }
            }
        }
    }

    pub fn dispatch(&mut self, action: Action, app: &mut App) -> bool {
        let (serial, package) = {
            let tb = app.toolbar();
            (
                tb.device.as_ref().map(|d| d.serial.clone()),
                tb.package.clone(),
            )
        };

        // While viewing a captured session, side-effecting actions against
        // the device are pointless — the user is looking at history, not
        // touching live state. Short-circuit with a status flash so users
        // learn the affordance without us actually dispatching the call.
        // Quit, navigation, and session-management actions still go
        // through.
        if app.is_viewing_session() && action_is_blocked_while_viewing(&action) {
            app.set_status_flash(
                "viewing session — open ^h history to go live".into(),
                true,
            );
            return false;
        }

        match action {
            Action::Quit => return true,
            Action::FetchDevices => {
                self.devices_rx = Some(spawn_fetch_devices(&self.adb));
            }
            Action::FetchApps => {
                if let Some(s) = &serial {
                    self.packages_rx = Some(spawn_fetch_packages(&self.adb, s));
                }
            }
            Action::ChangeDevice(d) => {
                self.clear_session_view(app);
                let last = app.toolbar().last_package.clone();
                app.commands_mut().is_emulator = d.serial.starts_with("emulator-");
                app.toolbar_mut().device = Some(d.clone());
                app.toolbar_mut().device_connected = true;
                app.toolbar_mut().package = None;
                app.reset_for_new_app("");
                self.data = None;
                self.title = String::new();
                self.pending_build_rx = Some(spawn_select_and_build(
                    self.adb.clone(),
                    d,
                    last,
                    *app.panel_visibility(),
                ));
            }
            Action::ChangeApp(p) => {
                self.clear_session_view(app);
                app.toolbar_mut().package = Some(p.clone());
                app.toolbar_mut().last_package = Some(p.clone());
                toolbar::save_last_package(&p);
                app.reset_for_new_app(&p);
                self.data = None;
                self.title = String::new();
                if let Some(device) = app.toolbar().device.clone() {
                    self.pending_build_rx = Some(spawn_build(
                        self.adb.clone(),
                        device,
                        p,
                        *app.panel_visibility(),
                    ));
                }
            }
            Action::OpenApp => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Opening app...".into(), false);
                    spawn_app_action(&self.adb, s, p, "Failed to open app", &self.command_tx, None, |adb, s, p| {
                        adb.launch_app(s, p)
                    });
                }
            }
            Action::OpenAppInfo => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Opening app info...".into(), false);
                    spawn_app_action(&self.adb, s, p, "Failed to open app info", &self.command_tx, None, |adb, s, p| adb.open_app_info(s, p));
                }
            }
            Action::KillApp => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Killing app...".into(), false);
                    spawn_app_action(&self.adb, s, p, "Failed to kill app", &self.command_tx, None, |adb, s, p| adb.kill_app(s, p));
                }
            }
            Action::ClearData => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Clearing data...".into(), false);
                    spawn_app_action(&self.adb, s, p, "Failed to clear data", &self.command_tx, None, |adb, s, p| adb.clear_app_data(s, p));
                }
            }
            Action::UninstallApp => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Uninstalling...".into(), false);
                    spawn_app_action(&self.adb, s, p, "Failed to uninstall", &self.command_tx, None, |adb, s, p| adb.uninstall_app(s, p));
                }
            }
            Action::WakeScreen => {
                if let Some(s) = &serial {
                    app.set_status_flash("Waking screen...".into(), false);
                    spawn_app_action(&self.adb, s, package.as_deref().unwrap_or(""), "Failed to wake screen", &self.command_tx, None, |adb, s, _| adb.wake_screen(s));
                }
            }
            Action::ToggleLayoutBounds => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::layout_bounds, set: App::set_layout_bounds,
                    label: "Layout bounds", error_msg: "Failed to set layout bounds",
                    adb_fn: |adb, s, enabled| adb.set_layout_bounds(s, enabled),
                });
            }
            Action::ToggleAirplaneMode => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::airplane_mode, set: App::set_airplane_mode,
                    label: "Airplane mode", error_msg: "Failed to toggle airplane mode",
                    adb_fn: |adb, s, enabled| adb.set_airplane_mode(s, enabled),
                });
            }
            Action::ToggleWifi => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::wifi_enabled, set: App::set_wifi_enabled,
                    label: "Wi-Fi", error_msg: "Failed to toggle Wi-Fi",
                    adb_fn: |adb, s, enabled| adb.set_wifi_enabled(s, enabled),
                });
            }
            Action::ToggleDarkMode => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::dark_mode, set: App::set_dark_mode,
                    label: "Dark mode", error_msg: "Failed to toggle dark mode",
                    adb_fn: |adb, s, enabled| adb.set_dark_mode(s, enabled),
                });
            }
            Action::ToggleShowTaps => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::show_taps, set: App::set_show_taps,
                    label: "Show taps", error_msg: "Failed to toggle show taps",
                    adb_fn: |adb, s, enabled| adb.set_show_taps(s, enabled),
                });
            }
            Action::TogglePointerLocation => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::pointer_location, set: App::set_pointer_location,
                    label: "Pointer location", error_msg: "Failed to toggle pointer location",
                    adb_fn: |adb, s, enabled| adb.set_pointer_location(s, enabled),
                });
            }
            Action::ToggleGpuRendering => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::gpu_rendering, set: App::set_gpu_rendering,
                    label: "GPU rendering", error_msg: "Failed to toggle GPU rendering",
                    adb_fn: |adb, s, enabled| adb.set_gpu_rendering(s, enabled),
                });
            }
            Action::ToggleTalkback => {
                dispatch_toggle(app, &serial, &self.adb, &self.command_tx, ToggleOpts {
                    get: App::talkback, set: App::set_talkback,
                    label: "TalkBack", error_msg: "Failed to toggle TalkBack",
                    adb_fn: |adb, s, enabled| adb.set_talkback_enabled(s, enabled),
                });
            }
            Action::WirelessAdb => {
                if let Some(s) = &serial {
                    app.set_status_flash("Enabling wireless ADB...".into(), false);
                    spawn_app_action(&self.adb, s, "", "Failed to enable wireless ADB", &self.command_tx, None, |adb, s, _| adb.enable_wireless_adb(s).map(|_| ()));
                }
            }
            Action::Screenshot => {
                if let (Some(s), Some(_p)) = (&serial, &package)
                    && let Some(dest_dir) = artifact_dir_for(self, app, "screenshots")
                {
                    app.set_status_flash("Taking screenshot...".into(), false);
                    let adb = self.adb.clone();
                    let serial = s.clone();
                    let tx = self.command_tx.clone();
                    std::thread::spawn(move || {
                        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let dest = dest_dir.join(format!("{timestamp}_screenshot.png"));
                        let result = std::fs::create_dir_all(&dest_dir)
                            .map_err(|e| color_eyre::eyre::eyre!("create dir: {e}"))
                            .and_then(|_| adb.take_screenshot(&serial, &dest));
                        if result.is_ok() {
                            let _ = open::that(&dest);
                        }
                        let _ = tx.send(CommandResult {
                            error_msg: "Failed to take screenshot".into(),
                            result,
                            rollback: None,
                        });
                    });
                }
            }
            Action::TogglePermission(perm, granted) => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    let adb = self.adb.clone();
                    let serial = s.clone();
                    let package = p.clone();
                    std::thread::spawn(move || {
                        let _ = if granted {
                            adb.grant_permission(&serial, &package, &perm)
                        } else {
                            adb.revoke_permission(&serial, &package, &perm)
                        };
                    });
                }
            }
            Action::ResetLogcat => {
                if let Some(d) = &mut self.data {
                    d.logcat_lines.clear();
                }
            }
            Action::RefreshDb => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.restart_db_detection(self.adb.clone(), s.clone(), p.clone());
                }
            }
            Action::CopyDbResult(text) => {
                crate::clipboard::copy_to_clipboard(&text);
            }
            Action::CopyPath(text) => {
                if crate::clipboard::copy_to_clipboard(&text) {
                    app.set_status_flash(format!("copied {text}"), false);
                } else {
                    app.set_status_flash("clipboard copy failed".into(), true);
                }
            }
            Action::OpenLogcat => {
                // The captured logcat already lives on disk inside the
                // displayed session — no need to rewrite a filtered copy.
                // Resolve which session is on screen (live or viewed) and
                // hand the file to `$EDITOR` directly. Filter is dropped
                // intentionally; users grep inside the editor instead.
                let id = if let Some(view) = app.session_view() {
                    Some(view.id.clone())
                } else {
                    self.data.as_ref().and_then(|d| d.active_session_id().cloned())
                };
                match id {
                    Some(id) => {
                        let path = id.dir().join("logcat.log");
                        if path.exists() {
                            if launch_editor(&path) {
                                self.pending_redraw = true;
                            }
                        } else {
                            app.set_status_flash("no session log yet".into(), true);
                        }
                    }
                    None => {
                        app.set_status_flash("no session log yet".into(), true);
                    }
                }
            }
            Action::RunQuery(db, sql) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_query(self.adb.clone(), s.clone(), p.clone(), db, sql);
                }
            }
            Action::PullDb(db) => {
                if let (Some(s), Some(p), Some(dest)) = (
                    &serial,
                    &package,
                    artifact_dir_for(self, app, "db"),
                ) && let Some(d) = &mut self.data
                {
                    d.start_pull(self.adb.clone(), s.clone(), p.clone(), db, dest);
                }
            }
            Action::DbFetchTables(db) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_fetch_tables(self.adb.clone(), s.clone(), p.clone(), db);
                }
            }
            Action::RefreshFiles => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_list_dir(self.adb.clone(), s.clone(), p.clone(), ".".to_string());
                }
            }
            Action::ExpandDir(path) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_list_dir(self.adb.clone(), s.clone(), p.clone(), path);
                }
            }
            Action::OpenFile(path) => {
                if let (Some(s), Some(p), Some(dest)) = (
                    &serial,
                    &package,
                    artifact_dir_for(self, app, "files"),
                ) && let Some(d) = &mut self.data
                {
                    d.start_pull_file(self.adb.clone(), s.clone(), p.clone(), path, dest);
                }
            }
            Action::StartTrace => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    let preset = app.trace_state().preset;
                    d.start_trace(self.adb.clone(), s.clone(), p.clone(), preset);
                }
            }
            Action::StopTrace => {
                if let (Some(s), Some(p), Some(dest)) = (
                    &serial,
                    &package,
                    artifact_dir_for(self, app, "traces"),
                ) && let Some(d) = &mut self.data
                {
                    let preset = app.trace_state().preset;
                    d.stop_and_pull_trace(self.adb.clone(), s.clone(), p.clone(), preset, dest);
                }
            }
            Action::LaunchEmulator(name) => {
                let adb = self.adb.clone();
                let adb2 = self.adb.clone();
                let avd_name = name.clone();
                std::thread::spawn(move || {
                    let _ = adb.launch_emulator(&name);
                });
                self.pending_emulator_rx = Some(spawn_await_emulator(adb2, avd_name));
            }
            Action::MirrorDevice => {
                if let Some(s) = &serial {
                    app.set_status_flash("Starting mirror...".into(), false);
                    let has_scrcpy = std::process::Command::new("scrcpy")
                        .arg("--version")
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .is_ok();
                    if has_scrcpy {
                        let serial = s.clone();
                        std::thread::spawn(move || {
                            let _ = std::process::Command::new("scrcpy")
                                .args(["-s", &serial])
                                .stdin(std::process::Stdio::null())
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .spawn();
                        });
                    } else {
                        app.open_dialog(
                            "scrcpy is not installed.\n\
                             \n\
                             Install with:\n\
                             \n\
                             macOS:   brew install scrcpy\n\
                             Linux:   sudo apt install scrcpy\n\
                             Windows: scoop install scrcpy"
                                .to_string(),
                        );
                    }
                }
            }
            Action::OpenInEditor(text) => {
                if let Some(dir) = artifact_dir_for(self, app, "dumps")
                    && open_in_editor(text, &dir, "issues", "txt")
                {
                    self.pending_redraw = true;
                }
            }
            Action::RefreshSessions => {
                refresh_sessions(self, app);
            }
            Action::OpenSession(id) => {
                self.open_session(app, id);
            }
            Action::DeleteSession(id) => {
                if let Err(e) = session::delete_session(&id) {
                    app.set_status_flash(format!("delete failed: {e}"), true);
                }
                refresh_sessions(self, app);
            }
            Action::CloseSession => {
                self.close_session(app);
            }
            Action::Noop | Action::Unfocus => {}
        }
        false
    }
}

/// True for actions that try to mutate the device or kick off background
/// adb work. While viewing a captured session, these are short-circuited —
/// the user is looking at history, not interacting with the device — but
/// navigation, refresh, copy-to-clipboard, and session-management actions
/// still go through.
fn action_is_blocked_while_viewing(action: &Action) -> bool {
    matches!(
        action,
        Action::OpenApp
            | Action::OpenAppInfo
            | Action::KillApp
            | Action::ClearData
            | Action::UninstallApp
            | Action::WakeScreen
            | Action::ToggleLayoutBounds
            | Action::ToggleAirplaneMode
            | Action::TogglePermission(_, _)
            | Action::Screenshot
            | Action::ToggleWifi
            | Action::ToggleDarkMode
            | Action::ToggleShowTaps
            | Action::TogglePointerLocation
            | Action::ToggleGpuRendering
            | Action::ToggleTalkback
            | Action::WirelessAdb
            | Action::MirrorDevice
            | Action::StartTrace
            | Action::StopTrace
            | Action::RunQuery(_, _)
            | Action::PullDb(_)
            | Action::DbFetchTables(_)
            | Action::RefreshDb
            | Action::RefreshFiles
            | Action::ExpandDir(_)
            | Action::OpenFile(_)
            // ResetLogcat clears `data.logcat_lines` — the live tail. The
            // user can still see it after closing the view, so dropping
            // it from a hidden buffer is a silent surprise.
            | Action::ResetLogcat,
    )
}

/// Re-scan the sessions root for the current package and stamp the active
/// live session id (if any) so the dialog can render its `(live)` badge.
pub fn refresh_sessions(ctx: &mut DispatchContext, app: &mut App) {
    let package = app.toolbar().package.clone();
    let sessions = package
        .as_deref()
        .map(session::list_sessions)
        .unwrap_or_default();
    let live_id = ctx.data.as_ref().and_then(|d| d.active_session_id().cloned());
    let state = app.sessions_state_mut();
    state.set_sessions(sessions);
    state.live_id = live_id;
}

impl DispatchContext {
    fn open_session(&mut self, app: &mut App, id: SessionId) {
        // Capture is "always on": we keep `self.data` alive so the live
        // SessionWriter keeps appending while the user looks at history.
        // App-state pushes are gated by `data.rs::poll` against
        // `app.is_viewing_session()`.
        self.session_view_rx = Some(replay::load(id.clone()));
        if let Some(pkg) = app.toolbar().package.clone() {
            // Clear panel data shapes so partial-load states never bleed
            // older session contents into the view.
            app.reset_panel_data_for_app(&pkg);
        }
        app.set_status_flash(format!("loading session {}…", id.dir_name), false);
    }

    fn close_session(&mut self, app: &mut App) {
        if !self.clear_session_view(app) {
            return;
        }
        // No respawn — `self.data` was kept alive across the session view.
        // Reset App state so the live workers' next pushes start fresh; the
        // logcat panel continues to show the in-memory tail accumulated by
        // `DataSources` while the captured session was open.
        if let Some(pkg) = app.toolbar().package.clone() {
            app.reset_panel_data_for_app(&pkg);
        }
    }

    /// Drop session-view state without re-spawning live workers. Used by
    /// `close_session` and by ChangeDevice/ChangeApp (which respawn the
    /// build themselves). Returns `true` if there was anything to clear —
    /// either an installed view or a still-pending load.
    fn clear_session_view(&mut self, app: &mut App) -> bool {
        let had_pending = self.session_view_rx.take().is_some();
        let had_view = app.close_session_view();
        if had_view {
            app.toolbar_mut().session_label = None;
        }
        had_view || had_pending
    }

    fn apply_session_view(&mut self, app: &mut App, session: ReplaySession) {
        let label = format!(
            "{} · {}",
            session.meta.package,
            session.meta.started_at.format("%Y-%m-%d %H:%M:%S"),
        );
        app.toolbar_mut().session_label = Some(label);

        // Swap captured state into App. NetworkState/IssuesState/MonitorState
        // already match the live shapes, so no translation needed.
        *app.network_state_mut() = session.network;
        *app.issues_state_mut() = session.issues;
        *app.monitor_state_mut() = session.monitor;
        // Trace panel reads its file list from a directory; point it at the
        // captured session's traces/.
        let mut trace_state = crate::trace::TraceState::new(&session.meta.package);
        trace_state.pulled_traces = crate::trace::list_perfetto_traces(&session.traces_dir);
        *app.trace_state_mut() = trace_state;

        // Single transition: install the view, snapshot+collapse layout,
        // drop focus. After this returns there are no half-states for any
        // caller to mishandle.
        app.open_session_view(SessionView {
            id: session.id,
            logcat_lines: std::sync::Arc::from(session.logcat),
        });
        app.set_status_flash("session loaded".into(), false);
    }
}

/// Resolve the on-disk directory an artifact (screenshot, pulled file,
/// editor dump, etc.) should land in. The single source of truth for "where
/// does this go on disk" — kept here so individual action handlers don't
/// each re-derive the policy.
///
/// Resolution order:
/// 1. Viewing a captured session → that session's dir + `kind` subdir.
/// 2. Live capture has an active session → its dir + `kind` subdir.
/// 3. Else → `<sessions_root>/<package>/_unsessioned/<kind>/`. The `_`
///    prefix sorts before timestamped session dirs and is filtered out
///    of the history dialog by the metadata.json check.
///
/// Returns `None` only when no `package` is selected at all (which means
/// the action wasn't reachable to begin with — every caller already gates
/// on `app.toolbar().package.is_some()`).
pub(crate) fn artifact_dir_for(
    ctx: &DispatchContext,
    app: &App,
    kind: &str,
) -> Option<std::path::PathBuf> {
    artifact_dir_for_inner(
        app.session_view().map(|v| &v.id),
        ctx.data.as_ref().and_then(|d| d.active_session_id()),
        app.toolbar().package.as_deref(),
        kind,
    )
}

/// Pure helper, broken out so the resolution policy is testable without
/// constructing a real `DispatchContext` (which spawns worker threads).
/// `viewed_session` being `Some` already implies viewing mode (single
/// source of truth on `App::session_view`), so no separate flag is needed.
fn artifact_dir_for_inner(
    viewed_session: Option<&SessionId>,
    live_session: Option<&SessionId>,
    package: Option<&str>,
    kind: &str,
) -> Option<std::path::PathBuf> {
    if let Some(id) = viewed_session {
        return Some(id.dir().join(kind));
    }
    if let Some(id) = live_session {
        return Some(id.dir().join(kind));
    }
    let package = package?;
    Some(
        session::sessions_root()
            .join(package)
            .join("_unsessioned")
            .join(kind),
    )
}

/// Writes `text` into a session-resident dumps file and opens it in
/// `$EDITOR`. The destination directory is supplied by the caller (typically
/// via `artifact_dir_for(..., "dumps")`); `source` is folded into the file
/// name so callers can tell `issues` and `network` dumps apart at a glance.
/// Returns `true` when a terminal editor was launched synchronously, so the
/// caller can force ratatui to redraw after the editor exits.
fn open_in_editor(text: String, dir: &std::path::Path, source: &str, ext: &str) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = dir.join(format!("{timestamp}_{source}.{ext}"));
    if std::fs::write(&path, &text).is_err() {
        return false;
    }
    launch_editor(&path)
}

/// Opens `path` in the user's editor. Terminal editors (nvim, vim, nano, …)
/// are run synchronously with holo's raw-mode + alt-screen suspended so they
/// don't fight holo for the tty. GUI editors are spawned detached.
pub(crate) fn launch_editor(path: &std::path::Path) -> bool {
    let editor = std::env::var("EDITOR").ok().or_else(|| std::env::var("VISUAL").ok());
    match editor {
        Some(ref e) if is_terminal_editor(e) => {
            run_editor_synchronously(e, path);
            true
        }
        Some(e) => {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{} \"{}\"", e, path.display()))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            false
        }
        None => {
            let _ = open::that(path);
            false
        }
    }
}

fn is_terminal_editor(cmd: &str) -> bool {
    let first = cmd.split_whitespace().next().unwrap_or("");
    let name = std::path::Path::new(first)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    matches!(
        name,
        "nvim" | "vim" | "vi" | "nano" | "micro" | "helix" | "hx"
            | "kak" | "kakoune" | "emacs" | "joe" | "pico" | "ed"
    )
}

fn run_editor_synchronously(editor: &str, path: &std::path::Path) {
    use crossterm::{
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"{}\"", editor, path.display()))
        .status();
    let _ = enable_raw_mode();
    let _ = execute!(std::io::stdout(), EnterAlternateScreen);
}

struct ToggleOpts {
    get: fn(&App) -> bool,
    set: fn(&mut App, bool),
    label: &'static str,
    error_msg: &'static str,
    adb_fn: fn(&dyn Adb, &str, bool) -> Result<()>,
}

fn dispatch_toggle(
    app: &mut App,
    serial: &Option<String>,
    adb: &Arc<dyn Adb>,
    tx: &mpsc::Sender<CommandResult>,
    opts: ToggleOpts,
) {
    (opts.set)(app, !(opts.get)(app));
    let enabled = (opts.get)(app);
    let flash = if enabled { format!("{} on", opts.label) } else { format!("{} off", opts.label) };
    app.set_status_flash(flash, false);
    if let Some(s) = serial {
        let rollback: Rollback = Box::new(move |app| (opts.set)(app, !enabled));
        let adb = adb.clone();
        let serial = s.clone();
        let error_msg = opts.error_msg.to_string();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = (opts.adb_fn)(&*adb, &serial, enabled);
            let _ = tx.send(CommandResult { error_msg, result, rollback: Some(rollback) });
        });
    }
}

fn spawn_app_action(
    adb: &Arc<dyn Adb>,
    serial: &str,
    package: &str,
    error_msg: &str,
    tx: &mpsc::Sender<CommandResult>,
    rollback: Option<Rollback>,
    f: impl FnOnce(Arc<dyn Adb>, &str, &str) -> Result<()> + Send + 'static,
) {
    let adb = adb.clone();
    let serial = serial.to_string();
    let package = package.to_string();
    let tx = tx.clone();
    let error_msg = error_msg.to_string();
    std::thread::spawn(move || {
        let result = f(adb, &serial, &package);
        let _ = tx.send(CommandResult { error_msg, result, rollback });
    });
}

fn spawn_fetch_devices(adb: &Arc<dyn Adb>) -> mpsc::Receiver<Vec<Device>> {
    let (tx, rx) = mpsc::channel();
    let adb = adb.clone();
    std::thread::spawn(move || {
        let mut devices = adb.list_devices().unwrap_or_default();

        let mut running_avds = std::collections::HashSet::new();
        for d in &devices {
            if d.serial.starts_with("emulator-")
                && let Ok(name) = adb.get_avd_name(&d.serial)
            {
                running_avds.insert(name);
            }
        }

        if let Ok(avds) = adb.list_avds() {
            for avd in avds {
                if !running_avds.contains(&avd) {
                    devices.push(Device {
                        serial: avd.clone(),
                        model: Some(avd),
                        device: None,
                        connected: false,
                    });
                }
            }
        }

        let _ = tx.send(devices);
    });
    rx
}

fn spawn_fetch_packages(adb: &Arc<dyn Adb>, serial: &str) -> mpsc::Receiver<Vec<String>> {
    let (tx, rx) = mpsc::channel();
    let adb = adb.clone();
    let serial = serial.to_string();
    std::thread::spawn(move || {
        if let Ok(packages) = adb.list_packages(&serial) {
            let _ = tx.send(packages);
        }
    });
    rx
}

fn spawn_await_emulator(adb: Arc<dyn Adb>, avd_name: String) -> mpsc::Receiver<Device> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(1);
        for _ in 0..60 {
            std::thread::sleep(interval);
            let devices = match adb.list_devices() {
                Ok(d) => d,
                Err(_) => continue,
            };
            for d in &devices {
                if d.serial.starts_with("emulator-")
                    && let Ok(name) = adb.get_avd_name(&d.serial)
                    && name == avd_name
                {
                    let _ = tx.send(d.clone());
                    return;
                }
            }
        }
    });
    rx
}

pub fn build_title(data: &DataSources) -> String {
    match &data.app_version {
        Some((name, code)) if !name.is_empty() => {
            format!(" {} / {} ", name, code)
        }
        _ => String::new(),
    }
}

pub fn apply_initial_state(app: &mut App, data: &DataSources) {
    app.set_layout_bounds(data.initial_layout_bounds);
    app.set_airplane_mode(data.initial_airplane_mode);
    app.set_wifi_enabled(data.initial_wifi_enabled);
    app.set_dark_mode(data.initial_dark_mode);
    app.set_show_taps(data.initial_show_taps);
    app.set_pointer_location(data.initial_pointer_location);
    app.set_gpu_rendering(data.initial_gpu_rendering);
    app.set_talkback(data.initial_talkback);
    app.set_measure_sdk_detected(data.has_measure_sdk);
}

/// Wedged adb shells made startup synchronous — by the time `DataSources::new`
/// returns, the user has already seen a blank, unresponsive TUI. Spawn the
/// `list_packages` + `DataSources::new` path so the main loop can render and
/// take input while the device is being probed.
pub fn spawn_select_and_build(
    adb: Arc<dyn Adb>,
    device: Device,
    last_package: Option<String>,
    panel_vis: [bool; 8],
) -> mpsc::Receiver<PendingBuild> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let packages = adb.list_packages(&device.serial).unwrap_or_default();
        let auto_pkg = last_package
            .as_deref()
            .and_then(|lp| packages.iter().find(|p| p.as_str() == lp).cloned());
        let built = auto_pkg.map(|pkg| {
            let data = DataSources::new(
                adb.clone(),
                &device.serial,
                device.model.as_deref(),
                &pkg,
                &panel_vis,
            );
            let title = build_title(&data);
            AutoBuild { package: pkg, data, title }
        });
        let _ = tx.send(PendingBuild {
            device,
            packages: Some(packages),
            built,
        });
    });
    rx
}

/// Same off-thread treatment as [`spawn_select_and_build`], for paths that
/// already know which package to build (ChangeApp, reconnect).
pub fn spawn_build(
    adb: Arc<dyn Adb>,
    device: Device,
    package: String,
    panel_vis: [bool; 8],
) -> mpsc::Receiver<PendingBuild> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let data = DataSources::new(
            adb.clone(),
            &device.serial,
            device.model.as_deref(),
            &package,
            &panel_vis,
        );
        let title = build_title(&data);
        let _ = tx.send(PendingBuild {
            device,
            packages: None,
            built: Some(AutoBuild { package, data, title }),
        });
    });
    rx
}

fn apply_pending_build(ctx: &mut DispatchContext, app: &mut App, pb: PendingBuild) {
    // Drop late-arriving results that no longer match the toolbar's device.
    if app.toolbar().device.as_ref().map(|d| d.serial.as_str()) != Some(pb.device.serial.as_str()) {
        return;
    }
    if let Some(packages) = pb.packages {
        app.toolbar_mut().receive_packages(packages);
    }
    match pb.built {
        Some(auto) => {
            // Worker callback: data only. `reset_panel_data_for_app` keeps
            // the user's current popup intact.
            app.toolbar_mut().package = Some(auto.package.clone());
            app.reset_panel_data_for_app(&auto.package);
            apply_initial_state(app, &auto.data);
            ctx.title = auto.title;
            ctx.data = Some(auto.data);
        }
        None => {
            // No auto-selected package. The decision to auto-open the App
            // picker is made by `poll_receivers` (after the user-driven
            // navigation state has been observed), not here — this function
            // is a pure data update.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(dir: &str) -> SessionId {
        SessionId {
            package: "com.test".into(),
            dir_name: dir.into(),
        }
    }

    #[test]
    fn resolver_prefers_viewed_session_over_live() {
        let viewed = id("captured");
        let live = id("running");
        let dir = artifact_dir_for_inner(
            Some(&viewed),
            Some(&live),
            Some("com.test"),
            "dumps",
        )
        .unwrap();
        assert!(dir.ends_with("captured/dumps"));
    }

    #[test]
    fn resolver_falls_back_to_live_when_not_viewing() {
        let live = id("running");
        let dir =
            artifact_dir_for_inner(None, Some(&live), Some("com.test"), "screenshots")
                .unwrap();
        assert!(dir.ends_with("running/screenshots"));
    }

    #[test]
    fn resolver_uses_unsessioned_when_no_session_active() {
        let dir =
            artifact_dir_for_inner(None, None, Some("com.test"), "screenshots").unwrap();
        assert!(dir.ends_with("com.test/_unsessioned/screenshots"));
    }

    #[test]
    fn resolver_returns_none_without_a_package() {
        // No package selected and no session anywhere — there's nowhere
        // useful to write, and the action wasn't reachable from any panel
        // path that requires a package.
        let dir = artifact_dir_for_inner(None, None, None, "screenshots");
        assert!(dir.is_none());
    }
}
