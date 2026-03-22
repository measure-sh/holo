use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crate::adb::Adb;
use crate::app::App;
use crate::battery;
use crate::database;
use crate::logcat;
use crate::permissions;
use crate::processes;

const MAX_LOGCAT_LINES: usize = 1000;

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

    pub initial_layout_bounds: bool,
    pub initial_airplane_mode: bool,
    pub app_version: Option<(String, String)>,
}

impl DataSources {
    pub fn new(adb: Arc<dyn Adb>, serial: &str, package: &str) -> Self {
        let initial_layout_bounds = adb.get_layout_bounds(serial).unwrap_or(false);
        let initial_airplane_mode = adb.get_airplane_mode(serial).unwrap_or(false);
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
            initial_layout_bounds,
            initial_airplane_mode,
            app_version,
        }
    }

    pub fn poll(&mut self, app: &mut App, serial: &str, package: &str) {
        while let Ok(level) = self.battery_rx.try_recv() {
            self.battery_level = Some(level);
        }
        while let Ok(procs) = self.procs_rx.try_recv() {
            self.process_map = Some(procs);
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

        if let Some(rx) = &self.permissions_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(perms) => app.permissions_state_mut().permissions = perms,
                    Err(e) => app.permissions_state_mut().error = Some(e),
                }
                self.permissions_rx = None;
            }
        }
        if let Some(rx) = &self.db_detect_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(dbs) => app.db_state_mut().databases = dbs,
                    Err(e) => app.db_state_mut().error = Some(e),
                }
                self.db_detect_rx = None;
            }
        }
        if let Some(rx) = &self.db_query_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(output) => app.db_state_mut().push_result(&output),
                    Err(e) => app.db_state_mut().push_error(&e),
                }
                self.db_query_rx = None;
            }
        }
        if let Some(rx) = &self.db_pull_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(path) => app.db_state_mut().push_result(&format!("pulled to {}", path.display())),
                    Err(e) => app.db_state_mut().push_error(&format!("pull failed: {e}")),
                }
                self.db_pull_rx = None;
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
}
