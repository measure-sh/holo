mod adb;
mod app;
mod apps;
mod battery;
mod boot;
mod boot_ui;
mod database;
mod database_ui;
mod logcat;
mod logcat_state;
mod logcat_ui;
mod panel;
mod processes;
mod selector;
mod theme;
mod ui;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};

use adb::{Adb, Device, RealAdb};
use app::{Action, App};
use boot::{BootAction, BootPhase};

fn spawn_device_poller(adb: Arc<dyn Adb>) -> mpsc::Receiver<Vec<Device>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        if let Ok(devices) = adb.list_devices() {
            if tx.send(devices).is_err() {
                return;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    });
    rx
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

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb: Arc<dyn Adb> = Arc::new(RealAdb);
    let terminal = ratatui::init();
    let result = run_boot(terminal, adb);
    ratatui::restore();
    result
}

fn run_boot(mut terminal: ratatui::DefaultTerminal, adb: Arc<dyn Adb>) -> Result<()> {
    let mut phase = BootPhase::WaitingForDevice;
    let device_rx = spawn_device_poller(adb.clone());
    let mut apps_rx: Option<mpsc::Receiver<Vec<String>>> = None;
    let mut tick: u8 = 0;

    loop {
        while let Ok(devices) = device_rx.try_recv() {
            phase.receive_devices(devices);
        }

        if let BootPhase::SelectApp { device, .. } = &phase {
            if apps_rx.is_none() {
                apps_rx = Some(apps::spawn_poller(adb.clone(), device.serial.clone()));
            }
        }

        if let Some(rx) = &apps_rx {
            while let Ok(pkgs) = rx.try_recv() {
                phase.receive_packages(pkgs);
            }
        }

        if phase.is_done() {
            let (device, package) = phase.into_result().unwrap();
            return run_app(terminal, adb, &device, &package);
        }

        terminal.draw(|frame| boot_ui::render_boot(frame, &phase, tick))?;

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match phase.handle_key(key.code) {
                    BootAction::Quit => return Ok(()),
                    BootAction::Continue => {}
                }
            }
        }

        tick = tick.wrapping_add(1);
    }
}

fn run_app(
    mut terminal: ratatui::DefaultTerminal,
    adb: Arc<dyn Adb>,
    device: &Device,
    package: &str,
) -> Result<()> {
    let title = format!(" {} — {} ", selector::selector_label(device), package);
    let mut app = App::new();

    let battery_rx = battery::spawn_poller(adb.clone(), device.serial.clone());
    let mut battery_level: Option<u8> = None;

    let procs_rx = processes::spawn_poller(adb.clone(), device.serial.clone());
    let mut process_map: Option<HashMap<String, u32>> = None;

    let mut logcat_handle: Option<logcat::LogcatHandle> = None;
    let mut logcat_lines: Vec<String> = Vec::new();
    let mut monitored_pid: Option<u32> = None;
    const MAX_LOGCAT_LINES: usize = 1000;

    let mut db_detect_rx: Option<mpsc::Receiver<Result<Vec<String>, String>>> =
        Some(database::spawn_db_detector(adb.clone(), device.serial.clone(), package.to_string()));
    let mut db_query_rx: Option<mpsc::Receiver<Result<String, String>>> = None;
    let mut db_pull_rx: Option<mpsc::Receiver<Result<PathBuf, String>>> = None;

    loop {
        while let Ok(level) = battery_rx.try_recv() {
            battery_level = Some(level);
        }
        while let Ok(procs) = procs_rx.try_recv() {
            process_map = Some(procs);
        }

        let current_pid = process_map.as_ref().and_then(|m| m.get(package).copied());
        if current_pid != monitored_pid {
            logcat_handle = None;
            monitored_pid = None;
            if let Some(pid) = current_pid {
                logcat_handle = logcat::LogcatHandle::spawn(&device.serial, pid);
                monitored_pid = Some(pid);
            }
        }

        if let Some(handle) = &logcat_handle {
            let prev_len = logcat_lines.len();
            while let Ok(line) = handle.rx().try_recv() {
                logcat_lines.push(line);
            }
            let new_count = logcat_lines.len() - prev_len;
            if new_count > 0 {
                app.logcat_state_mut().adjust_scroll_for_new_lines(new_count);
                if logcat_lines.len() > MAX_LOGCAT_LINES {
                    logcat_lines.drain(..logcat_lines.len() - MAX_LOGCAT_LINES);
                }
            }
        }

        if let Some(rx) = &db_detect_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(dbs) => app.db_state_mut().databases = dbs,
                    Err(e) => app.db_state_mut().error = Some(e),
                }
                db_detect_rx = None;
            }
        }
        if let Some(rx) = &db_query_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(output) => app.db_state_mut().push_result(&output),
                    Err(e) => app.db_state_mut().push_error(&e),
                }
                db_query_rx = None;
            }
        }
        if let Some(rx) = &db_pull_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(path) => app.db_state_mut().push_result(&format!("pulled to {}", path.display())),
                    Err(e) => app.db_state_mut().push_error(&format!("pull failed: {e}")),
                }
                db_pull_rx = None;
            }
        }

        let now = chrono::Local::now();
        let time_str = format!(" {} ", now.format("%H:%M:%S"));
        terminal.draw(|frame| {
            ui::render_app(frame, &title, &time_str, battery_level, &mut app, &logcat_lines, monitored_pid)
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.handle_key(key.code) {
                    Action::Quit => return Ok(()),
                    Action::OpenApp => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, p| {
                            adb.launch_app(s, p)
                        });
                    }
                    Action::KillApp => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, p| {
                            adb.kill_app(s, p)
                        });
                    }
                    Action::ClearData => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, p| {
                            adb.clear_app_data(s, p)
                        });
                    }
                    Action::ResetLogcat => {
                        logcat_lines.clear();
                    }
                    Action::RunQuery(db, sql) => {
                        db_query_rx = Some(database::spawn_query(
                            adb.clone(),
                            device.serial.clone(),
                            package.to_string(),
                            db,
                            sql,
                        ));
                    }
                    Action::PullDb(db) => {
                        db_pull_rx = Some(database::spawn_pull_db(
                            adb.clone(),
                            device.serial.clone(),
                            package.to_string(),
                            db,
                        ));
                    }
                    Action::None => {}
                }
            }
        }
    }
}
