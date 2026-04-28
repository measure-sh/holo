use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::mpsc;

use crate::issues::IssuesState;
use crate::monitor::MonitorState;
use crate::network::NetworkState;
use crate::session::{self, SessionId, SessionMeta, SCHEMA_VERSION};
use crate::vitals::{self, VitalsEvent};
use crate::{anrs::AnrEntry, crashes::CrashEntry, network};

/// Tail-window cap for replay logcat. A 30-min chatty session can grow past
/// 36 MB; loading more than this synchronously visibly hitches ratatui, and
/// the in-UI logcat is capped at 1000 lines anyway.
const LOGCAT_TAIL_BYTES: u64 = 50 * 1024 * 1024;

/// All in-memory state the UI needs to render a replay. Built off the main
/// thread by `load`; the housekeeping loop installs it into `App` on receive.
pub struct ReplaySession {
    pub id: SessionId,
    pub meta: SessionMeta,
    pub logcat: Vec<String>,
    pub monitor: MonitorState,
    pub issues: IssuesState,
    pub network: NetworkState,
    pub traces_dir: PathBuf,
}

/// Spawn a loader thread; the receiver fires once with the assembled session
/// (or an error string suitable for status_flash). Sessions can be tens of
/// MBs; doing this synchronously on the main thread is visibly janky.
pub fn load(id: SessionId) -> mpsc::Receiver<Result<ReplaySession, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(load_blocking(&id).map_err(|e| e.to_string()));
    });
    rx
}

fn load_blocking(id: &SessionId) -> Result<ReplaySession, String> {
    let meta = session::read_meta(id)
        .ok_or_else(|| "session metadata missing or unreadable".to_string())?;
    if meta.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "session schema {} unsupported (this holo speaks {})",
            meta.schema_version, SCHEMA_VERSION,
        ));
    }
    let dir = id.dir();

    let (logcat, network) = load_logcat_and_network(&dir.join("logcat.log"));
    let monitor = load_monitor(id, &dir.join("vitals.bin"), meta.debuggable);
    let issues = load_issues(id);
    let traces_dir = dir.join("traces");

    Ok(ReplaySession {
        id: id.clone(),
        meta,
        logcat,
        monitor,
        issues,
        network,
        traces_dir,
    })
}

/// Read the trailing window of logcat lines (cheap on EOF), and rebuild the
/// `NetworkState` by re-running the same parser the live path uses
/// (`network::parse_http_data`). One source of truth.
fn load_logcat_and_network(path: &PathBuf) -> (Vec<String>, NetworkState) {
    let mut network = NetworkState::new();
    let lines = match File::open(path) {
        Ok(file) => read_tail_lines(file, LOGCAT_TAIL_BYTES),
        Err(_) => Vec::new(),
    };
    for line in &lines {
        if let Some(entry) = network::parse_http_data(line) {
            network.push(entry);
        }
    }
    (lines, network)
}

/// Read the last `cap` bytes worth of `\n`-delimited text from `file` as a
/// `Vec<String>`. If the file is smaller than the cap, returns everything.
fn read_tail_lines(file: File, cap: u64) -> Vec<String> {
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(cap);
    let mut reader = BufReader::new(file);
    if start > 0 {
        use std::io::{Seek, SeekFrom};
        if reader.seek(SeekFrom::Start(start)).is_err() {
            return Vec::new();
        }
        // Drop the (likely partial) first line so callers see only complete
        // logcat entries.
        let mut throwaway = String::new();
        let _ = reader.read_line(&mut throwaway);
    }
    reader.lines().map_while(Result::ok).collect()
}

fn load_monitor(id: &SessionId, vitals_path: &PathBuf, debuggable: bool) -> MonitorState {
    let mut state = MonitorState::new();
    // Seed from the captured metadata. Old (pre-debuggable-field) sessions
    // arrive with `debuggable = false` (serde default), which is the
    // conservative read — better to miss a "vitals attached" badge than
    // claim it for a session where the agent never ran.
    state.debuggable = debuggable;

    if let Ok(file) = File::open(vitals_path) {
        for event in vitals::decode_frames(BufReader::new(file)) {
            apply_event_to_monitor(&mut state, event);
        }
    }

    // Disk samples carry their own debuggable flag; the loop below
    // overwrites `state.debuggable` with the most recent sample's value,
    // matching live-mode behavior in `MonitorState::push`.
    for sample in session::read_disk_samples(id) {
        state.push(crate::monitor::MonitorSample {
            data_kb: sample.data_kb,
            debuggable: sample.debuggable,
        });
    }

    state
}

fn apply_event_to_monitor(state: &mut MonitorState, event: VitalsEvent) {
    match event {
        VitalsEvent::Gc { ts_ns, duration_us } => state.push_gc(ts_ns, duration_us),
        VitalsEvent::Memory { ts_ns, rss_kb, java_heap_kb, native_heap_kb } => {
            state.push_memory(ts_ns, rss_kb, java_heap_kb, native_heap_kb);
        }
        VitalsEvent::Cpu { ts_ns, cpu_centi_percent, num_threads } => {
            state.push_cpu(ts_ns, cpu_centi_percent as f32 / 100.0, num_threads);
        }
        VitalsEvent::Network { ts_ns, rx_bytes, tx_bytes } => {
            state.push_network(ts_ns, rx_bytes, tx_bytes);
        }
    }
}

fn load_issues(id: &SessionId) -> IssuesState {
    let mut crashes: Vec<CrashEntry> = Vec::new();
    let mut anrs: Vec<AnrEntry> = Vec::new();
    for issue in session::read_issues(id) {
        match issue.kind.as_str() {
            "crash" => crashes.push(CrashEntry {
                timestamp: issue.timestamp,
                exception: issue.description,
                full_text: issue.full_text,
            }),
            "anr" => anrs.push(AnrEntry {
                timestamp: issue.timestamp,
                reason: issue.description,
                full_text: issue.full_text,
            }),
            _ => {}
        }
    }
    let mut state = IssuesState::new();
    state.set_crashes(crashes);
    state.set_anrs(anrs);
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionSeed, SessionWriter, TEST_ENV_LOCK};
    use std::fs;

    fn pin_cache_dir(tmp: &std::path::Path) {
        unsafe {
            std::env::set_var("HOME", tmp);
            std::env::set_var("XDG_CACHE_HOME", tmp.join(".cache"));
        }
    }

    fn seed(device: &str, package: &str, pid: u32) -> SessionSeed {
        SessionSeed {
            device_serial: device.into(),
            device_model: None,
            package: package.into(),
            pid,
            app_version: None,
            has_measure_sdk: false,
            debuggable: false,
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

    #[test]
    fn round_trip_writer_to_replay_reproduces_state() {
        let _g = TEST_ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("holo-replay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        pin_cache_dir(&tmp);

        let mut w = SessionWriter::open(seed("emu", "com.test", 17)).unwrap();
        let id = w.id().clone();
        w.write_logcat_line("01-01 12:00:00.000  100  100 D Tag : hello");
        w.write_logcat_line("01-01 12:00:00.001  100  100 E Tag : oops");
        // Two GC events at distinct timestamps inside the retention window.
        let (kind, payload) = vitals::encode_event(
            &VitalsEvent::Gc { ts_ns: 1_000_000_000, duration_us: 250 },
        );
        w.write_vitals_frame(kind, &payload);
        let (kind, payload) = vitals::encode_event(
            &VitalsEvent::Memory {
                ts_ns: 2_000_000_000,
                rss_kb: 5000,
                java_heap_kb: 1000,
                native_heap_kb: 500,
            },
        );
        w.write_vitals_frame(kind, &payload);
        w.write_crash_if_new(&CrashEntry {
            timestamp: "2026-04-28 10:00:00".into(),
            exception: "NPE".into(),
            full_text: "stack".into(),
        });
        drop(w);

        let session = load_blocking(&id).expect("replay loads");
        assert_eq!(session.logcat.len(), 2);
        assert_eq!(session.monitor.gc_events.len(), 1);
        assert_eq!(session.monitor.memory_history.len(), 1);
        assert_eq!(session.monitor.memory_history[0].rss_kb, 5000);
        assert_eq!(session.issues.issues.len(), 1);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_unsupported_schema() {
        let _g = TEST_ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("holo-replay-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        pin_cache_dir(&tmp);

        let w = SessionWriter::open(seed("emu", "com.test", 1)).unwrap();
        let id = w.id().clone();
        drop(w);

        // Hand-edit the metadata to claim a future schema.
        let meta_path = id.dir().join("metadata.json");
        let mut meta: SessionMeta = serde_json::from_str(
            &fs::read_to_string(&meta_path).unwrap()
        ).unwrap();
        meta.schema_version = u32::MAX;
        fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();

        let result = load_blocking(&id);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }
}
