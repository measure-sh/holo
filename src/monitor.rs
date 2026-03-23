use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::adb::Adb;

const MAX_SAMPLES: usize = 60;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorSample {
    pub rss_kb: u64,
    pub cpu_percent: f32,
    pub total_frames: u64,
    pub slow_frames: u64,
    pub frozen_frames: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

pub struct MonitorState {
    pub history: Vec<MonitorSample>,
    pub frame_count_history: Vec<u64>,
    pub slow_percent_history: Vec<f32>,
    pub frozen_percent_history: Vec<f32>,
}

impl MonitorState {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            frame_count_history: Vec::new(),
            slow_percent_history: Vec::new(),
            frozen_percent_history: Vec::new(),
        }
    }

    pub fn push(&mut self, sample: MonitorSample) {
        if let Some(prev) = self.history.last() {
            let frame_delta = sample.total_frames.saturating_sub(prev.total_frames);
            let slow_delta = sample.slow_frames.saturating_sub(prev.slow_frames);
            let frozen_delta = sample.frozen_frames.saturating_sub(prev.frozen_frames);
            self.frame_count_history.push(frame_delta);
            let pct = |delta: u64| -> f32 {
                if frame_delta > 0 { delta as f32 / frame_delta as f32 * 100.0 } else { 0.0 }
            };
            self.slow_percent_history.push(pct(slow_delta));
            self.frozen_percent_history.push(pct(frozen_delta));
        }
        self.history.push(sample);
        if self.history.len() > MAX_SAMPLES {
            self.history.remove(0);
        }
        if self.frame_count_history.len() > MAX_SAMPLES {
            self.frame_count_history.remove(0);
        }
        if self.slow_percent_history.len() > MAX_SAMPLES {
            self.slow_percent_history.remove(0);
        }
        if self.frozen_percent_history.len() > MAX_SAMPLES {
            self.frozen_percent_history.remove(0);
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
        let interval = Duration::from_secs(1);
        loop {
            let mut sample = MonitorSample::default();

            if let Ok(mem) = adb.get_meminfo(&serial, &package) {
                sample.rss_kb = mem.rss_kb;
            }

            if let Ok(cpu) = adb.get_cpu_usage(&serial, &package) {
                sample.cpu_percent = cpu;
            }

            if let Ok(gfx) = adb.get_gfx_info(&serial, &package) {
                sample.total_frames = gfx.total_frames;
                sample.slow_frames = gfx.slow_frames;
                sample.frozen_frames = gfx.frozen_frames;
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

    fn sample(rss: u64) -> MonitorSample {
        MonitorSample {
            rss_kb: rss,
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
        assert_eq!(state.latest().unwrap().rss_kb, 69);
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
    fn frame_history_computes_deltas() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            total_frames: 100,
            slow_frames: 10,
            frozen_frames: 2,
            ..Default::default()
        });
        state.push(MonitorSample {
            total_frames: 200,
            slow_frames: 30,
            frozen_frames: 5,
            ..Default::default()
        });
        assert_eq!(state.frame_count_history, vec![100]);
        assert_eq!(state.slow_percent_history, vec![20.0]);
        assert_eq!(state.frozen_percent_history, vec![3.0]);
    }

    #[test]
    fn frame_history_zero_delta_no_panic() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            total_frames: 50,
            slow_frames: 5,
            frozen_frames: 1,
            ..Default::default()
        });
        state.push(MonitorSample {
            total_frames: 50,
            slow_frames: 5,
            frozen_frames: 1,
            ..Default::default()
        });
        assert_eq!(state.frame_count_history, vec![0]);
        assert_eq!(state.slow_percent_history, vec![0.0]);
        assert_eq!(state.frozen_percent_history, vec![0.0]);
    }

    #[test]
    fn frame_history_empty_with_single_sample() {
        let mut state = MonitorState::new();
        state.push(MonitorSample {
            total_frames: 100,
            ..Default::default()
        });
        assert!(state.frame_count_history.is_empty());
        assert!(state.slow_percent_history.is_empty());
        assert!(state.frozen_percent_history.is_empty());
    }

}
