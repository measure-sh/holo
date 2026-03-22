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
    pub total_frames: u64,
    pub janky_frames: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
    pub download_history: Vec<u64>,
    pub upload_history: Vec<u64>,
    pub frame_count_history: Vec<u64>,
    pub janky_percent_history: Vec<f32>,
}

impl MonitorState {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            download_history: Vec::new(),
            upload_history: Vec::new(),
            frame_count_history: Vec::new(),
            janky_percent_history: Vec::new(),
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        if let Some(prev) = self.history.last() {
            self.download_history.push(sample.net_rx_bytes.saturating_sub(prev.net_rx_bytes));
            self.upload_history.push(sample.net_tx_bytes.saturating_sub(prev.net_tx_bytes));

            let frame_delta = sample.total_frames.saturating_sub(prev.total_frames);
            let jank_delta = sample.janky_frames.saturating_sub(prev.janky_frames);
            self.frame_count_history.push(frame_delta);
            self.janky_percent_history.push(if frame_delta > 0 {
                jank_delta as f32 / frame_delta as f32 * 100.0
            } else {
                0.0
            });
        }
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
        if self.download_history.len() > MAX_SAMPLES {
            self.download_history.remove(0);
        }
        if self.upload_history.len() > MAX_SAMPLES {
            self.upload_history.remove(0);
        }
        if self.frame_count_history.len() > MAX_SAMPLES {
            self.frame_count_history.remove(0);
        }
        if self.janky_percent_history.len() > MAX_SAMPLES {
            self.janky_percent_history.remove(0);
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

}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<MonitorSample> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(3);
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

            if let Ok(gfx) = adb.get_gfx_info(&serial, &package) {
                sample.total_frames = gfx.total_frames;
                sample.janky_frames = gfx.janky_frames;
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
    fn net_history_computes_deltas() {
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
        assert_eq!(state.download_history, vec![5000]);
        assert_eq!(state.upload_history, vec![2500]);
    }

    #[test]
    fn frame_history_computes_deltas() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            total_frames: 100,
            janky_frames: 10,
            ..Default::default()
        });
        state.push(MonitorSample {
            total_frames: 200,
            janky_frames: 30,
            ..Default::default()
        });
        assert_eq!(state.frame_count_history, vec![100]);
        assert_eq!(state.janky_percent_history, vec![20.0]);
    }

    #[test]
    fn frame_history_zero_delta_no_panic() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            total_frames: 50,
            janky_frames: 5,
            ..Default::default()
        });
        state.push(MonitorSample {
            total_frames: 50,
            janky_frames: 5,
            ..Default::default()
        });
        assert_eq!(state.frame_count_history, vec![0]);
        assert_eq!(state.janky_percent_history, vec![0.0]);
    }

    #[test]
    fn frame_history_empty_with_single_sample() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            total_frames: 100,
            ..Default::default()
        });
        assert!(state.frame_count_history.is_empty());
        assert!(state.janky_percent_history.is_empty());
    }

    #[test]
    fn net_history_empty_with_single_sample() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            net_rx_bytes: 1000,
            ..Default::default()
        });
        assert!(state.download_history.is_empty());
        assert!(state.upload_history.is_empty());
    }
}
