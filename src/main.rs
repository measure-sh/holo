mod adb;
mod app;
mod apps;
mod battery;
mod boot;
mod panel;
mod processes;
mod selector;
mod theme;
mod ui;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};

use adb::{Adb, Device, RealAdb};
use app::{Action, App};
use boot::BootResult;

fn resolve_filtered_package(packages: &Option<Vec<String>>, filter: &str, idx: usize) -> Option<String> {
    let pkgs = packages.as_ref()?;
    let filtered = apps::filtered_packages(pkgs, filter);
    filtered.get(idx).map(|s| s.to_string())
}

fn spawn_app_action(
    adb: &Arc<dyn Adb>,
    serial: &str,
    pkg: Option<String>,
    f: impl FnOnce(Arc<dyn Adb>, &str, &str) -> Result<()> + Send + 'static,
) {
    if let Some(pkg) = pkg {
        let adb = adb.clone();
        let serial = serial.to_string();
        std::thread::spawn(move || {
            let _ = f(adb, &serial, &pkg);
        });
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb = Arc::new(RealAdb);
    let devices = adb.list_devices()?;

    let device = match boot::resolve_device(devices) {
        BootResult::NoDevices => {
            eprintln!("No devices connected. Connect a device and try again.");
            std::process::exit(1);
        }
        BootResult::Selected(device) => device,
        BootResult::NeedsSelection(devices) => selector::run(devices)?,
    };

    let terminal = ratatui::init();
    let result = run_app(terminal, adb, &device);
    ratatui::restore();
    result
}

fn run_app(mut terminal: ratatui::DefaultTerminal, adb: Arc<dyn Adb>, device: &Device) -> Result<()> {
    let title = format!(" {} ", selector::selector_label(device));
    let mut app = App::new();

    let battery_rx = battery::spawn_poller(adb.clone(), device.serial.clone());
    let mut battery_level: Option<u8> = None;

    let procs_rx = processes::spawn_poller(adb.clone(), device.serial.clone());
    let mut process_map: Option<HashMap<String, u32>> = None;

    let apps_rx = apps::spawn_poller(adb.clone(), device.serial.clone());
    let mut packages: Option<Vec<String>> = None;

    loop {
        while let Ok(level) = battery_rx.try_recv() {
            battery_level = Some(level);
        }
        while let Ok(procs) = procs_rx.try_recv() {
            process_map = Some(procs);
        }
        while let Ok(pkgs) = apps_rx.try_recv() {
            packages = Some(pkgs);
        }

        let now = chrono::Local::now();
        let time_str = format!(" {} ", now.format("%H:%M:%S"));
        terminal.draw(|frame| {
            ui::render_app(frame, &title, &time_str, battery_level, &app, packages.as_deref(), process_map.as_ref())
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let filtered_count = packages.as_ref().map_or(0, |p| {
                    apps::filtered_packages(p, app.filter_text()).len()
                });
                match app.handle_key(key.code, filtered_count) {
                    Action::Quit => return Ok(()),
                    Action::OpenApp(idx) => {
                        let pkg = resolve_filtered_package(&packages, app.filter_text(), idx);
                        spawn_app_action(&adb, &device.serial, pkg, |adb, s, p| adb.launch_app(s, p));
                    }
                    Action::KillApp(idx) => {
                        let pkg = resolve_filtered_package(&packages, app.filter_text(), idx);
                        spawn_app_action(&adb, &device.serial, pkg, |adb, s, p| adb.kill_app(s, p));
                    }
                    Action::ClearData(idx) => {
                        let pkg = resolve_filtered_package(&packages, app.filter_text(), idx);
                        spawn_app_action(&adb, &device.serial, pkg, |adb, s, p| adb.clear_app_data(s, p));
                    }
                    Action::ClearDataAndOpen(idx) => {
                        let pkg = resolve_filtered_package(&packages, app.filter_text(), idx);
                        spawn_app_action(&adb, &device.serial, pkg, |adb, s, p| {
                            adb.clear_app_data(s, p).and_then(|_| adb.launch_app(s, p))
                        });
                    }
                    Action::None => {}
                }
            }
        }
    }
}
