use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::adb::Adb;

const MAX_SAMPLES: usize = 60;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorSample {
    pub total_pss_kb: u64,
    pub java_heap_kb: u64,
    pub native_heap_kb: u64,
    pub cpu_percent: f32,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
}

impl MonitorState {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
    }

    pub fn latest(&self) -> Option<&MonitorSample> {
        self.history.last()
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

    pub fn net_throughput(&self) -> (u64, u64) {
        if self.history.len() < 2 {
            return (0, 0);
        }
        let curr = self.history.last().unwrap();
        let prev = &self.history[self.history.len() - 2];
        let rx = curr.net_rx_bytes.saturating_sub(prev.net_rx_bytes) / 5;
        let tx = curr.net_tx_bytes.saturating_sub(prev.net_tx_bytes) / 5;
        (rx, tx)
    }
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<MonitorSample> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(5);
        loop {
            let mut sample = MonitorSample::default();

            if let Ok(mem) = adb.get_meminfo(&serial, &package) {
                sample.total_pss_kb = mem.total_pss_kb;
                sample.java_heap_kb = mem.java_heap_kb;
                sample.native_heap_kb = mem.native_heap_kb;
            }

            if let Ok(cpu) = adb.get_cpu_usage(&serial, &package) {
                sample.cpu_percent = cpu;
            }

            if let Ok((rx_bytes, tx_bytes)) = adb.get_net_stats(&serial) {
                sample.net_rx_bytes = rx_bytes;
                sample.net_tx_bytes = tx_bytes;
            }

            if tx.send(sample).is_err() {
                return;
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(total: u64, java: u64, native: u64) -> MonitorSample {
        MonitorSample {
            total_pss_kb: total,
            java_heap_kb: java,
            native_heap_kb: native,
            ..Default::default()
        }
    }

    #[test]
    fn push_caps_at_max_samples() {
        let mut state = MonitorState::new();
        for i in 0..70 {
            state.push(sample(i, 0, 0));
        }
        assert_eq!(state.history.len(), MAX_SAMPLES);
        assert_eq!(state.latest().unwrap().total_pss_kb, 69);
    }

    #[test]
    fn trend_stable_with_few_samples() {
        let mut state = MonitorState::new();
        state.push(sample(100, 0, 0));
        state.push(sample(200, 0, 0));
        assert_eq!(state.trend_u64(|m| m.total_pss_kb), Trend::Stable);
    }

    #[test]
    fn trend_rising_when_increasing() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(100 + i * 20, 0, 0));
        }
        assert_eq!(state.trend_u64(|m| m.total_pss_kb), Trend::Rising);
    }

    #[test]
    fn trend_falling_when_decreasing() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(200 - i * 20, 0, 0));
        }
        assert_eq!(state.trend_u64(|m| m.total_pss_kb), Trend::Falling);
    }

    #[test]
    fn trend_stable_with_small_changes() {
        let mut state = MonitorState::new();
        for i in 0..5 {
            state.push(sample(1000 + i, 0, 0));
        }
        assert_eq!(state.trend_u64(|m| m.total_pss_kb), Trend::Stable);
    }

    #[test]
    fn sparkline_extracts_values() {
        let mut state = MonitorState::new();
        state.push(sample(100, 40, 50));
        state.push(sample(120, 45, 55));
        assert_eq!(state.sparkline_u64(|m| m.java_heap_kb), vec![40, 45]);
    }

    #[test]
    fn net_throughput_computes_delta() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            net_rx_bytes: 1000,
            net_tx_bytes: 500,
            ..Default::default()
        });
        state.push(MonitorSample {
            net_rx_bytes: 6000,
            net_tx_bytes: 3000,
            ..Default::default()
        });
        let (rx, tx) = state.net_throughput();
        assert_eq!(rx, 1000);
        assert_eq!(tx, 500);
    }

    #[test]
    fn net_throughput_zero_with_single_sample() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            net_rx_bytes: 1000,
            ..Default::default()
        });
        assert_eq!(state.net_throughput(), (0, 0));
    }
}
