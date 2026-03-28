use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Adb;
use crate::app::Action;

type FileMap = Arc<Mutex<HashMap<String, PathBuf>>>;

pub struct TraceState {
    pub recording: bool,
    pub started_at: Option<std::time::Instant>,
    pub status_message: Option<String>,
    pub message_at: Option<std::time::Instant>,
    pub pulled_traces: Vec<PathBuf>,
    pub selected_index: usize,
    server: Option<TraceServer>,
}

struct TraceServer {
    port: u16,
    files: FileMap,
    shutdown: Arc<AtomicBool>,
}

impl Drop for TraceServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(format!("127.0.0.1:{}", self.port));
    }
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
            server: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;
        match code {
            KeyCode::Char('s') => {
                if self.recording {
                    self.recording = false;
                    self.started_at = None;
                    Some(Action::StopTrace)
                } else {
                    self.recording = true;
                    self.started_at = Some(std::time::Instant::now());
                    self.status_message = None;
                    self.message_at = None;
                    Some(Action::StartTrace)
                }
            }
            KeyCode::Up | KeyCode::Char('k') if !self.recording => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') if !self.recording => {
                let max = self.pulled_traces.len().saturating_sub(1);
                if self.selected_index < max {
                    self.selected_index += 1;
                }
                Some(Action::Noop)
            }
            KeyCode::Enter if !self.recording => {
                if let Some(path) = self.selected_path().cloned() {
                    self.open_trace(&path);
                }
                Some(Action::Noop)
            }
            KeyCode::Char('d') if !self.recording => {
                if let Some(path) = self.delete_selected() {
                    let _ = std::fs::remove_file(&path);
                }
                Some(Action::Noop)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
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

    pub fn open_trace(&mut self, path: &Path) {
        if self.server.is_none() {
            self.server = start_trace_server();
        }
        if let Some(server) = &self.server {
            let fname = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            server.files.lock().unwrap().insert(fname.clone(), path.to_path_buf());
            let url = format!(
                "https://ui.perfetto.dev/#!/?url=http://127.0.0.1:{}/{fname}",
                server.port
            );
            let _ = open::that(&url);
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

fn start_trace_server() -> Option<TraceServer> {
    let listener = TcpListener::bind("127.0.0.1:9001").ok()?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let flag = shutdown.clone();
    let files: FileMap = Arc::new(Mutex::new(HashMap::new()));
    let server_files = files.clone();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if flag.load(Ordering::Relaxed) {
                return;
            }

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

            let fname = requested_path.trim_start_matches('/');
            let file_path = server_files.lock().unwrap().get(fname).cloned();

            match file_path.and_then(|p| std::fs::read(&p).ok()) {
                Some(data) => {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n\
                         Access-Control-Allow-Origin: https://ui.perfetto.dev\r\n\
                         Content-Type: application/octet-stream\r\n\
                         Cache-Control: no-cache\r\n\
                         Content-Length: {}\r\n\r\n",
                        data.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.write_all(&data);
                }
                None => {
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        }
    });

    Some(TraceServer { port: 9001, files, shutdown })
}
