use std::sync::{mpsc, Arc};

use color_eyre::Result;

use crate::adb::{Adb, Device};
use crate::app::{Action, App};
use crate::data::DataSources;
use crate::toolbar;

pub struct DispatchContext {
    pub adb: Arc<dyn Adb>,
    pub data: Option<DataSources>,
    pub title: String,
    pub devices_rx: Option<mpsc::Receiver<Vec<Device>>>,
    pub packages_rx: Option<mpsc::Receiver<Vec<String>>>,
}

impl DispatchContext {
    pub fn poll_receivers(&mut self, app: &mut App) {
        if let Some(rx) = &self.devices_rx {
            if let Ok(devices) = rx.try_recv() {
                app.toolbar_mut().receive_devices(devices);
                self.devices_rx = None;
            }
        }
        if let Some(rx) = &self.packages_rx {
            if let Ok(packages) = rx.try_recv() {
                if let Some(auto_pkg) = app.toolbar_mut().receive_packages(packages) {
                    app.toolbar_mut().package = Some(auto_pkg.clone());
                    if let Some(device) = app.toolbar().device.clone() {
                        self.data = Some(build_data(&self.adb, &device, &auto_pkg, app));
                        self.title = build_title(self.data.as_ref().unwrap());
                    }
                }
                self.packages_rx = None;
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
                app.toolbar_mut().device = Some(d.clone());
                app.toolbar_mut().device_connected = true;
                app.toolbar_mut().package = None;
                self.data = None;
                self.title = String::new();
                let (packages, auto) = try_auto_select_package(&self.adb, &d, last.as_deref());
                app.toolbar_mut().receive_packages(packages);
                if let Some(pkg) = auto {
                    app.toolbar_mut().package = Some(pkg.clone());
                    self.data = Some(build_data(&self.adb, &d, &pkg, app));
                    self.title = build_title(self.data.as_ref().unwrap());
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
                    spawn_app_action(&self.adb, s, p, |adb, s, p| adb.launch_app(s, p));
                }
            }
            Action::KillApp => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    spawn_app_action(&self.adb, s, p, |adb, s, p| adb.kill_app(s, p));
                }
            }
            Action::ClearData => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    spawn_app_action(&self.adb, s, p, |adb, s, p| adb.clear_app_data(s, p));
                }
            }
            Action::UninstallApp => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    spawn_app_action(&self.adb, s, p, |adb, s, p| adb.uninstall_app(s, p));
                }
            }
            Action::WakeScreen => {
                if let Some(s) = &serial {
                    spawn_app_action(&self.adb, s, package.as_deref().unwrap_or(""), |adb, s, _| adb.wake_screen(s));
                }
            }
            Action::ToggleLayoutBounds => {
                app.set_layout_bounds(!app.layout_bounds());
                if let Some(s) = &serial {
                    let enabled = app.layout_bounds();
                    spawn_app_action(&self.adb, s, "", move |adb, s, _| adb.set_layout_bounds(s, enabled));
                }
            }
            Action::ToggleAirplaneMode => {
                app.set_airplane_mode(!app.airplane_mode());
                if let Some(s) = &serial {
                    let enabled = app.airplane_mode();
                    spawn_app_action(&self.adb, s, "", move |adb, s, _| adb.set_airplane_mode(s, enabled));
                }
            }
            Action::ToggleWifi => {
                app.set_wifi_enabled(!app.wifi_enabled());
                if let Some(s) = &serial {
                    let enabled = app.wifi_enabled();
                    spawn_app_action(&self.adb, s, "", move |adb, s, _| adb.set_wifi_enabled(s, enabled));
                }
            }
            Action::ToggleDarkMode => {
                app.set_dark_mode(!app.dark_mode());
                if let Some(s) = &serial {
                    let enabled = app.dark_mode();
                    spawn_app_action(&self.adb, s, "", move |adb, s, _| adb.set_dark_mode(s, enabled));
                }
            }
            Action::WirelessAdb => {
                if let Some(s) = &serial {
                    spawn_app_action(&self.adb, s, "", |adb, s, _| adb.enable_wireless_adb(s).map(|_| ()));
                }
            }
            Action::Screenshot => {
                if let (Some(s), Some(p)) = (&serial, &package) {
                    let adb = self.adb.clone();
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
            Action::ResetDb => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.restart_db_detection(self.adb.clone(), s.clone(), p.clone());
                }
            }
            Action::CopyDbResult(text) => {
                crate::clipboard::copy_to_clipboard(&text);
            }
            Action::CopyLogcat => {
                if let Some(d) = &self.data {
                    let filter = &app.logcat_state().filter;
                    let text: String = d.logcat_lines
                        .iter()
                        .filter(|line| filter.matches(line))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    crate::clipboard::copy_to_clipboard(&text);
                }
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
            Action::PullFile(path) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_pull_file(self.adb.clone(), s.clone(), p.clone(), path, false);
                }
            }
            Action::OpenFile(path) => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_pull_file(self.adb.clone(), s.clone(), p.clone(), path, true);
                }
            }
            Action::StartTrace => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.start_trace(self.adb.clone(), s.clone(), p.clone());
                }
            }
            Action::StopTrace => {
                if let (Some(d), Some(s), Some(p)) = (&mut self.data, &serial, &package) {
                    d.stop_and_pull_trace(self.adb.clone(), s.clone(), p.clone());
                }
            }
            Action::LaunchEmulator(name) => {
                let adb = self.adb.clone();
                std::thread::spawn(move || {
                    let _ = adb.launch_emulator(&name);
                });
            }
            Action::Noop | Action::Unfocus => {}
        }
        false
    }
}

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

fn spawn_fetch_devices(adb: &Arc<dyn Adb>) -> mpsc::Receiver<Vec<Device>> {
    let (tx, rx) = mpsc::channel();
    let adb = adb.clone();
    std::thread::spawn(move || {
        let mut devices = adb.list_devices().unwrap_or_default();

        let mut running_avds = std::collections::HashSet::new();
        for d in &devices {
            if d.serial.starts_with("emulator-") {
                if let Ok(name) = adb.get_avd_name(&d.serial) {
                    running_avds.insert(name);
                }
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

pub fn build_data(adb: &Arc<dyn Adb>, device: &Device, package: &str, app: &mut App) -> DataSources {
    let data = DataSources::new(adb.clone(), &device.serial, package);
    app.set_layout_bounds(data.initial_layout_bounds);
    app.set_airplane_mode(data.initial_airplane_mode);
    app.set_wifi_enabled(data.initial_wifi_enabled);
    app.set_dark_mode(data.initial_dark_mode);
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
