use std::sync::{mpsc, Arc};

use color_eyre::Result;

use crate::adb::{Adb, Device};
use crate::app::{Action, App};
use crate::data::DataSources;
use crate::toolbar;

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
    pub command_tx: mpsc::Sender<CommandResult>,
    pub command_rx: mpsc::Receiver<CommandResult>,
    pub pending_redraw: bool,
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
                app.toolbar_mut().package = Some(auto_pkg.clone());
                app.reset_for_new_app(&auto_pkg);
                if let Some(device) = app.toolbar().device.clone() {
                    self.data = Some(build_data(&self.adb, &device, &auto_pkg, app));
                    self.title = build_title(self.data.as_ref().unwrap());
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
        while let Ok(cr) = self.command_rx.try_recv() {
            if cr.result.is_err() {
                app.set_status_flash(cr.error_msg, true);
                if let Some(rollback) = cr.rollback {
                    rollback(app);
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
                let last = app.toolbar().last_package.clone();
                app.commands_mut().is_emulator = d.serial.starts_with("emulator-");
                app.toolbar_mut().device = Some(d.clone());
                app.toolbar_mut().device_connected = true;
                app.toolbar_mut().package = None;
                self.data = None;
                self.title = String::new();
                let (packages, auto) = try_auto_select_package(&self.adb, &d, last.as_deref());
                app.toolbar_mut().receive_packages(packages);
                if let Some(pkg) = auto {
                    app.toolbar_mut().package = Some(pkg.clone());
                    app.reset_for_new_app(&pkg);
                    self.data = Some(build_data(&self.adb, &d, &pkg, app));
                    self.title = build_title(self.data.as_ref().unwrap());
                } else {
                    app.reset_for_new_app("");
                }
            }
            Action::ChangeApp(p) => {
                app.toolbar_mut().package = Some(p.clone());
                app.toolbar_mut().last_package = Some(p.clone());
                toolbar::save_last_package(&p);
                app.reset_for_new_app(&p);
                if let Some(device) = app.toolbar().device.clone() {
                    self.data = Some(build_data(&self.adb, &device, &p, app));
                    self.title = build_title(self.data.as_ref().unwrap());
                }
            }
            Action::OpenApp => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Opening app...".into(), false);
                    spawn_app_action(&self.adb, s, p, "Failed to open app", &self.command_tx, None, |adb, s, p| adb.launch_app(s, p));
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
                if let (Some(s), Some(p)) = (&serial, &package) {
                    app.set_status_flash("Taking screenshot...".into(), false);
                    let adb = self.adb.clone();
                    let serial = s.clone();
                    let package = p.clone();
                    let tx = self.command_tx.clone();
                    std::thread::spawn(move || {
                        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let dest = std::env::temp_dir().join("holo")
                            .join(&package)
                            .join("screenshots")
                            .join(format!("{timestamp}_screenshot.png"));
                        let result = adb.take_screenshot(&serial, &dest);
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
            Action::OpenLogcat => {
                if let Some(d) = &self.data {
                    let filter = &app.logcat_state().filter;
                    let text: String = d.logcat_lines
                        .iter()
                        .filter(|line| filter.matches(line))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    if open_in_editor(text, package.as_deref(), "logcat", "log") {
                        self.pending_redraw = true;
                    }
                }
            }
            Action::ZoomIn => {
                // Handled in dispatch_panel_key
            }
            Action::RunQuery(db, sql) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_query(self.adb.clone(), s.clone(), p.clone(), db, sql);
                }
            }
            Action::PullDb(db) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_pull(self.adb.clone(), s.clone(), p.clone(), db);
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
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_pull_file(self.adb.clone(), s.clone(), p.clone(), path);
                }
            }
            Action::StartTrace => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    let preset = app.trace_state().preset;
                    d.start_trace(self.adb.clone(), s.clone(), p.clone(), preset);
                }
            }
            Action::StopTrace => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    let preset = app.trace_state().preset;
                    d.stop_and_pull_trace(self.adb.clone(), s.clone(), p.clone(), preset);
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
                        app.dialog = Some(
                            "scrcpy is not installed.\n\
                             \n\
                             Install with:\n\
                             \n\
                             macOS:   brew install scrcpy\n\
                             Linux:   sudo apt install scrcpy\n\
                             Windows: scoop install scrcpy".to_string()
                        );
                    }
                }
            }
            Action::OpenInEditor(text) => {
                if open_in_editor(text, package.as_deref(), "issues", "txt") {
                    self.pending_redraw = true;
                }
            }
            Action::Noop | Action::Unfocus => {}
        }
        false
    }
}

/// Writes `text` to a temp file and opens it in `$EDITOR` (or `$VISUAL`).
/// Returns `true` when a terminal editor was launched synchronously, so the
/// caller can force ratatui to redraw after the editor exits.
fn open_in_editor(text: String, package: Option<&str>, subdir: &str, ext: &str) -> bool {
    let package = package.unwrap_or("unknown");
    let dir = std::env::temp_dir().join("holo").join(package).join(subdir);
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = dir.join(format!("{timestamp}.{ext}"));
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

pub fn build_data(adb: &Arc<dyn Adb>, device: &Device, package: &str, app: &mut App) -> DataSources {
    let data = DataSources::new(adb.clone(), &device.serial, package, app.panel_visibility());
    app.set_layout_bounds(data.initial_layout_bounds);
    app.set_airplane_mode(data.initial_airplane_mode);
    app.set_wifi_enabled(data.initial_wifi_enabled);
    app.set_dark_mode(data.initial_dark_mode);
    app.set_show_taps(data.initial_show_taps);
    app.set_pointer_location(data.initial_pointer_location);
    app.set_gpu_rendering(data.initial_gpu_rendering);
    app.set_talkback(data.initial_talkback);
    app.set_measure_sdk_detected(data.has_measure_sdk);
    data
}

pub fn build_title(data: &DataSources) -> String {
    match &data.app_version {
        Some((name, code)) if !name.is_empty() => {
            format!(" {} / {} ", name, code)
        }
        _ => String::new(),
    }
}

pub fn try_auto_select_package(adb: &Arc<dyn Adb>, device: &Device, last_package: Option<&str>) -> (Vec<String>, Option<String>) {
    let packages = adb.list_packages(&device.serial).unwrap_or_default();
    let auto = last_package.and_then(|lp| {
        packages.iter().find(|p| p.as_str() == lp).cloned()
    });
    (packages, auto)
}
