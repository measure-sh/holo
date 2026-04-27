use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Adb;
use crate::app::Action;
use crate::panel;

pub const POLL_SYSTEM: u8 = 1;
pub const POLL_DISK: u8 = 2;

const MAX_SAMPLES: usize = 120;
const NS_PER_SEC: i64 = 1_000_000_000;
/// Drop GC events and memory samples older than this many seconds relative to
/// the freshest agent timestamp we've seen. Matches the chart's visible span.
const RETENTION_NS: i64 = MAX_SAMPLES as i64 * NS_PER_SEC;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorSample {
    pub cpu_percent: f32,
    pub data_kb: u64,
    pub cache_kb: u64,
    pub debuggable: bool,
    pub num_cores: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

/// GC pause as reported by the JVMTI agent. `ts_ns` is the agent's
/// `CLOCK_MONOTONIC` timestamp at GC end, the same clock that stamps
/// `MemorySample.ts_ns` — the two streams are aligned by construction.
#[derive(Debug, Clone, Copy)]
pub struct GcEvent {
    pub ts_ns: i64,
    #[allow(dead_code)]
    pub duration_us: u32,
}

/// Memory snapshot emitted by the agent at ~1 Hz. `ts_ns` is the agent's
/// `CLOCK_MONOTONIC` timestamp; `java_heap_kb` and `native_heap_kb` are
/// reserved for forward-compatible agent extensions.
#[derive(Debug, Clone, Copy)]
pub struct MemorySample {
    pub ts_ns: i64,
    pub rss_kb: u64,
    #[allow(dead_code)]
    pub java_heap_kb: Option<u64>,
    #[allow(dead_code)]
    pub native_heap_kb: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorView {
    All,
    Cpu,
    /// Memory: Java heap (when the agent provides it), with Native + RSS
    /// available as overlays in the detail view.
    Memory,
    Disk,
    Download,
    Upload,
}

const VIEWS: [MonitorView; 6] = [
    MonitorView::All,
    MonitorView::Cpu,
    MonitorView::Memory,
    MonitorView::Disk,
    MonitorView::Download,
    MonitorView::Upload,
];

impl MonitorView {
    pub fn label(self) -> &'static str {
        match self {
            MonitorView::All => "All",
            MonitorView::Cpu => "CPU",
            MonitorView::Memory => "Memory",
            MonitorView::Disk => "Disk",
            MonitorView::Download => "Download",
            MonitorView::Upload => "Upload",
        }
    }

    fn cycle(self, forward: bool) -> Self {
        let i = VIEWS.iter().position(|v| *v == self).unwrap_or(0);
        let next = if forward {
            (i + 1) % VIEWS.len()
        } else {
            (i + VIEWS.len() - 1) % VIEWS.len()
        };
        VIEWS[next]
    }
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
    pub gc_events: Vec<GcEvent>,
    pub memory_history: Vec<MemorySample>,
    pub debuggable: bool,
    pub view: MonitorView,
}

impl MonitorState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Up | KeyCode::Char('k') => {
                self.view = self.view.cycle(false);
                Some(Action::Noop)
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Down | KeyCode::Char('j') => {
                self.view = self.view.cycle(true);
                Some(Action::Noop)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }

    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            gc_events: Vec::new(),
            memory_history: Vec::new(),
            debuggable: true,
            view: MonitorView::All,
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        self.debuggable = sample.debuggable;
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
    }

    pub fn push_gc(&mut self, ts_ns: i64, duration_us: u32) {
        self.gc_events.push(GcEvent { ts_ns, duration_us });
        self.evict_at_ns(ts_ns);
    }

    pub fn push_memory(
        &mut self,
        ts_ns: i64,
        rss_kb: u64,
        java_heap_kb: Option<u64>,
        native_heap_kb: Option<u64>,
    ) {
        self.memory_history.push(MemorySample {
            ts_ns,
            rss_kb,
            java_heap_kb,
            native_heap_kb,
        });
        if self.memory_history.len() > MAX_SAMPLES {
            self.memory_history.remove(0);
        }
        self.evict_at_ns(ts_ns);
    }

    /// Drop GC events and memory samples older than `RETENTION_NS` relative to
    /// the just-pushed `now_ns`. Both streams share the agent's
    /// `CLOCK_MONOTONIC`, so a single cutoff applies cleanly to both.
    fn evict_at_ns(&mut self, now_ns: i64) {
        let cutoff = now_ns.saturating_sub(RETENTION_NS);
        self.gc_events.retain(|e| e.ts_ns >= cutoff);
        self.memory_history.retain(|s| s.ts_ns >= cutoff);
    }

    /// Latest known agent timestamp across either stream — used as the chart's
    /// "now" anchor when rendering. Returns `None` until the agent has emitted
    /// at least one event.
    pub fn latest_agent_ts_ns(&self) -> Option<i64> {
        let mem = self.memory_history.last().map(|s| s.ts_ns);
        let gc = self.gc_events.last().map(|e| e.ts_ns);
        match (mem, gc) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    #[allow(dead_code)]
    pub fn trend_u64(&self, extract: fn(&MonitorSample) -> u64) -> Trend {
        if self.history.len() < 3 {
            return Trend::Stable;
        }
        let recent: Vec<u64> = self.history.iter().rev().take(5).map(extract).collect();
        let newest = recent[0];
        let oldest = *recent.last().unwrap();
        if oldest == 0 {
            return Trend::Stable;
        }
        let change_pct = ((newest as f64 - oldest as f64) / oldest as f64 * 100.0).abs();
        if change_pct < 3.0 {
            Trend::Stable
        } else if newest > oldest {
            Trend::Rising
        } else {
            Trend::Falling
        }
    }

}

pub fn visibility_mask(vis: &[bool; 8]) -> u8 {
    let mut mask = 0u8;
    if vis[(panel::MONITOR - 1) as usize] {
        mask |= POLL_SYSTEM | POLL_DISK;
    }
    mask
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    visibility: Arc<AtomicU8>,
    connectivity: Arc<AtomicBool>,
) -> mpsc::Receiver<MonitorSample> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(1);
        let mut tick: u64 = 0;
        let mut last_data_kb: u64 = 0;
        let mut last_cache_kb: u64 = 0;
        let debuggable = adb.is_debuggable(&serial, &package);
        let num_cores = adb.get_num_cores(&serial).unwrap_or(1).max(1);
        loop {
            let mask = visibility.load(Ordering::Relaxed);
            if mask == 0 || !connectivity.load(Ordering::Relaxed) {
                std::thread::sleep(interval);
                tick += 1;
                continue;
            }
            let mut sample = MonitorSample::default();
            let poll_system = mask & POLL_SYSTEM != 0;
            let poll_disk = mask & POLL_DISK != 0 && tick.is_multiple_of(5);

            std::thread::scope(|s| {
                let cpu = poll_system
                    .then(|| s.spawn(|| adb.get_cpu_usage(&serial, &package)));
                let disk = poll_disk
                    .then(|| s.spawn(|| adb.get_disk_usage(&serial, &package)));

                if let Some(Ok(Ok(cpu))) = cpu.map(|h| h.join()) {
                    sample.cpu_percent = (cpu / num_cores as f32).clamp(0.0, 100.0);
                    sample.num_cores = num_cores;
                }
                if let Some(Ok(Ok((data, cache)))) = disk.map(|h| h.join()) {
                    last_data_kb = data;
                    last_cache_kb = cache;
                }
            });

            sample.data_kb = last_data_kb;
            sample.cache_kb = last_cache_kb;
            sample.debuggable = debuggable;

            if tx.send(sample).is_err() {
                return;
            }
            tick += 1;
            std::thread::sleep(interval);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(data: u64) -> MonitorSample {
        MonitorSample {
            data_kb: data,
            debuggable: true,
            ..Default::default()
        }
    }

    #[test]
    fn push_caps_at_max_samples() {
        let mut state = MonitorState::new();
        for i in 0..130 {
            state.push(sample(i));
        }
        assert_eq!(state.history.len(), MAX_SAMPLES);
        assert_eq!(state.history.last().unwrap().data_kb, 129);
    }

    #[test]
    fn trend_stable_with_few_samples() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        state.push(sample(200));
        assert_eq!(state.trend_u64(|m| m.data_kb), Trend::Stable);
    }

    #[test]
    fn trend_rising_when_increasing() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(100 + i * 20));
        }
        assert_eq!(state.trend_u64(|m| m.data_kb), Trend::Rising);
    }

    #[test]
    fn trend_falling_when_decreasing() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(200 - i * 20));
        }
        assert_eq!(state.trend_u64(|m| m.data_kb), Trend::Falling);
    }

    #[test]
    fn trend_stable_with_small_changes() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(1000 + i));
        }
        assert_eq!(state.trend_u64(|m| m.data_kb), Trend::Stable);
    }

    #[test]
    fn push_tracks_debuggable_flag() {
        let mut state = MonitorState::new();
        assert!(state.debuggable);
        state.push(MonitorSample {
            debuggable: false,
            ..Default::default()
        });
        assert!(!state.debuggable);
    }

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn cycle_view_forward_wraps_through_all_positions() {
        let mut v = MonitorView::All;
        let order = [
            MonitorView::Cpu,
            MonitorView::Memory,
            MonitorView::Disk,
            MonitorView::Download,
            MonitorView::Upload,
            MonitorView::All,
        ];
        for expected in order {
            v = v.cycle(true);
            assert_eq!(v, expected);
        }
    }

    #[test]
    fn cycle_view_backward_wraps_to_last() {
        assert_eq!(MonitorView::All.cycle(false), MonitorView::Upload);
    }

    #[test]
    fn handle_key_right_advances_view() {
        let mut state = MonitorState::new();
        state.handle_key(key_event(KeyCode::Right));
        assert_eq!(state.view, MonitorView::Cpu);
    }

    #[test]
    fn handle_key_left_steps_view_backwards() {
        let mut state = MonitorState::new();
        state.view = MonitorView::Memory;
        state.handle_key(key_event(KeyCode::Left));
        assert_eq!(state.view, MonitorView::Cpu);
    }

    #[test]
    fn handle_key_esc_unfocuses_without_resetting_view() {
        let mut state = MonitorState::new();
        state.view = MonitorView::Disk;
        let action = state.handle_key(key_event(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Unfocus)));
        assert_eq!(state.view, MonitorView::Disk);
    }

    #[test]
    fn push_gc_records_event() {
        let mut state = MonitorState::new();
        state.push_gc(42, 1234);
        assert_eq!(state.gc_events.len(), 1);
        assert_eq!(state.gc_events[0].ts_ns, 42);
        assert_eq!(state.gc_events[0].duration_us, 1234);
    }

    #[test]
    fn push_memory_records_sample_and_caps() {
        let mut state = MonitorState::new();
        for i in 0..(MAX_SAMPLES as i64 + 5) {
            state.push_memory(i * NS_PER_SEC, (1000 + i) as u64, None, None);
        }
        assert_eq!(state.memory_history.len(), MAX_SAMPLES);
        assert_eq!(
            state.memory_history.last().unwrap().rss_kb,
            (1000 + MAX_SAMPLES as i64 + 4) as u64,
        );
    }

    #[test]
    fn push_memory_evicts_stale_gc_events() {
        let mut state = MonitorState::new();
        // GC event from "the distant past" relative to a fresh memory sample.
        state.gc_events.push(GcEvent { ts_ns: 0, duration_us: 5 });
        state.push_memory(RETENTION_NS + NS_PER_SEC, 100, None, None);
        assert!(state.gc_events.is_empty());
    }

    #[test]
    fn push_memory_keeps_recent_gc_events() {
        let mut state = MonitorState::new();
        let now = 500 * NS_PER_SEC;
        state.gc_events.push(GcEvent { ts_ns: now - NS_PER_SEC, duration_us: 5 });
        state.push_memory(now, 100, None, None);
        assert_eq!(state.gc_events.len(), 1);
    }

    #[test]
    fn evict_keeps_events_at_exact_cutoff() {
        let mut state = MonitorState::new();
        let now = 500 * NS_PER_SEC;
        state.gc_events = vec![GcEvent { ts_ns: now - RETENTION_NS, duration_us: 99 }];
        state.evict_at_ns(now);
        assert_eq!(state.gc_events.len(), 1);
    }

    #[test]
    fn latest_agent_ts_ns_picks_freshest_across_streams() {
        let mut state = MonitorState::new();
        assert_eq!(state.latest_agent_ts_ns(), None);
        state.push_memory(100, 1, None, None);
        state.push_gc(200, 1);
        assert_eq!(state.latest_agent_ts_ns(), Some(200));
        state.push_memory(300, 2, None, None);
        assert_eq!(state.latest_agent_ts_ns(), Some(300));
    }
}
