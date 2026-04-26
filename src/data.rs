use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc};

use crate::adb::Adb;
use crate::monitor::MonitorSample;
use crate::app::App;
use crate::battery;
use crate::database;
use crate::anrs;
use crate::crashes;
use crate::files;
use crate::logcat;
use crate::monitor;
use crate::network;
use crate::permissions;
use crate::processes;
use crate::trace;
use crate::vitals::{VitalsHandle, VitalsEvent};

const MAX_LOGCAT_LINES: usize = 1000;

fn try_poll<T>(rx: &mut Option<mpsc::Receiver<T>>) -> Option<T> {
    let result = rx.as_ref()?.try_recv().ok()?;
    *rx = None;
    Some(result)
}

pub struct DataSources {
    adb: Arc<dyn Adb>,
    battery_rx: mpsc::Receiver<u8>,
    pub battery_level: Option<u8>,

    procs_rx: mpsc::Receiver<Option<u32>>,
    last_polled_pid: Option<u32>,

    logcat_handle: Option<logcat::LogcatHandle>,
    pub logcat_lines: Vec<String>,
    pub monitored_pid: Option<u32>,

    vitals_handle: Option<VitalsHandle>,
    vitals_package: String,
    vitals_debuggable: bool,

    db_detect_rx: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
    db_query_rx: Option<mpsc::Receiver<Result<String, String>>>,
    db_pull_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    db_tables_rx: Option<mpsc::Receiver<database::TableListResult>>,
    db_table_data_rx: Option<mpsc::Receiver<database::TableDataResult>>,

    permissions_rx: mpsc::Receiver<Result<Vec<(String, bool)>, String>>,

    files_list_rx: Option<mpsc::Receiver<files::DirListResult>>,
    files_pull_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    files_stat_rx: Option<mpsc::Receiver<files::StatResult>>,
    files_cat_rx: Option<mpsc::Receiver<files::CatResult>>,
    pending_editor_open: Option<PathBuf>,

    monitor_rx: mpsc::Receiver<MonitorSample>,
    monitor_visibility: Arc<AtomicU8>,

    trace_start_rx: Option<mpsc::Receiver<Result<(), String>>>,
    trace_pull_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,

    crashes_rx: mpsc::Receiver<Result<Vec<crashes::CrashEntry>, String>>,
    anrs_rx: mpsc::Receiver<Result<Vec<anrs::AnrEntry>, String>>,

    pub initial_layout_bounds: bool,
    pub initial_airplane_mode: bool,
    pub initial_wifi_enabled: bool,
    pub initial_dark_mode: bool,
    pub initial_show_taps: bool,
    pub initial_pointer_location: bool,
    pub initial_gpu_rendering: bool,
    pub initial_talkback: bool,
    pub app_version: Option<(String, String)>,
    pub has_measure_sdk: bool,

    connectivity_rx: mpsc::Receiver<bool>,
    pub device_connected: bool,

    traffic_rx: Option<mpsc::Receiver<network::TrafficSample>>,
}

impl DataSources {
    pub fn new(adb: Arc<dyn Adb>, serial: &str, package: &str, panel_vis: &[bool; 8]) -> Self {
        let initial_layout_bounds = adb.get_layout_bounds(serial).unwrap_or(false);
        let initial_airplane_mode = adb.get_airplane_mode(serial).unwrap_or(false);
        let initial_wifi_enabled = adb.get_wifi_enabled(serial).unwrap_or(false);
        let initial_dark_mode = adb.get_dark_mode(serial).unwrap_or(false);
        let initial_show_taps = adb.get_show_taps(serial).unwrap_or(false);
        let initial_pointer_location = adb.get_pointer_location(serial).unwrap_or(false);
        let initial_gpu_rendering = adb.get_gpu_rendering(serial).unwrap_or(false);
        let initial_talkback = adb.get_talkback_enabled(serial).unwrap_or(false);
        let app_version = adb.get_app_version(serial, package).ok();
        let has_measure_sdk = adb.has_measure_sdk(serial, package);
        let monitor_visibility = Arc::new(AtomicU8::new(monitor::visibility_mask(panel_vis)));
        let vitals_debuggable = adb.is_debuggable(serial, package);
        let traffic_rx = Some(network::spawn_traffic_poller(
            adb.clone(),
            serial.to_string(),
            package.to_string(),
        ));
        Self {
            adb: adb.clone(),
            battery_rx: battery::spawn_poller(adb.clone(), serial.to_string()),
            battery_level: None,
            procs_rx: processes::spawn_poller(adb.clone(), serial.to_string(), package.to_string()),
            last_polled_pid: None,
            logcat_handle: None,
            logcat_lines: Vec::new(),
            monitored_pid: None,
            vitals_handle: None,
            vitals_package: package.to_string(),
            vitals_debuggable,
            db_detect_rx: Some(database::spawn_db_detector(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            )),
            db_query_rx: None,
            db_pull_rx: None,
            db_tables_rx: None,
            db_table_data_rx: None,
            permissions_rx: permissions::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            ),
            files_list_rx: Some(files::spawn_list_dir(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
                ".".to_string(),
            )),
            files_pull_rx: None,
            files_stat_rx: None,
            files_cat_rx: None,
            pending_editor_open: None,
            monitor_rx: monitor::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
                monitor_visibility.clone(),
            ),
            trace_start_rx: None,
            trace_pull_rx: None,
            crashes_rx: crashes::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            ),
            anrs_rx: anrs::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
            ),
            initial_layout_bounds,
            initial_airplane_mode,
            initial_wifi_enabled,
            initial_dark_mode,
            initial_show_taps,
            initial_pointer_location,
            initial_gpu_rendering,
            initial_talkback,
            app_version,
            has_measure_sdk,
            monitor_visibility,
            connectivity_rx: spawn_connectivity_poller(adb.clone(), serial.to_string()),
            device_connected: true,
            traffic_rx,
        }
    }

    pub fn update_monitor_visibility(&self, vis: &[bool; 8]) {
        self.monitor_visibility.store(monitor::visibility_mask(vis), Ordering::Relaxed);
    }

    pub fn poll(&mut self, app: &mut App, serial: &str) {
        while let Ok(connected) = self.connectivity_rx.try_recv() {
            self.device_connected = connected;
        }
        if let Some(rx) = &self.traffic_rx {
            while let Ok(sample) = rx.try_recv() {
                app.network_state_mut().push_traffic(sample);
            }
        }
        while let Ok(level) = self.battery_rx.try_recv() {
            self.battery_level = Some(level);
        }
        while let Ok(pid) = self.procs_rx.try_recv() {
            self.last_polled_pid = pid;
        }
        while let Ok(info) = self.monitor_rx.try_recv() {
            app.monitor_state_mut().push(info);
        }

        let current_pid = self.last_polled_pid;
        if current_pid != self.monitored_pid {
            self.logcat_handle = None;
            self.vitals_handle = None;
            self.monitored_pid = None;
            if let Some(pid) = current_pid {
                self.logcat_handle = logcat::LogcatHandle::spawn(serial, pid);
                self.monitored_pid = Some(pid);
                if self.vitals_debuggable {
                    self.vitals_handle = VitalsHandle::spawn(
                        self.adb.clone(),
                        serial.to_string(),
                        self.vitals_package.clone(),
                        pid,
                    )
                    .ok();
                }
            }
        }

        if let Some(handle) = &self.vitals_handle {
            while let Ok(event) = handle.rx.try_recv() {
                match event {
                    VitalsEvent::Gc { duration_us, .. } => {
                        app.monitor_state_mut().push_gc(duration_us);
                    }
                }
            }
        }

        if let Some(handle) = &self.logcat_handle {
            let prev_len = self.logcat_lines.len();
            while let Ok(line) = handle.rx().try_recv() {
                if let Some(entry) = network::parse_http_data(&line) {
                    app.network_state_mut().push(entry);
                }
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

        while let Ok(result) = self.permissions_rx.try_recv() {
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
        if let Some(result) = try_poll(&mut self.db_tables_rx) {
            match result {
                Ok((db, tables)) => app.database_state_mut().receive_tables(db, tables),
                Err(e) => {
                    let db = app.database_state_mut();
                    db.tables_loading.clear();
                    db.error = Some(e);
                }
            }
        }
        if let Some(result) = try_poll(&mut self.db_table_data_rx) {
            match result {
                Ok(data) => app.database_state_mut().receive_table_data(data),
                Err(e) => app.database_state_mut().receive_table_error(e),
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
                Ok(path) => {
                    app.files_state_mut().action_flash =
                        Some(("opening...", std::time::Instant::now()));
                    self.pending_editor_open = Some(path);
                }
                Err(e) => app.files_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.files_stat_rx) {
            match result {
                Ok((path, meta)) => app.files_state_mut().receive_meta(path, meta),
                Err(e) => app.files_state_mut().receive_detail_error(e),
            }
        }
        if let Some(result) = try_poll(&mut self.files_cat_rx) {
            match result {
                Ok((path, bytes)) => app.files_state_mut().receive_content(path, bytes),
                Err(e) => app.files_state_mut().receive_detail_error(e),
            }
        }
        if let Some(result) = try_poll(&mut self.trace_start_rx)
            && let Err(e) = result
        {
            let ts = app.trace_state_mut();
            ts.recording = false;
            ts.started_at = None;
            ts.status_message = Some(format!("failed: {e}"));
            ts.message_at = Some(std::time::Instant::now());
        }
        if let Some(result) = try_poll(&mut self.trace_pull_rx) {
            let ts = app.trace_state_mut();
            match result {
                Ok(path) => {
                    ts.status_message = Some("done!".to_string());
                    ts.message_at = Some(std::time::Instant::now());
                    ts.open_trace(&path);
                    ts.pulled_traces.push(path);
                }
                Err(e) => {
                    ts.status_message = Some(format!("failed: {e}"));
                    ts.message_at = Some(std::time::Instant::now());
                }
            }
        }
        while let Ok(result) = self.crashes_rx.try_recv() {
            match result {
                Ok(crashes) => app.issues_state_mut().set_crashes(crashes),
                Err(e) => app.issues_state_mut().set_crash_error(e),
            }
        }
        while let Ok(result) = self.anrs_rx.try_recv() {
            match result {
                Ok(anrs) => app.issues_state_mut().set_anrs(anrs),
                Err(e) => app.issues_state_mut().set_anr_error(e),
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

    pub fn start_fetch_tables(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, db: String) {
        self.db_tables_rx = Some(database::spawn_table_list(adb, serial, package, db));
    }

    pub fn start_fetch_table_data(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, db: String, table: String, kind: database::FetchKind) {
        self.db_table_data_rx = Some(database::spawn_table_data(adb, serial, package, db, table, kind));
    }

    pub fn start_list_dir(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, path: String) {
        self.files_list_rx = Some(files::spawn_list_dir(adb, serial, package, path));
    }

    pub fn start_pull_file(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, path: String) {
        self.files_pull_rx = Some(files::spawn_pull_file(adb, serial, package, path));
    }

    pub fn start_stat_file(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, path: String) {
        self.files_stat_rx = Some(files::spawn_stat_file(adb, serial, package, path));
    }

    pub fn start_cat_file(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, path: String) {
        self.files_cat_rx = Some(files::spawn_cat_file(adb, serial, package, path, files::MAX_DETAIL_BYTES));
    }

    pub fn files_cat_in_flight(&self) -> bool {
        self.files_cat_rx.is_some()
    }

    pub fn files_stat_in_flight(&self) -> bool {
        self.files_stat_rx.is_some()
    }

    pub fn take_pending_editor_open(&mut self) -> Option<PathBuf> {
        self.pending_editor_open.take()
    }

    pub fn start_trace(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, preset: trace::TracePreset) {
        self.trace_start_rx = Some(trace::spawn_start_trace(adb, serial, package, preset));
    }

    pub fn stop_and_pull_trace(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, preset: trace::TracePreset) {
        self.trace_pull_rx = Some(trace::spawn_stop_and_pull_trace(adb, serial, package, preset));
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
