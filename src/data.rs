use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crate::adb::Adb;
use crate::monitor::MonitorSample;
use crate::app::App;
use crate::battery;
use crate::database;
use crate::files;
use crate::logcat;
use crate::monitor;
use crate::permissions;
use crate::processes;
use crate::trace;

const MAX_LOGCAT_LINES: usize = 1000;

fn try_poll<T>(rx: &mut Option<mpsc::Receiver<T>>) -> Option<T> {
    let result = rx.as_ref()?.try_recv().ok()?;
    *rx = None;
    Some(result)
}

pub struct DataSources {
    battery_rx: mpsc::Receiver<u8>,
    pub battery_level: Option<u8>,

    procs_rx: mpsc::Receiver<HashMap<String, u32>>,
    process_map: Option<HashMap<String, u32>>,

    logcat_handle: Option<logcat::LogcatHandle>,
    pub logcat_lines: Vec<String>,
    pub monitored_pid: Option<u32>,

    db_detect_rx: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
    db_query_rx: Option<mpsc::Receiver<Result<String, String>>>,
    db_pull_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,

    permissions_rx: Option<mpsc::Receiver<Result<Vec<(String, bool)>, String>>>,

    files_list_rx: Option<mpsc::Receiver<Result<(String, Vec<(String, bool)>), String>>>,
    files_pull_rx: Option<mpsc::Receiver<Result<(String, bool), String>>>,

    monitor_rx: mpsc::Receiver<MonitorSample>,

    trace_start_rx: Option<mpsc::Receiver<Result<(), String>>>,
    trace_pull_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,

    pub initial_layout_bounds: bool,
    pub initial_airplane_mode: bool,
    pub initial_wifi_enabled: bool,
    pub initial_dark_mode: bool,
    pub app_version: Option<(String, String)>,

    connectivity_rx: mpsc::Receiver<bool>,
    pub device_connected: bool,
}

impl DataSources {
    pub fn new(adb: Arc<dyn Adb>, serial: &str, package: &str) -> Self {
        let initial_layout_bounds = adb.get_layout_bounds(serial).unwrap_or(false);
        let initial_airplane_mode = adb.get_airplane_mode(serial).unwrap_or(false);
        let initial_wifi_enabled = adb.get_wifi_enabled(serial).unwrap_or(false);
        let initial_dark_mode = adb.get_dark_mode(serial).unwrap_or(false);
        let app_version = adb.get_app_version(serial, package).ok();
        Self {
            battery_rx: battery::spawn_poller(adb.clone(), serial.to_string()),
            battery_level: None,
            procs_rx: processes::spawn_poller(adb.clone(), serial.to_string()),
            process_map: None,
            logcat_handle: None,
            logcat_lines: Vec::new(),
            monitored_pid: None,
            db_detect_rx: Some(database::spawn_db_detector(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            )),
            db_query_rx: None,
            db_pull_rx: None,
            permissions_rx: Some(permissions::spawn_permissions_loader(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            )),
            files_list_rx: Some(files::spawn_list_dir(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
                ".".to_string(),
            )),
            files_pull_rx: None,
            monitor_rx: monitor::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            ),
            trace_start_rx: None,
            trace_pull_rx: None,
            initial_layout_bounds,
            initial_airplane_mode,
            initial_wifi_enabled,
            initial_dark_mode,
            app_version,
            connectivity_rx: spawn_connectivity_poller(adb.clone(), serial.to_string()),
            device_connected: true,
        }
    }

    pub fn poll(&mut self, app: &mut App, serial: &str, package: &str) {
        while let Ok(connected) = self.connectivity_rx.try_recv() {
            self.device_connected = connected;
        }
        while let Ok(level) = self.battery_rx.try_recv() {
            self.battery_level = Some(level);
        }
        while let Ok(procs) = self.procs_rx.try_recv() {
            self.process_map = Some(procs);
        }
        while let Ok(info) = self.monitor_rx.try_recv() {
            app.monitor_state_mut().push(info);
        }

        let current_pid = self.process_map.as_ref().and_then(|m| m.get(package).copied());
        if current_pid != self.monitored_pid {
            self.logcat_handle = None;
            self.monitored_pid = None;
            if let Some(pid) = current_pid {
                self.logcat_handle = logcat::LogcatHandle::spawn(serial, pid);
                self.monitored_pid = Some(pid);
            }
        }

        if let Some(handle) = &self.logcat_handle {
            let prev_len = self.logcat_lines.len();
            while let Ok(line) = handle.rx().try_recv() {
                self.logcat_lines.push(line);
            }
            let new_count = self.logcat_lines.len() - prev_len;
            if new_count > 0 {
                app.logcat_state_mut().adjust_scroll_for_new_lines(new_count);
                if self.logcat_lines.len() > MAX_LOGCAT_LINES {
                    self.logcat_lines.drain(..self.logcat_lines.len() - MAX_LOGCAT_LINES);
                }
            }
        }

        if let Some(result) = try_poll(&mut self.permissions_rx) {
            match result {
                Ok(perms) => app.permissions_state_mut().permissions = perms,
                Err(e) => app.permissions_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.db_detect_rx) {
            match result {
                Ok(dbs) => app.database_state_mut().databases = dbs,
                Err(e) => app.database_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.db_query_rx) {
            match result {
                Ok(output) => app.database_state_mut().push_result(&output),
                Err(e) => app.database_state_mut().push_error(&e),
            }
        }
        if let Some(result) = try_poll(&mut self.db_pull_rx) {
            match result {
                Ok(path) => app.database_state_mut().push_result(&format!("pulled to {}", path.display())),
                Err(e) => app.database_state_mut().push_error(&format!("pull failed: {e}")),
            }
        }
        if let Some(result) = try_poll(&mut self.files_list_rx) {
            match result {
                Ok((path, entries)) => {
                    if path == "." {
                        app.files_state_mut().set_root_children(entries);
                    } else {
                        app.files_state_mut().set_children(&path, entries);
                    }
                }
                Err(e) => app.files_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.files_pull_rx) {
            match result {
                Ok((_, opened)) => {
                    let label = if opened { "opening..." } else { "pulling..." };
                    app.files_state_mut().action_flash =
                        Some((label, std::time::Instant::now()));
                }
                Err(e) => app.files_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.trace_start_rx) {
            if let Err(e) = result {
                let ts = app.trace_state_mut();
                ts.recording = false;
                ts.started_at = None;
                ts.status_message = Some(format!("failed: {e}"));
                ts.message_at = Some(std::time::Instant::now());
            }
        }
        if let Some(result) = try_poll(&mut self.trace_pull_rx) {
            let ts = app.trace_state_mut();
            match result {
                Ok(path) => {
                    ts.status_message = Some("done!".to_string());
                    ts.message_at = Some(std::time::Instant::now());
                    trace::open_in_perfetto_ui(&path);
                    ts.pulled_traces.push(path);
                }
                Err(e) => {
                    ts.status_message = Some(format!("failed: {e}"));
                    ts.message_at = Some(std::time::Instant::now());
                }
            }
        }
    }

    pub fn start_query(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, db: String, sql: String) {
        self.db_query_rx = Some(database::spawn_query(adb, serial, package, db, sql));
    }

    pub fn start_pull(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, db: String) {
        self.db_pull_rx = Some(database::spawn_pull_db(adb, serial, package, db));
    }

    pub fn restart_db_detection(&mut self, adb: Arc<dyn Adb>, serial: String, package: String) {
        self.db_detect_rx = Some(database::spawn_db_detector(adb, serial, package));
    }

    pub fn start_list_dir(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, path: String) {
        self.files_list_rx = Some(files::spawn_list_dir(adb, serial, package, path));
    }

    pub fn start_pull_file(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, path: String, open_after: bool) {
        self.files_pull_rx = Some(files::spawn_pull_file(adb, serial, package, path, open_after));
    }

    pub fn start_trace(&mut self, adb: Arc<dyn Adb>, serial: String, package: String) {
        self.trace_start_rx = Some(trace::spawn_start_trace(adb, serial, package));
    }

    pub fn stop_and_pull_trace(&mut self, adb: Arc<dyn Adb>, serial: String, package: String) {
        self.trace_pull_rx = Some(trace::spawn_stop_and_pull_trace(adb, serial, package));
    }
}

fn spawn_connectivity_poller(adb: Arc<dyn Adb>, serial: String) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(5);
        loop {
            let connected = adb
                .get_state(&serial)
                .map(|s| s == "device")
                .unwrap_or(false);
            if tx.send(connected).is_err() {
                return;
            }
            std::thread::sleep(interval);
        }
    });
    rx
}
