use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use crate::adb::Adb;

pub struct TraceState {
    pub recording: bool,
    pub started_at: Option<std::time::Instant>,
    pub status_message: Option<String>,
    pub message_at: Option<std::time::Instant>,
    pub pulled_traces: Vec<PathBuf>,
    pub selected_index: usize,
}

impl TraceState {
    pub fn new(package: &str) -> Self {
        let traces_dir = std::env::temp_dir()
            .join("msh")
            .join(package)
            .join("traces");
        let mut pulled_traces: Vec<PathBuf> = std::fs::read_dir(&traces_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("perfetto-trace"))
            .collect();
        pulled_traces.sort();
        Self {
            recording: false,
            started_at: None,
            status_message: None,
            message_at: None,
            pulled_traces,
            selected_index: 0,
        }
    }

    pub fn clamp_selection(&mut self) {
        if self.pulled_traces.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.pulled_traces.len() {
            self.selected_index = self.pulled_traces.len() - 1;
        }
    }

    pub fn delete_selected(&mut self) -> Option<PathBuf> {
        if self.pulled_traces.is_empty() {
            return None;
        }
        let idx = self.selected_index;
        let path = self.pulled_traces.remove(idx);
        self.clamp_selection();
        Some(path)
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.pulled_traces.get(self.selected_index)
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

pub fn open_in_perfetto_ui(path: &Path) {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let fname = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let listener = match TcpListener::bind("127.0.0.1:9001") {
            Ok(l) => l,
            Err(_) => return,
        };

        let url = format!(
            "https://ui.perfetto.dev/#!/?url=http://127.0.0.1:9001/{fname}"
        );
        let _ = open::that(&url);

        let trace_data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => return,
        };

        listener.set_nonblocking(false).ok();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };

            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);

            let requested_path = req.lines().next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("");

            if req.starts_with("OPTIONS") {
                let response = "HTTP/1.1 204 No Content\r\n\
                     Access-Control-Allow-Origin: https://ui.perfetto.dev\r\n\
                     Access-Control-Allow-Methods: GET\r\n\
                     Access-Control-Allow-Headers: *\r\n\
                     Content-Length: 0\r\n\r\n";
                let _ = stream.write_all(response.as_bytes());
                continue;
            }

            if requested_path != format!("/{fname}") {
                let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response.as_bytes());
                continue;
            }

            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Access-Control-Allow-Origin: https://ui.perfetto.dev\r\n\
                 Content-Type: application/octet-stream\r\n\
                 Cache-Control: no-cache\r\n\
                 Content-Length: {}\r\n\r\n",
                trace_data.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&trace_data);
            break;
        }
    });
}
