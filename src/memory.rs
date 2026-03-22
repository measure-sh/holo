use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::adb::{Adb, MemInfo};

const MAX_SAMPLES: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

pub struct MemoryState {
    pub history: Vec<MemInfo>,
}

impl MemoryState {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    pub fn push(&mut self, sample: MemInfo) {
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
    }

    pub fn latest(&self) -> Option<&MemInfo> {
        self.history.last()
    }

    pub fn first(&self) -> Option<&MemInfo> {
        self.history.first()
    }

    pub fn trend(&self, extract: fn(&MemInfo) -> u64) -> Trend {
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

    pub fn sparkline_data(&self, extract: fn(&MemInfo) -> u64) -> Vec<u64> {
        self.history.iter().map(extract).collect()
    }
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<MemInfo> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(5);
        loop {
            if let Ok(info) = adb.get_meminfo(&serial, &package) {
                if tx.send(info).is_err() {
                    return;
                }
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(total: u64, java: u64, native: u64) -> MemInfo {
        MemInfo {
            total_pss_kb: total,
            java_heap_kb: java,
            native_heap_kb: native,
        }
    }

    #[test]
    fn push_caps_at_max_samples() {
        let mut state = MemoryState::new();
        for i in 0..70 {
            state.push(sample(i, 0, 0));
        }
        assert_eq!(state.history.len(), MAX_SAMPLES);
        assert_eq!(state.first().unwrap().total_pss_kb, 10);
        assert_eq!(state.latest().unwrap().total_pss_kb, 69);
    }

    #[test]
    fn trend_stable_with_few_samples() {
        let mut state = MemoryState::new();
        state.push(sample(100, 0, 0));
        state.push(sample(200, 0, 0));
        assert_eq!(state.trend(|m| m.total_pss_kb), Trend::Stable);
    }

    #[test]
    fn trend_rising_when_increasing() {
        let mut state = MemoryState::new();
        for i in 0..5 {
            state.push(sample(100 + i * 20, 0, 0));
        }
        assert_eq!(state.trend(|m| m.total_pss_kb), Trend::Rising);
    }

    #[test]
    fn trend_falling_when_decreasing() {
        let mut state = MemoryState::new();
        for i in 0..5 {
            state.push(sample(200 - i * 20, 0, 0));
        }
        assert_eq!(state.trend(|m| m.total_pss_kb), Trend::Falling);
    }

    #[test]
    fn trend_stable_with_small_changes() {
        let mut state = MemoryState::new();
        for i in 0..5 {
            state.push(sample(1000 + i, 0, 0));
        }
        assert_eq!(state.trend(|m| m.total_pss_kb), Trend::Stable);
    }

    #[test]
    fn sparkline_data_extracts_values() {
        let mut state = MemoryState::new();
        state.push(sample(100, 40, 50));
        state.push(sample(120, 45, 55));
        assert_eq!(state.sparkline_data(|m| m.java_heap_kb), vec![40, 45]);
    }
}
