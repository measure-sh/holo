use std::sync::{mpsc, Arc};

use crossterm::event::KeyCode;

use crate::adb::Adb;
use crate::app::Action;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IssuesTab {
    Crashes,
    Anrs,
}

pub struct CrashEntry {
    pub timestamp: String,
    pub process: String,
    pub exception: String,
    pub full_text: String,
}

pub struct AnrEntry {
    pub timestamp: String,
    pub process: String,
    pub reason: String,
    pub full_text: String,
}

pub struct IssuesState {
    pub active_tab: IssuesTab,
    pub crashes: Vec<CrashEntry>,
    pub anrs: Vec<AnrEntry>,
    pub crash_selected: usize,
    pub anr_selected: usize,
    pub viewing_detail: bool,
    pub error: Option<String>,
}

impl IssuesState {
    pub fn new() -> Self {
        Self {
            active_tab: IssuesTab::Crashes,
            crashes: Vec::new(),
            anrs: Vec::new(),
            crash_selected: 0,
            anr_selected: 0,
            viewing_detail: false,
            error: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<Action> {
        match code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.active_tab = IssuesTab::Crashes;
                Some(Action::Noop)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.active_tab = IssuesTab::Anrs;
                Some(Action::Noop)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if self.has_entries() {
                    self.viewing_detail = !self.viewing_detail;
                }
                Some(Action::Noop)
            }
            KeyCode::Esc => {
                if self.viewing_detail {
                    self.viewing_detail = false;
                    Some(Action::Noop)
                } else {
                    Some(Action::Unfocus)
                }
            }
            _ => None,
        }
    }

    fn move_up(&mut self) {
        match self.active_tab {
            IssuesTab::Crashes => {
                self.crash_selected = self.crash_selected.saturating_sub(1);
            }
            IssuesTab::Anrs => {
                self.anr_selected = self.anr_selected.saturating_sub(1);
            }
        }
        self.viewing_detail = false;
    }

    fn move_down(&mut self) {
        match self.active_tab {
            IssuesTab::Crashes => {
                if !self.crashes.is_empty() {
                    self.crash_selected = (self.crash_selected + 1).min(self.crashes.len() - 1);
                }
            }
            IssuesTab::Anrs => {
                if !self.anrs.is_empty() {
                    self.anr_selected = (self.anr_selected + 1).min(self.anrs.len() - 1);
                }
            }
        }
        self.viewing_detail = false;
    }

    fn has_entries(&self) -> bool {
        match self.active_tab {
            IssuesTab::Crashes => !self.crashes.is_empty(),
            IssuesTab::Anrs => !self.anrs.is_empty(),
        }
    }
}

pub fn parse_crashes(output: &str, package: &str) -> Vec<CrashEntry> {
    parse_dropbox_entries(output, "data_app_crash", package)
        .into_iter()
        .map(|(timestamp, body)| {
            let process = extract_field(&body, "Process: ").unwrap_or_default();
            let exception = extract_exception(&body);
            CrashEntry {
                timestamp,
                process,
                exception,
                full_text: body,
            }
        })
        .collect()
}

pub fn parse_anrs(output: &str, package: &str) -> Vec<AnrEntry> {
    parse_dropbox_entries(output, "data_app_anr", package)
        .into_iter()
        .map(|(timestamp, body)| {
            let process = extract_field(&body, "Process: ").unwrap_or_default();
            let reason = extract_field(&body, "Subject: ")
                .or_else(|| extract_field(&body, "Reason: "))
                .unwrap_or_default();
            AnrEntry {
                timestamp,
                process,
                reason,
                full_text: body,
            }
        })
        .collect()
}

fn parse_dropbox_entries(output: &str, tag: &str, package: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut current_timestamp = String::new();
    let mut current_body = String::new();
    let prefix = format!(" {tag} ");

    for line in output.lines() {
        if line.len() >= 19 && line[19..].starts_with(&prefix) {
            if !current_timestamp.is_empty() && !current_body.is_empty() {
                entries.push((current_timestamp.clone(), current_body.clone()));
            }
            current_timestamp = line[..19].to_string();
            current_body.clear();
        } else if !current_timestamp.is_empty() {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }
    if !current_timestamp.is_empty() && !current_body.is_empty() {
        entries.push((current_timestamp, current_body));
    }

    if package.is_empty() {
        return entries;
    }

    entries
        .into_iter()
        .filter(|(_, body)| {
            body.lines().any(|l| {
                l.trim()
                    .strip_prefix("Process: ")
                    .map_or(false, |p| p.trim() == package)
            })
        })
        .collect()
}

fn extract_field(body: &str, prefix: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.trim().strip_prefix(prefix).map(|v| v.trim().to_string()))
}

fn extract_exception(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Exception:") || trimmed.contains("Error:") {
            return trimmed.to_string();
        }
    }
    String::new()
}

pub fn spawn_crash_loader(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<Vec<CrashEntry>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .get_dropbox_crashes(&serial)
            .map(|output| parse_crashes(&output, &package))
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_anr_loader(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<Vec<AnrEntry>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .get_dropbox_anrs(&serial)
            .map(|output| parse_anrs(&output, &package))
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_defaults() {
        let state = IssuesState::new();
        assert_eq!(state.active_tab, IssuesTab::Crashes);
        assert!(state.crashes.is_empty());
        assert!(state.anrs.is_empty());
        assert!(!state.viewing_detail);
    }

    #[test]
    fn h_l_switches_tabs() {
        let mut state = IssuesState::new();
        state.handle_key(KeyCode::Char('l'));
        assert_eq!(state.active_tab, IssuesTab::Anrs);
        state.handle_key(KeyCode::Char('h'));
        assert_eq!(state.active_tab, IssuesTab::Crashes);
    }

    #[test]
    fn navigate_crashes() {
        let mut state = IssuesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), process: "p".into(), exception: "e1".into(), full_text: "".into() },
            CrashEntry { timestamp: "t2".into(), process: "p".into(), exception: "e2".into(), full_text: "".into() },
        ];
        assert_eq!(state.crash_selected, 0);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.crash_selected, 1);
        state.handle_key(KeyCode::Char('j'));
        assert_eq!(state.crash_selected, 1);
        state.handle_key(KeyCode::Char('k'));
        assert_eq!(state.crash_selected, 0);
    }

    #[test]
    fn enter_toggles_detail() {
        let mut state = IssuesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), process: "p".into(), exception: "e".into(), full_text: "full".into() },
        ];
        assert!(!state.viewing_detail);
        state.handle_key(KeyCode::Enter);
        assert!(state.viewing_detail);
        state.handle_key(KeyCode::Enter);
        assert!(!state.viewing_detail);
    }

    #[test]
    fn esc_closes_detail_first() {
        let mut state = IssuesState::new();
        state.crashes = vec![
            CrashEntry { timestamp: "t1".into(), process: "p".into(), exception: "e".into(), full_text: "full".into() },
        ];
        state.handle_key(KeyCode::Enter);
        assert!(state.viewing_detail);
        let action = state.handle_key(KeyCode::Esc);
        assert!(!state.viewing_detail);
        assert!(matches!(action, Some(Action::Noop)));
    }

    #[test]
    fn esc_unfocuses_when_no_detail() {
        let mut state = IssuesState::new();
        let action = state.handle_key(KeyCode::Esc);
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn enter_noop_on_empty() {
        let mut state = IssuesState::new();
        state.handle_key(KeyCode::Enter);
        assert!(!state.viewing_detail);
    }

    #[test]
    fn parse_crashes_extracts_entries() {
        let output = "\
2024-01-15 10:23:45 data_app_crash (text, 1234 bytes)
Process: com.example.app
PID: 12345
java.lang.NullPointerException: Attempt to invoke
    at com.example.app.Main.onCreate(Main.java:42)

2024-01-15 11:00:00 data_app_crash (text, 567 bytes)
Process: com.other.app
java.lang.RuntimeException: boom
    at com.other.app.Foo.bar(Foo.java:10)
";
        let entries = parse_crashes(output, "com.example.app");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].timestamp, "2024-01-15 10:23:45");
        assert_eq!(entries[0].process, "com.example.app");
        assert!(entries[0].exception.contains("NullPointerException"));
    }

    #[test]
    fn parse_crashes_empty_package_returns_all() {
        let output = "\
2024-01-15 10:23:45 data_app_crash (text, 100 bytes)
Process: com.example.app
java.lang.NullPointerException: test

2024-01-15 11:00:00 data_app_crash (text, 100 bytes)
Process: com.other.app
java.lang.RuntimeException: test
";
        let entries = parse_crashes(output, "");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn parse_crashes_empty_output() {
        let entries = parse_crashes("", "com.example.app");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_anrs_extracts_entries() {
        let output = "\
2024-01-15 10:23:45 data_app_anr (text, 500 bytes)
Process: com.example.app
Subject: Input dispatching timed out
PID: 12345
";
        let entries = parse_anrs(output, "com.example.app");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].timestamp, "2024-01-15 10:23:45");
        assert_eq!(entries[0].reason, "Input dispatching timed out");
    }

    #[test]
    fn parse_anrs_no_match() {
        let output = "\
2024-01-15 10:23:45 data_app_anr (text, 500 bytes)
Process: com.other.app
Subject: ANR reason
";
        let entries = parse_anrs(output, "com.example.app");
        assert!(entries.is_empty());
    }
}
