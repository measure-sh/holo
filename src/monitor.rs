use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::adb::Adb;
use crate::panel;

pub const POLL_SYSTEM: u8 = 1;
pub const POLL_DISK: u8 = 2;

const MAX_SAMPLES: usize = 60;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorSample {
    pub rss_kb: u64,
    pub cpu_percent: f32,
    pub data_kb: u64,
    pub cache_kb: u64,
    pub debuggable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
    pub debuggable: bool,
}

impl MonitorState {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            debuggable: true,
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        self.debuggable = sample.debuggable;
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
    }

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

    pub fn sparkline_u64(&self, extract: fn(&MonitorSample) -> u64) -> Vec<u64> {
        self.history.iter().map(extract).collect()
    }

    pub fn sparkline_f32(&self, extract: fn(&MonitorSample) -> f32) -> Vec<f32> {
        self.history.iter().map(extract).collect()
    }

}

pub fn visibility_mask(vis: &[bool; 9]) -> u8 {
    let mut mask = 0u8;
    if vis[(panel::SYSTEM - 1) as usize] { mask |= POLL_SYSTEM; }
    if vis[(panel::DISK - 1) as usize] { mask |= POLL_DISK; }
    mask
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    visibility: Arc<AtomicU8>,
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
            let poll_disk = mask & POLL_DISK != 0 && tick.is_multiple_of(5);

            std::thread::scope(|s| {
                let mem = (mask & POLL_SYSTEM != 0)
                    .then(|| s.spawn(|| adb.get_meminfo(&serial, &package)));
                let cpu = (mask & POLL_SYSTEM != 0)
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
        for i in 0..70 {
            state.push(sample(i));
        }
        assert_eq!(state.history.len(), MAX_SAMPLES);
        assert_eq!(state.history.last().unwrap().rss_kb, 69);
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
    fn sparkline_extracts_values() {
        let mut state = MonitorState::new();
        state.push(sample(100));
        state.push(sample(120));
        assert_eq!(state.sparkline_u64(|m| m.rss_kb), vec![100, 120]);
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

}
