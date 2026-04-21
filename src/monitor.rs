use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

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
    // Measure SDK fields
    pub java_max_heap_kb: u64,
    pub java_total_heap_kb: u64,
    pub java_free_heap_kb: u64,
    pub total_pss_kb: u64,
    pub native_total_heap_kb: u64,
    pub native_free_heap_kb: u64,
    pub num_cores: u8,
}

pub struct MeasureMemory {
    pub java_max_heap_kb: u64,
    pub java_total_heap_kb: u64,
    pub java_free_heap_kb: u64,
    pub total_pss_kb: u64,
    pub rss_kb: u64,
    pub native_total_heap_kb: u64,
    pub native_free_heap_kb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
    pub debuggable: bool,
    pub disk_kb: u64,
    pub cache_kb: u64,
}

impl MonitorState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Enter => Some(Action::ZoomIn),
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }

    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            debuggable: true,
            disk_kb: 0,
            cache_kb: 0,
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        self.debuggable = sample.debuggable;
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
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

    pub fn push_measure_cpu(&mut self, percent: f32, num_cores: u8) {
        let mut sample = self.history.last().copied().unwrap_or_default();
        sample.cpu_percent = percent;
        sample.num_cores = num_cores;
        sample.data_kb = self.disk_kb;
        sample.cache_kb = self.cache_kb;
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
    }

    pub fn update_disk(&mut self, data_kb: u64, cache_kb: u64) {
        self.disk_kb = data_kb;
        self.cache_kb = cache_kb;
    }

    pub fn push_measure_memory(&mut self, mem: MeasureMemory) {
        if let Some(last) = self.history.last_mut() {
            last.rss_kb = mem.rss_kb;
            last.java_max_heap_kb = mem.java_max_heap_kb;
            last.java_total_heap_kb = mem.java_total_heap_kb;
            last.java_free_heap_kb = mem.java_free_heap_kb;
            last.total_pss_kb = mem.total_pss_kb;
            last.native_total_heap_kb = mem.native_total_heap_kb;
            last.native_free_heap_kb = mem.native_free_heap_kb;
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
    has_measure_sdk: bool,
) -> mpsc::Receiver<MonitorSample> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(1);
        let mut tick: u64 = 0;
        let mut last_data_kb: u64 = 0;
        let mut last_cache_kb: u64 = 0;
        let debuggable = adb.is_debuggable(&serial, &package);
        loop {
            let mask = visibility.load(Ordering::Relaxed);
            if mask == 0 {
                std::thread::sleep(interval);
                tick += 1;
                continue;
            }
            let mut sample = MonitorSample::default();
            let poll_system = !has_measure_sdk && mask & POLL_SYSTEM != 0;
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
                    sample.cpu_percent = cpu;
                }
                if let Some(Ok(Ok((data, cache)))) = disk.map(|h| h.join()) {
                    last_data_kb = data;
                    last_cache_kb = cache;
                }
            });

            sample.data_kb = last_data_kb;
            sample.cache_kb = last_cache_kb;
            sample.debuggable = debuggable;

            if has_measure_sdk && !poll_disk {
                std::thread::sleep(interval);
                tick += 1;
                continue;
            }

            if tx.send(sample).is_err() {
                return;
            }
            tick += 1;
            std::thread::sleep(interval);
        }
    });
    rx
}

// Measure SDK logcat parsing

use crate::logcat;

fn extract_field<'a>(data: &'a str, key: &str) -> Option<&'a str> {
    let start = data.find(key)? + key.len();
    let end = data[start..].find([',', ')']).map(|i| start + i).unwrap_or(data.len());
    Some(data[start..end].trim())
}

pub fn parse_cpu_usage(line: &str) -> Option<(f32, u8)> {
    let parsed = logcat::parse(line)?;
    if parsed.tag != "Measure" {
        return None;
    }
    let marker = "EventProcessor: CPU_USAGE, CpuUsageData(";
    let start = parsed.message.find(marker)? + marker.len();
    let data = &parsed.message[start..];
    let percent: f32 = extract_field(data, "percentage_usage=")?.parse().ok()?;
    let cores: u8 = extract_field(data, "num_cores=")?.parse().ok()?;
    Some((percent, cores))
}

pub fn parse_memory_usage(line: &str) -> Option<MeasureMemory> {
    let parsed = logcat::parse(line)?;
    if parsed.tag != "Measure" {
        return None;
    }
    let marker = "EventProcessor: MEMORY_USAGE, MemoryUsageData(";
    let start = parsed.message.find(marker)? + marker.len();
    let data = &parsed.message[start..];
    Some(MeasureMemory {
        java_max_heap_kb: extract_field(data, "java_max_heap=")?.parse().ok()?,
        java_total_heap_kb: extract_field(data, "java_total_heap=")?.parse().ok()?,
        java_free_heap_kb: extract_field(data, "java_free_heap=")?.parse().ok()?,
        total_pss_kb: extract_field(data, "total_pss=")?.parse().ok()?,
        rss_kb: extract_field(data, "rss=")?.parse().ok()?,
        native_total_heap_kb: extract_field(data, "native_total_heap=")?.parse().ok()?,
        native_free_heap_kb: extract_field(data, "native_free_heap=")?.parse().ok()?,
    })
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

    #[test]
    fn parse_cpu_usage_line() {
        let line = "04-15 14:58:27.336 10329 10364 D Measure : EventProcessor: CPU_USAGE, CpuUsageData(num_cores=8, clock_speed=100, start_time=8267336, uptime=83137214, utime=3437, cutime=0, cstime=0, stime=653, interval=5002, percentage_usage=6.1)";
        let (percent, cores) = parse_cpu_usage(line).unwrap();
        assert!((percent - 6.1).abs() < 0.01);
        assert_eq!(cores, 8);
    }

    #[test]
    fn parse_memory_usage_line() {
        let line = "04-15 14:58:27.576 10329 10364 D Measure : EventProcessor: MEMORY_USAGE, MemoryUsageData(java_max_heap=262144, java_total_heap=32533, java_free_heap=13860, total_pss=151528, rss=252144, native_total_heap=25084, native_free_heap=3949, interval=5065)";
        let mem = parse_memory_usage(line).unwrap();
        assert_eq!(mem.java_max_heap_kb, 262144);
        assert_eq!(mem.java_total_heap_kb, 32533);
        assert_eq!(mem.java_free_heap_kb, 13860);
        assert_eq!(mem.total_pss_kb, 151528);
        assert_eq!(mem.rss_kb, 252144);
        assert_eq!(mem.native_total_heap_kb, 25084);
        assert_eq!(mem.native_free_heap_kb, 3949);
    }

    #[test]
    fn parse_non_measure_cpu_line() {
        let line = "04-15 14:58:27.336 10329 10364 D OkHttp  : something";
        assert!(parse_cpu_usage(line).is_none());
    }

    #[test]
    fn push_measure_memory_updates_last() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        state.push_measure_memory(MeasureMemory {
            java_max_heap_kb: 262144,
            java_total_heap_kb: 32533,
            java_free_heap_kb: 13860,
            total_pss_kb: 151528,
            rss_kb: 252144,
            native_total_heap_kb: 25084,
            native_free_heap_kb: 3949,
        });
        let last = state.history.last().unwrap();
        assert_eq!(last.rss_kb, 252144);
        assert_eq!(last.java_max_heap_kb, 262144);
    }

    #[test]
    fn push_measure_cpu_creates_new_sample() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        assert_eq!(state.history.len(), 1);
        state.push_measure_cpu(6.1, 8);
        assert_eq!(state.history.len(), 2);
        let last = state.history.last().unwrap();
        assert!((last.cpu_percent - 6.1).abs() < 0.01);
        assert_eq!(last.num_cores, 8);
    }

    #[test]
    fn push_measure_cpu_carries_forward_memory() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        state.push_measure_memory(MeasureMemory {
            java_max_heap_kb: 262144,
            java_total_heap_kb: 32533,
            java_free_heap_kb: 13860,
            total_pss_kb: 151528,
            rss_kb: 252144,
            native_total_heap_kb: 25084,
            native_free_heap_kb: 3949,
        });
        state.push_measure_cpu(3.0, 8);
        let last = state.history.last().unwrap();
        assert_eq!(last.java_max_heap_kb, 262144);
        assert_eq!(last.rss_kb, 252144);
        assert!((last.cpu_percent - 3.0).abs() < 0.01);
    }

    #[test]
    fn push_measure_cpu_stamps_disk_from_state() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        state.update_disk(500, 100);
        state.push_measure_cpu(5.0, 4);
        let last = state.history.last().unwrap();
        assert_eq!(last.data_kb, 500);
        assert_eq!(last.cache_kb, 100);
    }

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn handle_key_enter_zooms_in() {
        let mut state = MonitorState::new();
        let action = state.handle_key(key_event(KeyCode::Enter));
        assert!(matches!(action, Some(Action::ZoomIn)));
    }

    #[test]
    fn handle_key_esc_unfocuses() {
        let mut state = MonitorState::new();
        let action = state.handle_key(key_event(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn update_disk_sets_state_fields() {
        let mut state = MonitorState::new();
        assert_eq!(state.disk_kb, 0);
        state.update_disk(1024, 256);
        assert_eq!(state.disk_kb, 1024);
        assert_eq!(state.cache_kb, 256);
    }
}
