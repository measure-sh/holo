mod adb;
mod app;
mod apps;
mod battery;
mod command_palette;
mod data;
mod database;
mod database_ui;
mod files;
mod files_ui;
mod logcat;
mod logcat_state;
mod monitor;
mod monitor_ui;
mod logcat_ui;
mod panel;
mod permissions;
mod permissions_ui;
mod processes;
mod selector;
mod theme;
mod toolbar;
mod trace;
mod ui;

use std::sync::{mpsc, Arc};
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};

use adb::{Adb, Device, RealAdb};
use app::{Action, App};
use data::DataSources;

fn spawn_app_action(
    adb: &Arc<dyn Adb>,
    serial: &str,
    package: &str,
    f: impl FnOnce(Arc<dyn Adb>, &str, &str) -> Result<()> + Send + 'static,
) {
    let adb = adb.clone();
    let serial = serial.to_string();
    let package = package.to_string();
    std::thread::spawn(move || {
        let _ = f(adb, &serial, &package);
    });
}

fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

fn spawn_fetch_devices(adb: &Arc<dyn Adb>) -> mpsc::Receiver<Vec<Device>> {
    let (tx, rx) = mpsc::channel();
    let adb = adb.clone();
    std::thread::spawn(move || {
        if let Ok(devices) = adb.list_devices() {
            let _ = tx.send(devices);
        }
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

fn build_data(adb: &Arc<dyn Adb>, device: &Device, package: &str, app: &mut App) -> DataSources {
    let data = DataSources::new(adb.clone(), &device.serial, package);
    app.set_layout_bounds(data.initial_layout_bounds);
    app.set_airplane_mode(data.initial_airplane_mode);
    app.set_wifi_enabled(data.initial_wifi_enabled);
    data
}

fn build_title(device: &Device, package: &str, data: &DataSources) -> String {
    match &data.app_version {
        Some((name, code)) if !name.is_empty() => {
            format!(" {} — {} ({} / {}) ", selector::selector_label(device), package, name, code)
        }
        _ => format!(" {} — {} ", selector::selector_label(device), package),
    }
}

fn try_auto_select_package(adb: &Arc<dyn Adb>, device: &Device, last_package: Option<&str>) -> (Vec<String>, Option<String>) {
    let packages = adb.list_packages(&device.serial).unwrap_or_default();
    let auto = last_package.and_then(|lp| {
        packages.iter().find(|p| p.as_str() == lp).cloned()
    });
    (packages, auto)
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb: Arc<dyn Adb> = Arc::new(RealAdb);
    let initial_device = adb.list_devices().ok()
        .and_then(|mut d| if !d.is_empty() { Some(d.swap_remove(0)) } else { None });

    let terminal = ratatui::init();
    let result = run_app(terminal, adb, initial_device);
    ratatui::restore();
    result
}

fn run_app(
    mut terminal: ratatui::DefaultTerminal,
    adb: Arc<dyn Adb>,
    initial_device: Option<Device>,
) -> Result<()> {
    let mut app = App::new(initial_device.clone(), None);
    let mut data: Option<DataSources> = None;
    let mut title = String::new();

    if let Some(device) = &initial_device {
        let (packages, auto) = try_auto_select_package(&adb, device, app.toolbar().last_package.as_deref());
        app.toolbar_mut().receive_packages(packages);
        if let Some(pkg) = auto {
            app.toolbar_mut().package = Some(pkg.clone());
            data = Some(build_data(&adb, device, &pkg, &mut app));
            title = build_title(device, &pkg, data.as_ref().unwrap());
        } else {
            app.toolbar_mut().open = Some(toolbar::DropdownKind::App);
        }
    }

    let mut devices_rx: Option<mpsc::Receiver<Vec<Device>>> = None;
    let mut packages_rx: Option<mpsc::Receiver<Vec<String>>> = None;

    loop {
        if let Some(rx) = &devices_rx {
            if let Ok(devices) = rx.try_recv() {
                app.toolbar_mut().receive_devices(devices);
                devices_rx = None;
            }
        }
        if let Some(rx) = &packages_rx {
            if let Ok(packages) = rx.try_recv() {
                if let Some(auto_pkg) = app.toolbar_mut().receive_packages(packages) {
                    app.toolbar_mut().package = Some(auto_pkg.clone());
                    if let Some(device) = app.toolbar().device.clone() {
                        data = Some(build_data(&adb, &device, &auto_pkg, &mut app));
                        title = build_title(&device, &auto_pkg, data.as_ref().unwrap());
                    }
                }
                packages_rx = None;
            }
        }

        let (serial, package) = {
            let tb = app.toolbar();
            (
                tb.device.as_ref().map(|d| d.serial.clone()),
                tb.package.clone(),
            )
        };

        if let Some(d) = &mut data {
            if let (Some(s), Some(p)) = (&serial, &package) {
                d.poll(&mut app, s, p);
            }
        }

        let now = chrono::Local::now();
        let time_str = format!(" {} ", now.format("%H:%M:%S"));
        let battery_level = data.as_ref().and_then(|d| d.battery_level);
        let logcat_lines: &[String] = data.as_ref().map_or(&[], |d| &d.logcat_lines);

        terminal.draw(|frame| {
            ui::render_app(frame, &title, &time_str, battery_level, &mut app, logcat_lines)
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.handle_key(key) {
                    Action::Quit => return Ok(()),
                    Action::FetchDevices => {
                        devices_rx = Some(spawn_fetch_devices(&adb));
                    }
                    Action::FetchApps => {
                        if let Some(s) = &serial {
                            packages_rx = Some(spawn_fetch_packages(&adb, s));
                        }
                    }
                    Action::ChangeDevice(d) => {
                        let last = app.toolbar().last_package.clone();
                        app.toolbar_mut().device = Some(d.clone());
                        app.toolbar_mut().package = None;
                        data = None;
                        title = String::new();
                        let (packages, auto) = try_auto_select_package(&adb, &d, last.as_deref());
                        app.toolbar_mut().receive_packages(packages);
                        if let Some(pkg) = auto {
                            app.toolbar_mut().package = Some(pkg.clone());
                            data = Some(build_data(&adb, &d, &pkg, &mut app));
                            title = build_title(&d, &pkg, data.as_ref().unwrap());
                        }
                    }
                    Action::ChangeApp(p) => {
                        app.toolbar_mut().package = Some(p.clone());
                        app.toolbar_mut().last_package = Some(p.clone());
                        toolbar::save_last_package(&p);
                        app.reset_for_new_app(&p);
                        if let Some(device) = app.toolbar().device.clone() {
                            data = Some(build_data(&adb, &device, &p, &mut app));
                            title = build_title(&device, &p, data.as_ref().unwrap());
                        }
                    }
                    Action::OpenApp => {
                        if let (Some(s), Some(p)) = (&serial, &package) {
                            spawn_app_action(&adb, s, p, |adb, s, p| adb.launch_app(s, p));
                        }
                    }
                    Action::KillApp => {
                        if let (Some(s), Some(p)) = (&serial, &package) {
                            spawn_app_action(&adb, s, p, |adb, s, p| adb.kill_app(s, p));
                        }
                    }
                    Action::ClearData => {
                        if let (Some(s), Some(p)) = (&serial, &package) {
                            spawn_app_action(&adb, s, p, |adb, s, p| adb.clear_app_data(s, p));
                        }
                    }
                    Action::UninstallApp => {
                        if let (Some(s), Some(p)) = (&serial, &package) {
                            spawn_app_action(&adb, s, p, |adb, s, p| adb.uninstall_app(s, p));
                        }
                    }
                    Action::WakeScreen => {
                        if let Some(s) = &serial {
                            spawn_app_action(&adb, s, package.as_deref().unwrap_or(""), |adb, s, _| adb.wake_screen(s));
                        }
                    }
                    Action::ToggleLayoutBounds => {
                        app.set_layout_bounds(!app.layout_bounds());
                        if let Some(s) = &serial {
                            let enabled = app.layout_bounds();
                            spawn_app_action(&adb, s, "", move |adb, s, _| adb.set_layout_bounds(s, enabled));
                        }
                    }
                    Action::ToggleAirplaneMode => {
                        app.set_airplane_mode(!app.airplane_mode());
                        if let Some(s) = &serial {
                            let enabled = app.airplane_mode();
                            spawn_app_action(&adb, s, "", move |adb, s, _| adb.set_airplane_mode(s, enabled));
                        }
                    }
                    Action::ToggleWifi => {
                        app.set_wifi_enabled(!app.wifi_enabled());
                        if let Some(s) = &serial {
                            let enabled = app.wifi_enabled();
                            spawn_app_action(&adb, s, "", move |adb, s, _| adb.set_wifi_enabled(s, enabled));
                        }
                    }
                    Action::WirelessAdb => {
                        if let Some(s) = &serial {
                            spawn_app_action(&adb, s, "", |adb, s, _| adb.enable_wireless_adb(s).map(|_| ()));
                        }
                    }
                    Action::Screenshot => {
                        if let (Some(s), Some(p)) = (&serial, &package) {
                            let adb = adb.clone();
                            let serial = s.clone();
                            let package = p.clone();
                            std::thread::spawn(move || {
                                let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                                let dest = std::env::temp_dir().join("msh")
                                    .join(&package)
                                    .join("screenshots")
                                    .join(format!("{timestamp}_screenshot.png"));
                                if adb.take_screenshot(&serial, &dest).is_ok() {
                                    let _ = open::that(&dest);
                                }
                            });
                        }
                    }
                    Action::TogglePermission(perm, granted) => {
                        if let (Some(s), Some(p)) = (&serial, &package) {
                            let adb = adb.clone();
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
                        if let Some(d) = &mut data {
                            d.logcat_lines.clear();
                        }
                    }
                    Action::ResetDb => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.restart_db_detection(adb.clone(), s.clone(), p.clone());
                        }
                    }
                    Action::CopyDbResult(text) => {
                        copy_to_clipboard(&text);
                    }
                    Action::CopyLogcat => {
                        if let Some(d) = &data {
                            let filter = &app.logcat_state().filter;
                            let text: String = d.logcat_lines
                                .iter()
                                .filter(|line| filter.matches(line))
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("\n");
                            copy_to_clipboard(&text);
                        }
                    }
                    Action::RunQuery(db, sql) => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_query(adb.clone(), s.clone(), p.clone(), db, sql);
                        }
                    }
                    Action::PullDb(db) => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_pull(adb.clone(), s.clone(), p.clone(), db);
                        }
                    }
                    Action::RefreshFiles => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_list_dir(adb.clone(), s.clone(), p.clone(), ".".to_string());
                        }
                    }
                    Action::ExpandDir(path) => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_list_dir(adb.clone(), s.clone(), p.clone(), path);
                        }
                    }
                    Action::PullFile(path) => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_pull_file(adb.clone(), s.clone(), p.clone(), path, false);
                        }
                    }
                    Action::OpenFile(path) => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_pull_file(adb.clone(), s.clone(), p.clone(), path, true);
                        }
                    }
                    Action::StartTrace => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.start_trace(adb.clone(), s.clone(), p.clone());
                        }
                    }
                    Action::StopTrace => {
                        if let (Some(d), Some(s), Some(p)) = (&mut data, &serial, &package) {
                            d.stop_and_pull_trace(adb.clone(), s.clone(), p.clone());
                        }
                    }
                    Action::None => {}
                }
            }
        }
    }
}
