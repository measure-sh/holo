use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crate::adb::Adb;

pub struct TraceState {
    pub recording: bool,
    pub started_at: Option<std::time::Instant>,
    pub status_message: Option<String>,
    pub message_at: Option<std::time::Instant>,
    pub pulled_traces: Vec<PathBuf>,
}

impl TraceState {
    pub fn new() -> Self {
        Self {
            recording: false,
            started_at: None,
            status_message: None,
            message_at: None,
            pulled_traces: Vec::new(),
        }
    }
}

pub fn trace_config(package: &str) -> String {
    format!(
        r#"buffers {{
  size_kb: 262144
  fill_policy: RING_BUFFER
}}

buffers {{
  size_kb: 8192
  fill_policy: RING_BUFFER
}}

data_sources {{
  config {{
    name: "linux.ftrace"
    target_buffer: 0
    ftrace_config {{
      atrace_categories: "gfx"
      atrace_categories: "input"
      atrace_categories: "view"
      atrace_categories: "webview"
      atrace_categories: "wm"
      atrace_categories: "am"
      atrace_categories: "sm"
      atrace_categories: "audio"
      atrace_categories: "video"
      atrace_categories: "camera"
      atrace_categories: "hal"
      atrace_categories: "res"
      atrace_categories: "dalvik"
      atrace_categories: "rs"
      atrace_categories: "bionic"
      atrace_categories: "power"
      atrace_categories: "pm"
      atrace_categories: "ss"
      atrace_categories: "database"
      atrace_categories: "network"
      atrace_categories: "adb"
      atrace_categories: "vibrator"
      atrace_categories: "aidl"
      atrace_categories: "nnapi"
      atrace_categories: "core_services"
      atrace_categories: "pdx"
      atrace_apps: "{package}"
      ftrace_events: "sched/sched_switch"
      ftrace_events: "sched/sched_waking"
      ftrace_events: "sched/sched_wakeup_new"
      ftrace_events: "sched/sched_process_exec"
      ftrace_events: "sched/sched_process_exit"
      ftrace_events: "sched/sched_process_fork"
      ftrace_events: "sched/sched_process_free"
      ftrace_events: "task/task_newtask"
      ftrace_events: "task/task_rename"
      ftrace_events: "power/suspend_resume"
      ftrace_events: "power/cpu_frequency"
      ftrace_events: "power/cpu_idle"
      ftrace_events: "power/gpu_frequency"
      buffer_size_kb: 16384
      drain_period_ms: 250
    }}
  }}
}}

data_sources {{
  config {{
    name: "linux.process_stats"
    target_buffer: 1
    process_stats_config {{
      scan_all_processes_on_start: true
      proc_stats_poll_ms: 1000
    }}
  }}
}}

data_sources {{
  config {{
    name: "linux.sys_stats"
    target_buffer: 1
    sys_stats_config {{
      meminfo_period_ms: 1000
      vmstat_period_ms: 1000
      stat_period_ms: 1000
    }}
  }}
}}

duration_ms: 1800000
"#
    )
}

pub fn spawn_start_trace(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let config = trace_config(&package);
        let result = adb
            .start_trace(&serial, &config)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_stop_and_pull_trace(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<PathBuf, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let dest = std::env::temp_dir()
            .join("msh")
            .join(&package)
            .join("traces")
            .join(format!("{timestamp}_trace.perfetto-trace"));
        let result = adb
            .stop_and_pull_trace(&serial, &dest)
            .map(|_| dest)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}
