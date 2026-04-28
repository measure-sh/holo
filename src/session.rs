use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::anrs::AnrEntry;
use crate::crashes::CrashEntry;
use crate::monitor::MonitorSample;

/// Bumped whenever the on-disk layout or any per-stream encoding changes in a
/// way that older readers would mis-decode. `replay::load` rejects anything
/// it doesn't recognize, so an old holo never tries to render a session
/// written by a newer one.
pub const SCHEMA_VERSION: u32 = 1;

const META_FILE: &str = "metadata.json";
const LOGCAT_FILE: &str = "logcat.log";
const VITALS_FILE: &str = "vitals.bin";
const DISK_FILE: &str = "monitor_disk.jsonl";
const ISSUES_FILE: &str = "issues.jsonl";
const TRACES_DIR: &str = "traces";

/// Identifies a session uniquely. The on-disk path is
/// `sessions_root()/<package>/<dir_name>/`. Device serial isn't part of the
/// path — sessions roll up per-app across devices, which matches how users
/// actually look at history (the same app on an emulator vs a physical
/// device is conceptually the same app). The captured device serial lives
/// in `metadata.json` if a row ever needs to surface it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId {
    pub package: String,
    pub dir_name: String,
}

impl SessionId {
    pub fn dir(&self) -> PathBuf {
        sessions_root().join(&self.package).join(&self.dir_name)
    }
}

/// Persisted in `metadata.json`. The probe fields (initial_*, has_measure_sdk,
/// app_version) are captured once at session open from `DataSources::new` so
/// replay can faithfully reconstruct `apply_initial_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub schema_version: u32,
    pub holo_version: String,
    pub device_serial: String,
    pub device_model: Option<String>,
    pub package: String,
    pub pid: u32,
    pub host_pid: u32,
    pub started_at: DateTime<Local>,
    pub ended_at: Option<DateTime<Local>>,
    pub app_version: Option<(String, String)>,
    pub has_measure_sdk: bool,
    /// `true` when the captured app was `android:debuggable` and the JVMTI
    /// agent was attachable. Replay reads this to seed `MonitorState` so a
    /// non-debuggable session doesn't claim "agent attached" in the UI.
    /// `#[serde(default)]` keeps older sessions readable; they default to
    /// `false` (conservative — show "no agent" rather than fake one).
    #[serde(default)]
    pub debuggable: bool,
    pub initial_layout_bounds: bool,
    pub initial_airplane_mode: bool,
    pub initial_wifi_enabled: bool,
    pub initial_dark_mode: bool,
    pub initial_show_taps: bool,
    pub initial_pointer_location: bool,
    pub initial_gpu_rendering: bool,
    pub initial_talkback: bool,
}

/// Inputs the writer can't infer on its own — captured at the `DataSources`
/// boundary because that's where the one-shot probes live.
#[derive(Debug, Clone)]
pub struct SessionSeed {
    pub device_serial: String,
    pub device_model: Option<String>,
    pub package: String,
    pub pid: u32,
    pub app_version: Option<(String, String)>,
    pub has_measure_sdk: bool,
    pub debuggable: bool,
    pub initial_layout_bounds: bool,
    pub initial_airplane_mode: bool,
    pub initial_wifi_enabled: bool,
    pub initial_dark_mode: bool,
    pub initial_show_taps: bool,
    pub initial_pointer_location: bool,
    pub initial_gpu_rendering: bool,
    pub initial_talkback: bool,
}

/// Listed in the history dialog. Only fields needed to render the row plus
/// `id` for view invocation.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: SessionId,
    pub started_at: DateTime<Local>,
    /// Read from `metadata.json`. Unused by the row renderer (which only
    /// shows `started_at` + `duration`), but kept around so future affordances
    /// like `(crashed)` badges can branch on `ended_at.is_none()` without a
    /// metadata re-read.
    #[allow(dead_code)]
    pub ended_at: Option<DateTime<Local>>,
    pub duration: Option<Duration>,
    pub size_bytes: u64,
    pub pid: u32,
    /// Device the session was captured against. Sessions roll up per-app
    /// across devices, so the dialog shows this column to disambiguate.
    pub device_serial: String,
    /// Device model from the original `adb` listing, when available. Some
    /// devices (notably emulators) don't report a model — we fall back to
    /// the serial in the dialog.
    pub device_model: Option<String>,
}

/// Issue dedup key — `(timestamp, description)` is unique enough across the
/// dropbox snapshots we see in practice without hashing the whole body.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct IssueKey {
    kind: &'static str,
    timestamp: String,
    description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersistedIssue {
    pub kind: String,
    pub timestamp: String,
    pub description: String,
    pub full_text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersistedDiskSample {
    pub data_kb: u64,
    pub debuggable: bool,
}

impl From<&MonitorSample> for PersistedDiskSample {
    fn from(s: &MonitorSample) -> Self {
        Self { data_kb: s.data_kb, debuggable: s.debuggable }
    }
}

/// Owns the session directory and its append-mode file handles. Drops finalize
/// metadata. Any write error trips `disabled` so subsequent writes silently
/// no-op until the writer is replaced.
pub struct SessionWriter {
    id: SessionId,
    meta: SessionMeta,
    logcat: Option<BufWriter<File>>,
    vitals: Option<BufWriter<File>>,
    disk: Option<BufWriter<File>>,
    issues: Option<BufWriter<File>>,
    seen_issues: HashSet<IssueKey>,
    disabled: bool,
    last_error: Option<String>,
}

impl SessionWriter {
    /// Open a fresh session directory. Returns `None` if the cache root is
    /// unavailable (no `dirs::cache_dir()`); the caller continues without a
    /// writer but the live UI keeps working.
    pub fn open(seed: SessionSeed) -> Option<Self> {
        let started_at = Local::now();
        let dir_name = format!(
            "{}_pid{}_h{}",
            started_at.format("%Y%m%d_%H%M%S"),
            seed.pid,
            std::process::id(),
        );
        let id = SessionId {
            package: seed.package.clone(),
            dir_name,
        };
        let path = id.dir();
        if let Err(e) = fs::create_dir_all(&path) {
            eprintln!("session: create_dir_all {} failed: {e}", path.display());
            return None;
        }
        if let Err(e) = fs::create_dir_all(path.join(TRACES_DIR)) {
            eprintln!("session: create_dir_all traces failed: {e}");
        }

        let meta = SessionMeta {
            schema_version: SCHEMA_VERSION,
            holo_version: env!("CARGO_PKG_VERSION").to_string(),
            device_serial: seed.device_serial,
            device_model: seed.device_model,
            package: seed.package,
            pid: seed.pid,
            host_pid: std::process::id(),
            started_at,
            ended_at: None,
            app_version: seed.app_version,
            has_measure_sdk: seed.has_measure_sdk,
            debuggable: seed.debuggable,
            initial_layout_bounds: seed.initial_layout_bounds,
            initial_airplane_mode: seed.initial_airplane_mode,
            initial_wifi_enabled: seed.initial_wifi_enabled,
            initial_dark_mode: seed.initial_dark_mode,
            initial_show_taps: seed.initial_show_taps,
            initial_pointer_location: seed.initial_pointer_location,
            initial_gpu_rendering: seed.initial_gpu_rendering,
            initial_talkback: seed.initial_talkback,
        };

        let mut writer = Self {
            id,
            meta,
            logcat: None,
            vitals: None,
            disk: None,
            issues: None,
            seen_issues: HashSet::new(),
            disabled: false,
            last_error: None,
        };

        if let Err(e) = writer.write_meta() {
            eprintln!("session: writing initial metadata failed: {e}");
            return None;
        }
        writer.logcat = open_append(&path.join(LOGCAT_FILE));
        writer.vitals = open_append(&path.join(VITALS_FILE));
        writer.disk = open_append(&path.join(DISK_FILE));
        writer.issues = open_append(&path.join(ISSUES_FILE));
        Some(writer)
    }

    pub fn id(&self) -> &SessionId {
        &self.id
    }

    pub fn traces_dir(&self) -> PathBuf {
        self.id.dir().join(TRACES_DIR)
    }

    pub fn write_logcat_line(&mut self, line: &str) {
        if self.disabled { return; }
        let Some(w) = self.logcat.as_mut() else { return; };
        if let Err(e) = writeln!(w, "{line}") {
            self.fail(format!("logcat: {e}"));
        }
    }

    pub fn write_vitals_frame(&mut self, kind: u8, payload: &[u8]) {
        if self.disabled { return; }
        let Some(w) = self.vitals.as_mut() else { return; };
        let len = payload.len() as u32;
        let header = [kind, (len >> 24) as u8, (len >> 16) as u8, (len >> 8) as u8, len as u8];
        if let Err(e) = w.write_all(&header).and_then(|_| w.write_all(payload)) {
            self.fail(format!("vitals: {e}"));
        }
    }

    pub fn write_disk_sample(&mut self, sample: &MonitorSample) {
        if self.disabled { return; }
        let Some(w) = self.disk.as_mut() else { return; };
        let persisted = PersistedDiskSample::from(sample);
        match serde_json::to_string(&persisted) {
            Ok(json) => {
                if let Err(e) = writeln!(w, "{json}") {
                    self.fail(format!("disk: {e}"));
                }
            }
            Err(e) => self.fail(format!("disk encode: {e}")),
        }
    }

    pub fn write_crash_if_new(&mut self, crash: &CrashEntry) {
        let key = IssueKey {
            kind: "crash",
            timestamp: crash.timestamp.clone(),
            description: crash.exception.clone(),
        };
        if !self.seen_issues.insert(key) {
            return;
        }
        self.write_issue(PersistedIssue {
            kind: "crash".into(),
            timestamp: crash.timestamp.clone(),
            description: crash.exception.clone(),
            full_text: crash.full_text.clone(),
        });
    }

    pub fn write_anr_if_new(&mut self, anr: &AnrEntry) {
        let key = IssueKey {
            kind: "anr",
            timestamp: anr.timestamp.clone(),
            description: anr.reason.clone(),
        };
        if !self.seen_issues.insert(key) {
            return;
        }
        self.write_issue(PersistedIssue {
            kind: "anr".into(),
            timestamp: anr.timestamp.clone(),
            description: anr.reason.clone(),
            full_text: anr.full_text.clone(),
        });
    }

    fn write_issue(&mut self, issue: PersistedIssue) {
        if self.disabled { return; }
        let Some(w) = self.issues.as_mut() else { return; };
        match serde_json::to_string(&issue) {
            Ok(json) => {
                if let Err(e) = writeln!(w, "{json}") {
                    self.fail(format!("issues: {e}"));
                }
            }
            Err(e) => self.fail(format!("issues encode: {e}")),
        }
    }

    /// Cheap — flushes BufWriter buffers but doesn't fsync. The next housekeeping
    /// tick will pick up failures via `take_error`.
    pub fn flush_periodic(&mut self) {
        if self.disabled { return; }
        if let Some(w) = self.logcat.as_mut() { let _ = w.flush(); }
        if let Some(w) = self.vitals.as_mut() { let _ = w.flush(); }
        if let Some(w) = self.disk.as_mut() { let _ = w.flush(); }
        if let Some(w) = self.issues.as_mut() { let _ = w.flush(); }
    }

    /// Pop a queued error so the UI can flash it once. Cleared after read.
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    fn fail(&mut self, e: String) {
        self.disabled = true;
        self.last_error = Some(e);
        self.logcat = None;
        self.vitals = None;
        self.disk = None;
        self.issues = None;
    }

    fn write_meta(&self) -> std::io::Result<()> {
        let path = self.id.dir().join(META_FILE);
        let json = serde_json::to_string_pretty(&self.meta).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }
}

impl Drop for SessionWriter {
    fn drop(&mut self) {
        // Flush all buffered streams before stamping ended_at; otherwise a
        // reader might see metadata claim "ended" while the last few KB of
        // logcat are still in the BufWriter's buffer.
        if let Some(mut w) = self.logcat.take() { let _ = w.flush(); }
        if let Some(mut w) = self.vitals.take() { let _ = w.flush(); }
        if let Some(mut w) = self.disk.take() { let _ = w.flush(); }
        if let Some(mut w) = self.issues.take() { let _ = w.flush(); }
        self.meta.ended_at = Some(Local::now());
        let _ = self.write_meta();
    }
}

fn open_append(path: &Path) -> Option<BufWriter<File>> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(BufWriter::new)
        .map_err(|e| eprintln!("session: open {} failed: {e}", path.display()))
        .ok()
}

/// Single source of truth for the holo cache root. Falls back to the OS
/// temp dir when `dirs::cache_dir()` is unset (no `$HOME` etc.) — the temp
/// dir is at least writable, whereas `cwd` (the previous fallback in
/// `toolbar.rs`) might not be. Both `sessions_root` and the toolbar's
/// `last_package` cache path build on top of this.
pub fn cache_root() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("holo")
}

pub fn sessions_root() -> PathBuf {
    cache_root().join("sessions")
}

/// Sessions for `package` across every device the user has captured against,
/// newest-first. Sessions whose `metadata.json` is missing or unreadable are
/// skipped silently — corrupted dirs shouldn't block the dialog from
/// rendering the rest.
pub fn list_sessions(package: &str) -> Vec<SessionSummary> {
    let dir = sessions_root().join(package);
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };
        let id = SessionId {
            package: package.to_string(),
            dir_name,
        };
        let meta_path = path.join(META_FILE);
        let Ok(meta_str) = fs::read_to_string(&meta_path) else { continue };
        let Ok(meta) = serde_json::from_str::<SessionMeta>(&meta_str) else { continue };
        let duration = meta.ended_at.map(|end| {
            let d = end.signed_duration_since(meta.started_at);
            Duration::from_secs(d.num_seconds().max(0) as u64)
        });
        let size_bytes = dir_size(&path);
        out.push(SessionSummary {
            id,
            started_at: meta.started_at,
            ended_at: meta.ended_at,
            duration,
            size_bytes,
            pid: meta.pid,
            device_serial: meta.device_serial,
            device_model: meta.device_model,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.started_at));
    out
}

pub fn read_meta(id: &SessionId) -> Option<SessionMeta> {
    let path = id.dir().join(META_FILE);
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn delete_session(id: &SessionId) -> std::io::Result<()> {
    fs::remove_dir_all(id.dir())
}

/// Walk every `(device, package, session)` triple in the sessions root and
/// remove anything whose `started_at` (or directory mtime when metadata is
/// unreadable) is older than `days`. Best-effort: errors are logged.
pub fn prune_older_than(days: u32) {
    let root = sessions_root();
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(days as u64 * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let Ok(packages) = fs::read_dir(&root) else { return; };
    for pkg in packages.flatten() {
        let Ok(sessions) = fs::read_dir(pkg.path()) else { continue; };
        for session in sessions.flatten() {
            let path = session.path();
            if !path.is_dir() { continue; }
            let too_old = match fs::read_to_string(path.join(META_FILE))
                .ok()
                .and_then(|s| serde_json::from_str::<SessionMeta>(&s).ok())
            {
                Some(m) => {
                    let secs = m.started_at.timestamp().max(0) as u64;
                    let started = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
                    started < cutoff
                }
                None => fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .map(|mt| mt < cutoff)
                    .unwrap_or(false),
            };
            if too_old {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(path) else { return 0; };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                total = total.saturating_add(dir_size(&p));
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// Shared lock for tests that pin `HOME` / `XDG_CACHE_HOME`. Process-global
/// env vars race across parallel tests; any test in the workspace that
/// touches the cache root must take this lock for the duration.
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Convenience for replay: stream issues back from disk in the order they
/// were appended.
pub fn read_issues(id: &SessionId) -> Vec<PersistedIssue> {
    let path = id.dir().join(ISSUES_FILE);
    let Ok(file) = File::open(path) else { return Vec::new(); };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

/// Convenience for replay: stream disk samples back from disk in order.
pub fn read_disk_samples(id: &SessionId) -> Vec<PersistedDiskSample> {
    let path = id.dir().join(DISK_FILE);
    let Ok(file) = File::open(path) else { return Vec::new(); };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_with_root(root: &Path, device: &str, package: &str, pid: u32) -> SessionSeed {
        // Tests pin the sessions root by setting HOME so dirs::cache_dir lands
        // inside our tempdir. The seed just provides the rest of the inputs.
        let _ = root;
        SessionSeed {
            device_serial: device.into(),
            device_model: Some("Pixel".into()),
            package: package.into(),
            pid,
            app_version: Some(("1.0".into(), "100".into())),
            has_measure_sdk: true,
            debuggable: true,
            initial_layout_bounds: false,
            initial_airplane_mode: false,
            initial_wifi_enabled: true,
            initial_dark_mode: false,
            initial_show_taps: false,
            initial_pointer_location: false,
            initial_gpu_rendering: false,
            initial_talkback: false,
        }
    }

    fn pin_cache_dir(tmp: &Path) {
        // dirs::cache_dir on macOS reads HOME. On Linux it reads XDG_CACHE_HOME
        // first, then HOME. Set both so tests are deterministic on either OS.
        unsafe {
            std::env::set_var("HOME", tmp);
            std::env::set_var("XDG_CACHE_HOME", tmp.join(".cache"));
        }
    }

    #[test]
    fn round_trip_session_writes_and_lists() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("holo-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        pin_cache_dir(&tmp);

        let seed = seed_with_root(&tmp, "emu-5554", "com.example.app", 1234);
        let mut w = SessionWriter::open(seed).expect("writer opens");
        let id = w.id().clone();
        w.write_logcat_line("hello world");
        w.write_logcat_line("second line");
        w.write_vitals_frame(0x01, &[0u8; 12]);
        w.write_disk_sample(&MonitorSample { data_kb: 42, debuggable: true });
        w.write_crash_if_new(&CrashEntry {
            timestamp: "2026-04-28 10:00:00".into(),
            exception: "NullPointerException".into(),
            full_text: "stack...".into(),
        });
        // Dedup: same key should not re-append.
        w.write_crash_if_new(&CrashEntry {
            timestamp: "2026-04-28 10:00:00".into(),
            exception: "NullPointerException".into(),
            full_text: "stack but ignored".into(),
        });
        w.write_anr_if_new(&AnrEntry {
            timestamp: "2026-04-28 10:01:00".into(),
            reason: "Input dispatch timeout".into(),
            full_text: "anr trace".into(),
        });
        drop(w); // finalize

        let summaries = list_sessions("com.example.app");
        assert_eq!(summaries.len(), 1);
        let s = &summaries[0];
        assert_eq!(s.pid, 1234);
        assert!(s.size_bytes > 0);
        assert!(s.ended_at.is_some());

        let issues = read_issues(&id);
        assert_eq!(issues.len(), 2, "dedup should produce exactly one crash + one anr");
        assert_eq!(issues[0].kind, "crash");
        assert_eq!(issues[1].kind, "anr");

        let samples = read_disk_samples(&id);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].data_kb, 42);

        let logcat = fs::read_to_string(id.dir().join(LOGCAT_FILE)).unwrap();
        assert_eq!(logcat, "hello world\nsecond line\n");

        let vitals = fs::read(id.dir().join(VITALS_FILE)).unwrap();
        assert_eq!(vitals.len(), 5 + 12, "header + payload");
        assert_eq!(vitals[0], 0x01);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn meta_round_trips_through_serde() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("holo-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        pin_cache_dir(&tmp);

        let seed = seed_with_root(&tmp, "emu-5556", "com.example.meta", 99);
        let w = SessionWriter::open(seed).unwrap();
        let id = w.id().clone();
        drop(w);

        let m = read_meta(&id).expect("metadata readable");
        assert_eq!(m.schema_version, SCHEMA_VERSION);
        assert_eq!(m.package, "com.example.meta");
        assert_eq!(m.pid, 99);
        assert!(m.has_measure_sdk);
        assert!(m.ended_at.is_some());

        let _ = fs::remove_dir_all(&tmp);
    }
}
