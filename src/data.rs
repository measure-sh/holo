use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

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
use crate::session::{SessionSeed, SessionWriter};
use crate::trace;
use crate::vitals::{self, VitalsHandle, VitalsEvent};

const MAX_LOGCAT_LINES: usize = 1000;
/// Hold the existing session open across brief PID==None blips (USB jiggle,
/// adb hiccup). Only close after this much sustained "no PID."
const PID_NONE_DEBOUNCE: Duration = Duration::from_secs(5);

/// Inputs `SessionWriter::open` needs that are stable across PID transitions
/// within one (device, package) attach: device identity, app version, the
/// one-shot toggle probes. Captured in `DataSources::new` from the same calls
/// `apply_initial_state` consumes, and stamped into every session opened by
/// this `DataSources` until the user changes app or device.
struct SessionSeedTemplate {
    device_serial: String,
    device_model: Option<String>,
    package: String,
    app_version: Option<(String, String)>,
    has_measure_sdk: bool,
    debuggable: bool,
    initial_layout_bounds: bool,
    initial_airplane_mode: bool,
    initial_wifi_enabled: bool,
    initial_dark_mode: bool,
    initial_show_taps: bool,
    initial_pointer_location: bool,
    initial_gpu_rendering: bool,
    initial_talkback: bool,
}

impl SessionSeedTemplate {
    /// Borrow-and-clone — the template lives on `DataSources` and is
    /// reused for every session opened by this attach. Clippy's
    /// `wrong_self_convention` flagged the previous `into_seed(&self)`.
    fn to_seed(&self, pid: u32) -> SessionSeed {
        SessionSeed {
            device_serial: self.device_serial.clone(),
            device_model: self.device_model.clone(),
            package: self.package.clone(),
            pid,
            app_version: self.app_version.clone(),
            has_measure_sdk: self.has_measure_sdk,
            debuggable: self.debuggable,
            initial_layout_bounds: self.initial_layout_bounds,
            initial_airplane_mode: self.initial_airplane_mode,
            initial_wifi_enabled: self.initial_wifi_enabled,
            initial_dark_mode: self.initial_dark_mode,
            initial_show_taps: self.initial_show_taps,
            initial_pointer_location: self.initial_pointer_location,
            initial_gpu_rendering: self.initial_gpu_rendering,
            initial_talkback: self.initial_talkback,
        }
    }
}

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
    /// First instant we observed `procs_rx` returning `None` since the
    /// last `Some`. Used by the session debounce so a brief USB jiggle
    /// doesn't fragment one human run into multiple session dirs.
    pid_none_since: Option<Instant>,

    logcat_handle: Option<logcat::LogcatHandle>,
    pub logcat_lines: Vec<String>,
    pub monitored_pid: Option<u32>,

    vitals_handle: Option<VitalsHandle>,
    vitals_package: String,
    vitals_debuggable: bool,

    session_writer: Option<SessionWriter>,
    /// Snapshot of the inputs `SessionWriter::open` needs but `poll` can't
    /// re-derive: device serial / model and the one-shot probe results
    /// that are otherwise consumed by `apply_initial_state` and lost.
    session_seed_template: SessionSeedTemplate,
    /// Last `traces_dir` we pushed into `App.trace_state.pulled_traces`.
    /// Tracked so the trace panel auto-rescans whenever a new session
    /// opens (PID transition) without paying the disk cost on every poll.
    last_traces_dir_seen: Option<PathBuf>,

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

    /// Updated by `spawn_connectivity_poller` and read by every per-feature
    /// poller before issuing an adb call. Off-device when `false` keeps wedged
    /// transport calls from accumulating.
    connectivity: Arc<AtomicBool>,
    pub device_connected: bool,
}

impl DataSources {
    pub fn new(
        adb: Arc<dyn Adb>,
        serial: &str,
        device_model: Option<&str>,
        package: &str,
        panel_vis: &[bool; 8],
    ) -> Self {
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
        let session_seed_template = SessionSeedTemplate {
            device_serial: serial.to_string(),
            device_model: device_model.map(|s| s.to_string()),
            package: package.to_string(),
            app_version: app_version.clone(),
            has_measure_sdk,
            debuggable: vitals_debuggable,
            initial_layout_bounds,
            initial_airplane_mode,
            initial_wifi_enabled,
            initial_dark_mode,
            initial_show_taps,
            initial_pointer_location,
            initial_gpu_rendering,
            initial_talkback,
        };
        let connectivity = Arc::new(AtomicBool::new(true));
        spawn_connectivity_poller(adb.clone(), serial.to_string(), connectivity.clone());
        Self {
            adb: adb.clone(),
            battery_rx: battery::spawn_poller(
                adb.clone(),
                serial.to_string(),
                connectivity.clone(),
            ),
            battery_level: None,
            procs_rx: processes::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
                connectivity.clone(),
            ),
            last_polled_pid: None,
            pid_none_since: None,
            logcat_handle: None,
            logcat_lines: Vec::new(),
            monitored_pid: None,
            vitals_handle: None,
            vitals_package: package.to_string(),
            vitals_debuggable,
            session_writer: None,
            session_seed_template,
            last_traces_dir_seen: None,
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
                connectivity.clone(),
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
                connectivity.clone(),
            ),
            trace_start_rx: None,
            trace_pull_rx: None,
            crashes_rx: crashes::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
                connectivity.clone(),
            ),
            anrs_rx: anrs::spawn_poller(
                adb.clone(),
                serial.to_string(),
                package.to_string(),
                connectivity.clone(),
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
            connectivity,
            device_connected: true,
        }
    }

    pub fn update_monitor_visibility(&self, vis: &[bool; 8]) {
        self.monitor_visibility.store(monitor::visibility_mask(vis), Ordering::Relaxed);
    }

    pub fn poll(&mut self, app: &mut App, serial: &str) {
        // Capture is "always on": even while the user is viewing a captured
        // session, we keep draining channels and writing to the live
        // session file. The *App-state* pushes are gated — clobbering the
        // captured-session snapshot with live events would defeat the
        // purpose of opening it. Snapshot the flag once at the top so a
        // mid-poll set_viewing_session flip can't tear data flow.
        let viewing = app.is_viewing_session();

        self.device_connected = self.connectivity.load(Ordering::Relaxed);
        while let Ok(level) = self.battery_rx.try_recv() {
            self.battery_level = Some(level);
        }
        while let Ok(pid) = self.procs_rx.try_recv() {
            self.last_polled_pid = pid;
        }
        while let Ok(info) = self.monitor_rx.try_recv() {
            if let Some(w) = self.session_writer.as_mut() {
                w.write_disk_sample(&info);
            }
            if !viewing {
                app.monitor_state_mut().push(info);
            }
        }

        self.update_session_for_pid(serial);

        // After a PID transition opens (or closes) the writer, re-list the
        // session's `traces/` so the Trace panel reflects the right set of
        // captures. Cheap: one `read_dir` per session change, not per tick.
        if !viewing {
            let cur_dir = self.session_writer.as_ref().map(|w| w.traces_dir());
            if cur_dir != self.last_traces_dir_seen {
                let traces = cur_dir
                    .as_deref()
                    .map(crate::trace::list_perfetto_traces)
                    .unwrap_or_default();
                let ts = app.trace_state_mut();
                ts.pulled_traces = traces;
                ts.selected_index = 0;
                self.last_traces_dir_seen = cur_dir;
            }
        }

        if let Some(handle) = &self.vitals_handle {
            while let Ok(event) = handle.rx.try_recv() {
                if let Some(w) = self.session_writer.as_mut() {
                    let (kind, payload) = vitals::encode_event(&event);
                    w.write_vitals_frame(kind, &payload);
                }
                if viewing {
                    continue;
                }
                match event {
                    VitalsEvent::Gc { ts_ns, duration_us } => {
                        app.monitor_state_mut().push_gc(ts_ns, duration_us);
                    }
                    VitalsEvent::Memory { ts_ns, rss_kb, java_heap_kb, native_heap_kb } => {
                        app.monitor_state_mut()
                            .push_memory(ts_ns, rss_kb, java_heap_kb, native_heap_kb);
                    }
                    VitalsEvent::Cpu { ts_ns, cpu_centi_percent, num_threads } => {
                        let cpu_percent = cpu_centi_percent as f32 / 100.0;
                        app.monitor_state_mut().push_cpu(ts_ns, cpu_percent, num_threads);
                    }
                    VitalsEvent::Network { ts_ns, rx_bytes, tx_bytes } => {
                        app.monitor_state_mut().push_network(ts_ns, rx_bytes, tx_bytes);
                    }
                }
            }
        }

        if let Some(handle) = &self.logcat_handle {
            let prev_len = self.logcat_lines.len();
            while let Ok(line) = handle.rx().try_recv() {
                if let Some(w) = self.session_writer.as_mut() {
                    w.write_logcat_line(&line);
                }
                if !viewing
                    && let Some(entry) = network::parse_http_data(&line)
                {
                    app.network_state_mut().push(entry);
                }
                // Always grow the in-memory tail so logcat shown after the
                // user closes a captured-session view includes
                // what was captured while the user was looking at history.
                self.logcat_lines.push(line);
            }
            let new_count = self.logcat_lines.len() - prev_len;
            if new_count > 0 {
                if !viewing {
                    app.logcat_state_mut().adjust_scroll_for_new_lines(new_count);
                }
                if self.logcat_lines.len() > MAX_LOGCAT_LINES {
                    self.logcat_lines.drain(..self.logcat_lines.len() - MAX_LOGCAT_LINES);
                }
            }
        }

        while let Ok(result) = self.permissions_rx.try_recv() {
            if viewing { continue; }
            match result {
                Ok(perms) => app.permissions_state_mut().permissions = perms,
                Err(e) => app.permissions_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.db_detect_rx)
            && !viewing
        {
            match result {
                Ok(dbs) => app.database_state_mut().databases = dbs,
                Err(e) => app.database_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.db_query_rx)
            && !viewing
        {
            match result {
                Ok(output) => app.database_state_mut().push_result(&output),
                Err(e) => app.database_state_mut().push_error(&e),
            }
        }
        if let Some(result) = try_poll(&mut self.db_pull_rx)
            && !viewing
        {
            match result {
                Ok(path) => app.database_state_mut().push_result(&format!("pulled to {}", path.display())),
                Err(e) => app.database_state_mut().push_error(&format!("pull failed: {e}")),
            }
        }
        if let Some(result) = try_poll(&mut self.db_tables_rx)
            && !viewing
        {
            match result {
                Ok((db, tables)) => app.database_state_mut().receive_tables(db, tables),
                Err(e) => {
                    let db = app.database_state_mut();
                    db.tables_loading.clear();
                    db.error = Some(e);
                }
            }
        }
        if let Some(result) = try_poll(&mut self.db_table_data_rx)
            && !viewing
        {
            match result {
                Ok(data) => app.database_state_mut().receive_table_data(data),
                Err(e) => app.database_state_mut().receive_table_error(e),
            }
        }
        if let Some(result) = try_poll(&mut self.files_list_rx)
            && !viewing
        {
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
        if let Some(result) = try_poll(&mut self.files_pull_rx)
            && !viewing
        {
            match result {
                Ok(path) => {
                    app.files_state_mut().action_flash =
                        Some(("opening...", std::time::Instant::now()));
                    self.pending_editor_open = Some(path);
                }
                Err(e) => app.files_state_mut().error = Some(e),
            }
        }
        if let Some(result) = try_poll(&mut self.files_stat_rx)
            && !viewing
        {
            match result {
                Ok((path, meta)) => app.files_state_mut().receive_meta(path, meta),
                Err(e) => app.files_state_mut().receive_detail_error(e),
            }
        }
        if let Some(result) = try_poll(&mut self.files_cat_rx)
            && !viewing
        {
            match result {
                Ok((path, bytes)) => app.files_state_mut().receive_content(path, bytes),
                Err(e) => app.files_state_mut().receive_detail_error(e),
            }
        }
        if let Some(result) = try_poll(&mut self.trace_start_rx)
            && let Err(e) = result
            && !viewing
        {
            let ts = app.trace_state_mut();
            ts.recording = false;
            ts.started_at = None;
            ts.status_message = Some(format!("failed: {e}"));
            ts.message_at = Some(std::time::Instant::now());
        }
        if let Some(result) = try_poll(&mut self.trace_pull_rx)
            && !viewing
        {
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
                Ok(crashes) => {
                    if let Some(w) = self.session_writer.as_mut() {
                        for c in &crashes {
                            w.write_crash_if_new(c);
                        }
                    }
                    if !viewing {
                        app.issues_state_mut().set_crashes(crashes);
                    }
                }
                Err(e) => {
                    if !viewing {
                        app.issues_state_mut().set_crash_error(e);
                    }
                }
            }
        }
        while let Ok(result) = self.anrs_rx.try_recv() {
            match result {
                Ok(anrs) => {
                    if let Some(w) = self.session_writer.as_mut() {
                        for a in &anrs {
                            w.write_anr_if_new(a);
                        }
                    }
                    if !viewing {
                        app.issues_state_mut().set_anrs(anrs);
                    }
                }
                Err(e) => {
                    if !viewing {
                        app.issues_state_mut().set_anr_error(e);
                    }
                }
            }
        }

        if let Some(w) = self.session_writer.as_mut() {
            w.flush_periodic();
            if let Some(err) = w.take_error() {
                app.set_status_flash(format!("session write failed: {err}"), true);
            }
        }
    }

    /// Open / close the `SessionWriter` to track the live PID. PID==None is
    /// debounced — a brief disconnect (USB jiggle, adb hiccup) leaves the
    /// existing writer open so one human run doesn't fragment into N session
    /// dirs. Same-PID transitions are no-ops; different-PID transitions
    /// finalize the old session and open a new one.
    fn update_session_for_pid(&mut self, _serial: &str) {
        let current_pid = self.last_polled_pid;
        match (self.monitored_pid, current_pid) {
            (Some(a), Some(b)) if a == b => {
                self.pid_none_since = None;
            }
            (None, Some(pid)) => {
                self.start_pid(pid);
                self.pid_none_since = None;
            }
            (Some(_), Some(pid)) => {
                // Different PID — close old session immediately and start fresh.
                self.stop_pid();
                self.start_pid(pid);
                self.pid_none_since = None;
            }
            (Some(_), None) => {
                let now = Instant::now();
                let since = self.pid_none_since.get_or_insert(now);
                if now.duration_since(*since) >= PID_NONE_DEBOUNCE {
                    self.stop_pid();
                    self.pid_none_since = None;
                }
            }
            (None, None) => {
                self.pid_none_since = None;
            }
        }
    }

    fn start_pid(&mut self, pid: u32) {
        let serial = self.session_seed_template.device_serial.clone();
        self.logcat_handle = logcat::LogcatHandle::spawn(&serial, pid);
        self.monitored_pid = Some(pid);
        if self.vitals_debuggable {
            self.vitals_handle = VitalsHandle::spawn(
                self.adb.clone(),
                serial,
                self.vitals_package.clone(),
                pid,
            )
            .ok();
        }
        self.session_writer = SessionWriter::open(self.session_seed_template.to_seed(pid));
    }

    fn stop_pid(&mut self) {
        self.logcat_handle = None;
        self.vitals_handle = None;
        self.monitored_pid = None;
        // Drop finalizes metadata.json with `ended_at`.
        self.session_writer = None;
    }

    /// Identifies the session currently being written to, if any. Used by
    /// the history dialog to render the `(live)` badge.
    pub fn active_session_id(&self) -> Option<&crate::session::SessionId> {
        self.session_writer.as_ref().map(|w| w.id())
    }

    pub fn start_query(&mut self, adb: Arc<dyn Adb>, serial: String, package: String, db: String, sql: String) {
        self.db_query_rx = Some(database::spawn_query(adb, serial, package, db, sql));
    }

    pub fn start_pull(
        &mut self,
        adb: Arc<dyn Adb>,
        serial: String,
        package: String,
        db: String,
        dest_dir: PathBuf,
    ) {
        self.db_pull_rx = Some(database::spawn_pull_db(adb, serial, package, db, dest_dir));
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

    pub fn start_pull_file(
        &mut self,
        adb: Arc<dyn Adb>,
        serial: String,
        package: String,
        path: String,
        dest_dir: PathBuf,
    ) {
        self.files_pull_rx = Some(files::spawn_pull_file(adb, serial, package, path, dest_dir));
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

    pub fn stop_and_pull_trace(
        &mut self,
        adb: Arc<dyn Adb>,
        serial: String,
        _package: String,
        preset: trace::TracePreset,
        dest_dir: PathBuf,
    ) {
        self.trace_pull_rx = Some(trace::spawn_stop_and_pull_trace(adb, serial, dest_dir, preset));
    }
}

fn spawn_connectivity_poller(adb: Arc<dyn Adb>, serial: String, flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(5);
        // Drop the worker once nothing else is reading the flag — otherwise we
        // keep probing a stale device after the owning DataSources is gone.
        loop {
            if Arc::strong_count(&flag) == 1 {
                return;
            }
            let connected = adb
                .get_state(&serial)
                .map(|s| s == "device")
                .unwrap_or(false);
            flag.store(connected, Ordering::Relaxed);
            std::thread::sleep(interval);
        }
    });
}
