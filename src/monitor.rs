use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Adb;
use crate::app::Action;
use crate::panel;

pub const POLL_SYSTEM: u8 = 1;
pub const POLL_DISK: u8 = 2;

const MAX_SAMPLES: usize = 120;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorSample {
    pub rss_kb: u64,
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

#[derive(Debug, Clone, Copy)]
pub struct GcEvent {
    pub received_at: Instant,
    #[allow(dead_code)]
    pub duration_us: u32,
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
    pub gc_events: Vec<GcEvent>,
    pub debuggable: bool,
    pub selected_metric: usize,
    pub detail_open: bool,
}

impl MonitorState {
    pub fn handle_key(&mut self, key: KeyEvent, metric_count: usize) -> Option<Action> {
        let max_index = metric_count.saturating_sub(1);

        if self.detail_open {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('h') => {
                    self.selected_metric = self.selected_metric.saturating_sub(1);
                    Some(Action::Noop)
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('l') => {
                    self.selected_metric = (self.selected_metric + 1).min(max_index);
                    Some(Action::Noop)
                }
                KeyCode::Enter | KeyCode::Esc => {
                    self.detail_open = false;
                    Some(Action::Noop)
                }
                _ => Some(Action::Noop),
            };
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_metric = self.selected_metric.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_metric = (self.selected_metric + 1).min(max_index);
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if metric_count > 0 {
                    self.detail_open = true;
                }
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
            debuggable: true,
            selected_metric: 0,
            detail_open: false,
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        self.debuggable = sample.debuggable;
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
        self.evict_old_gc_events_at(Instant::now());
    }

    pub fn push_gc(&mut self, duration_us: u32) {
        self.gc_events.push(GcEvent {
            received_at: Instant::now(),
            duration_us,
        });
        self.evict_old_gc_events_at(Instant::now());
    }

    fn evict_old_gc_events_at(&mut self, now: Instant) {
        let cutoff = now
            .checked_sub(Duration::from_secs(MAX_SAMPLES as u64))
            .unwrap_or(now);
        self.gc_events.retain(|e| e.received_at >= cutoff);
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
                let mem = poll_system
                    .then(|| s.spawn(|| adb.get_meminfo(&serial, &package)));
                let cpu = poll_system
                    .then(|| s.spawn(|| adb.get_cpu_usage(&serial, &package)));
                let disk = poll_disk
                    .then(|| s.spawn(|| adb.get_disk_usage(&serial, &package)));

                if let Some(Ok(Ok(mem))) = mem.map(|h| h.join()) {
                    sample.rss_kb = mem.rss_kb;
                }
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

    fn sample(rss: u64) -> MonitorSample {
        MonitorSample {
            rss_kb: rss,
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
        assert_eq!(state.history.last().unwrap().rss_kb, 129);
    }

    #[test]
    fn trend_stable_with_few_samples() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        state.push(sample(200));
        assert_eq!(state.trend_u64(|m| m.rss_kb), Trend::Stable);
    }

    #[test]
    fn trend_rising_when_increasing() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(100 + i * 20));
        }
        assert_eq!(state.trend_u64(|m| m.rss_kb), Trend::Rising);
    }

    #[test]
    fn trend_falling_when_decreasing() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(200 - i * 20));
        }
        assert_eq!(state.trend_u64(|m| m.rss_kb), Trend::Falling);
    }

    #[test]
    fn trend_stable_with_small_changes() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(1000 + i));
        }
        assert_eq!(state.trend_u64(|m| m.rss_kb), Trend::Stable);
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
    fn handle_key_enter_opens_detail() {
        let mut state = MonitorState::new();
        let action = state.handle_key(key_event(KeyCode::Enter), 5);
        assert!(matches!(action, Some(Action::Noop)));
        assert!(state.detail_open);
    }

    #[test]
    fn handle_key_enter_noop_when_no_metrics() {
        let mut state = MonitorState::new();
        state.handle_key(key_event(KeyCode::Enter), 0);
        assert!(!state.detail_open);
    }

    #[test]
    fn handle_key_esc_unfocuses() {
        let mut state = MonitorState::new();
        let action = state.handle_key(key_event(KeyCode::Esc), 5);
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn handle_key_down_advances_selection_clamped() {
        let mut state = MonitorState::new();
        for _ in 0..10 {
            state.handle_key(key_event(KeyCode::Down), 3);
        }
        assert_eq!(state.selected_metric, 2);
    }

    #[test]
    fn handle_key_up_decreases_selection_saturating() {
        let mut state = MonitorState::new();
        state.selected_metric = 1;
        state.handle_key(key_event(KeyCode::Up), 5);
        state.handle_key(key_event(KeyCode::Up), 5);
        assert_eq!(state.selected_metric, 0);
    }

    #[test]
    fn handle_key_down_in_detail_switches_metric() {
        let mut state = MonitorState::new();
        state.detail_open = true;
        state.handle_key(key_event(KeyCode::Down), 5);
        assert!(state.detail_open);
        assert_eq!(state.selected_metric, 1);
    }

    #[test]
    fn handle_key_enter_closes_detail() {
        let mut state = MonitorState::new();
        state.detail_open = true;
        let action = state.handle_key(key_event(KeyCode::Enter), 5);
        assert!(matches!(action, Some(Action::Noop)));
        assert!(!state.detail_open);
    }

    #[test]
    fn handle_key_esc_in_detail_closes_without_unfocus() {
        let mut state = MonitorState::new();
        state.detail_open = true;
        let action = state.handle_key(key_event(KeyCode::Esc), 4);
        assert!(matches!(action, Some(Action::Noop)));
        assert!(!state.detail_open);
    }

    #[test]
    fn handle_key_right_in_detail_advances_metric() {
        let mut state = MonitorState::new();
        state.detail_open = true;
        state.handle_key(key_event(KeyCode::Right), 4);
        assert_eq!(state.selected_metric, 1);
    }

    #[test]
    fn handle_key_left_in_detail_decreases_metric() {
        let mut state = MonitorState::new();
        state.detail_open = true;
        state.selected_metric = 2;
        state.handle_key(key_event(KeyCode::Left), 4);
        assert_eq!(state.selected_metric, 1);
    }

    #[test]
    fn push_gc_records_event() {
        let mut state = MonitorState::new();
        state.push_gc(1234);
        assert_eq!(state.gc_events.len(), 1);
        assert_eq!(state.gc_events[0].duration_us, 1234);
    }

    #[test]
    fn evict_drops_events_older_than_window() {
        let mut state = MonitorState::new();
        let now = Instant::now();
        state.gc_events = vec![
            GcEvent {
                received_at: now - Duration::from_secs(MAX_SAMPLES as u64 + 5),
                duration_us: 1,
            },
            GcEvent {
                received_at: now - Duration::from_secs(MAX_SAMPLES as u64 - 5),
                duration_us: 2,
            },
            GcEvent { received_at: now, duration_us: 3 },
        ];

        state.evict_old_gc_events_at(now);

        let durations: Vec<u32> = state.gc_events.iter().map(|e| e.duration_us).collect();
        assert_eq!(durations, vec![2, 3]);
    }

    #[test]
    fn evict_keeps_events_at_exact_cutoff() {
        let mut state = MonitorState::new();
        let now = Instant::now();
        state.gc_events = vec![GcEvent {
            received_at: now - Duration::from_secs(MAX_SAMPLES as u64),
            duration_us: 99,
        }];

        state.evict_old_gc_events_at(now);
        assert_eq!(state.gc_events.len(), 1);
    }

    #[test]
    fn push_sample_evicts_stale_gc_events() {
        let mut state = MonitorState::new();
        state.gc_events.push(GcEvent {
            received_at: Instant::now() - Duration::from_secs(MAX_SAMPLES as u64 + 10),
            duration_us: 5,
        });
        state.push(MonitorSample::default());
        assert!(state.gc_events.is_empty());
    }
}
