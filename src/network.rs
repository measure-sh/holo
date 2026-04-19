use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Adb;
use crate::app::Action;
use crate::logcat;

const MAX_ENTRIES: usize = 500;
const MAX_TRAFFIC: usize = 240;

#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficSample {
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub rx_total: u64,
    pub tx_total: u64,
}

pub struct NetworkEntry {
    pub url: String,
    pub method: String,
    pub status_code: u16,
    pub latency_ms: u64,
    pub failure_reason: Option<String>,
    pub failure_description: Option<String>,
    pub client: String,
    pub timestamp: String,
    pub request_headers: String,
    pub response_headers: String,
    pub request_body: String,
    pub response_body: String,
}

pub struct NetworkState {
    pub entries: Vec<NetworkEntry>,
    pub selected: usize,
    pub failure_count: usize,
    pub detail_open: bool,
    pub detail_focused: bool,
    pub detail_scroll: usize,
    pub search: String,
    pub editing_search: bool,
    pub traffic: Vec<TrafficSample>,
    pub traffic_baseline: Option<(u64, u64)>,
}

impl NetworkState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            failure_count: 0,
            detail_open: false,
            detail_focused: false,
            detail_scroll: 0,
            search: String::new(),
            editing_search: false,
            traffic: Vec::new(),
            traffic_baseline: None,
        }
    }

    pub fn push_traffic(&mut self, sample: TrafficSample) {
        if self.traffic_baseline.is_none() {
            self.traffic_baseline = Some((sample.rx_total, sample.tx_total));
        }
        self.traffic.push(sample);
        if self.traffic.len() > MAX_TRAFFIC {
            let drain = self.traffic.len() - MAX_TRAFFIC;
            self.traffic.drain(..drain);
        }
    }

    fn is_failure(entry: &NetworkEntry) -> bool {
        entry.status_code >= 400 || entry.failure_reason.is_some()
    }

    pub fn matches_search(entry: &NetworkEntry, search: &str) -> bool {
        if search.is_empty() {
            return true;
        }
        let lower = search.to_lowercase();
        entry.url.to_lowercase().contains(&lower)
            || entry.method.to_lowercase().contains(&lower)
            || entry.status_code.to_string().contains(&lower)
    }

    pub fn filtered_entries(&self) -> Vec<(usize, &NetworkEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| Self::matches_search(e, &self.search))
            .collect()
    }

    fn close_detail(&mut self) {
        self.detail_open = false;
        self.detail_focused = false;
        self.detail_scroll = 0;
    }

    pub fn clamp_detail_scroll(&mut self, total_lines: usize, visible_height: usize) {
        let max = total_lines.saturating_sub(visible_height);
        self.detail_scroll = self.detail_scroll.min(max);
    }

    pub fn push(&mut self, entry: NetworkEntry) {
        let at_last = self.entries.is_empty() || self.selected == self.entries.len() - 1;
        if Self::is_failure(&entry) {
            self.failure_count += 1;
        }
        self.entries.push(entry);
        if self.entries.len() > MAX_ENTRIES {
            let drained: Vec<_> = self.entries.drain(..self.entries.len() - MAX_ENTRIES).collect();
            for e in &drained {
                if Self::is_failure(e) {
                    self.failure_count = self.failure_count.saturating_sub(1);
                }
            }
        }
        if at_last {
            self.selected = self.entries.len() - 1;
        } else if !self.entries.is_empty() {
            self.selected = self.selected.min(self.entries.len() - 1);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.editing_search {
            match key.code {
                KeyCode::Char(c) => self.search.push(c),
                KeyCode::Backspace => { self.search.pop(); }
                KeyCode::Enter | KeyCode::Esc => self.editing_search = false,
                _ => {}
            }
            return Some(Action::Noop);
        }

        if self.detail_open && self.detail_focused {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    Some(Action::Noop)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.detail_scroll += 1;
                    Some(Action::Noop)
                }
                KeyCode::Char(' ') => {
                    self.detail_scroll += 20;
                    Some(Action::Noop)
                }
                KeyCode::Tab => {
                    self.detail_focused = false;
                    Some(Action::Noop)
                }
                KeyCode::Char('o') => {
                    self.entries.get(self.selected).map(|entry| {
                        Action::OpenInEditor(Self::format_detail(entry))
                    })
                }
                KeyCode::Enter => {
                    self.close_detail();
                    Some(Action::Noop)
                }
                KeyCode::Esc => {
                    self.detail_focused = false;
                    Some(Action::Noop)
                }
                _ => Some(Action::Noop),
            };
        }

        if self.detail_open {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                    self.detail_scroll = 0;
                    Some(Action::Noop)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !self.entries.is_empty() {
                        self.selected = (self.selected + 1).min(self.entries.len() - 1);
                        self.detail_scroll = 0;
                    }
                    Some(Action::Noop)
                }
                KeyCode::Tab => {
                    self.detail_focused = true;
                    Some(Action::Noop)
                }
                KeyCode::Char('o') => {
                    self.entries.get(self.selected).map(|entry| {
                        Action::OpenInEditor(Self::format_detail(entry))
                    })
                }
                KeyCode::Enter => {
                    self.close_detail();
                    Some(Action::Noop)
                }
                KeyCode::Esc => {
                    self.close_detail();
                    Some(Action::Unfocus)
                }
                _ => Some(Action::Noop),
            };
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.entries.is_empty() {
                    self.selected = (self.selected + 1).min(self.entries.len() - 1);
                }
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if self.entries.get(self.selected).is_some() {
                    self.detail_open = true;
                    Some(Action::ZoomIn)
                } else {
                    Some(Action::Noop)
                }
            }
            KeyCode::Char('o') => {
                self.entries.get(self.selected).map(|entry| {
                    Action::OpenInEditor(Self::format_detail(entry))
                })
            }
            KeyCode::Char('/') => {
                self.editing_search = true;
                Some(Action::Noop)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }

    fn format_detail(entry: &NetworkEntry) -> String {
        let req_headers = format_headers_text(&entry.request_headers);
        let resp_headers = format_headers_text(&entry.response_headers);
        let req_body = format_body_text(&entry.request_body);
        let resp_body = format_body_text(&entry.response_body);
        let mut text = format!(
            "{method} {url}\nStatus: {status} | Latency: {latency} | Client: {client}",
            method = entry.method.to_uppercase(),
            url = entry.url,
            status = entry.status_code,
            latency = crate::network_ui::format_latency(entry.latency_ms),
            client = entry.client,
        );
        if let Some(reason) = &entry.failure_reason {
            text.push_str(&format!("\nFailure: {reason}"));
        }
        if let Some(desc) = &entry.failure_description {
            text.push_str(&format!("\nDescription: {desc}"));
        }
        text.push_str(&format!(
            "\n\n--- Request Headers ---\n{req_headers}\n\n\
             --- Response Headers ---\n{resp_headers}\n\n\
             --- Request Body ---\n{req_body}\n\n\
             --- Response Body ---\n{resp_body}",
        ));
        text
    }
}

// Parsing

const FIELD_KEYS: &[&str] = &[
    "url=",
    "method=",
    "status_code=",
    "start_time=",
    "end_time=",
    "failure_reason=",
    "failure_description=",
    "request_headers=",
    "response_headers=",
    "request_body=",
    "response_body=",
    "client=",
];

fn is_empty_value(value: &str) -> bool {
    value.is_empty() || value == "null" || value == "{}"
}

/// Split header pairs on `, ` but only when the next segment contains `=`,
/// so values like `Wed, 15 Apr 2026 09:22:25 GMT` stay intact.
fn split_header_pairs(input: &str) -> Vec<&str> {
    let mut pairs = Vec::new();
    let mut start = 0;
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if i + 2 <= len && &input[i..i + 2] == ", " {
            let rest = &input[i + 2..];
            if rest.contains('=') && rest.find('=') < rest.find(", ") {
                pairs.push(input[start..i].trim());
                start = i + 2;
            }
        }
        i += 1;
    }
    let last = input[start..].trim();
    if !last.is_empty() {
        pairs.push(last);
    }
    pairs
}

/// Format headers from `{key=val, key=val}` into `key: val` lines for editor export.
fn format_headers_text(raw: &str) -> String {
    if is_empty_value(raw) {
        return "(empty)".to_string();
    }
    let inner = raw.trim().strip_prefix('{').unwrap_or(raw);
    let inner = inner.strip_suffix('}').unwrap_or(inner).trim();
    if inner.is_empty() {
        return "(empty)".to_string();
    }
    split_header_pairs(inner)
        .iter()
        .map(|pair| {
            if let Some((k, v)) = pair.split_once('=') {
                format!("{k}: {v}")
            } else {
                pair.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Pretty-print JSON body, or return raw text.
fn format_body_text(raw: &str) -> String {
    if is_empty_value(raw) {
        return "(empty)".to_string();
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw)
        && let Ok(pretty) = serde_json::to_string_pretty(&json)
    {
        return pretty;
    }
    raw.to_string()
}

fn find_field_value<'a>(data: &'a str, key: &str, next_key: Option<&str>) -> Option<&'a str> {
    let start = data.find(key)? + key.len();
    let end = if let Some(nk) = next_key {
        // Find the next key, then back up past the ", " separator
        data[start..].find(nk).map(|i| start + i - 2)?
    } else {
        data.len()
    };
    if end <= start {
        return Some("");
    }
    Some(data[start..end].trim())
}

fn parse_optional(value: &str) -> Option<String> {
    if value == "null" { None } else { Some(value.to_string()) }
}

fn is_localhost_url(url: &str) -> bool {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1" | "10.0.2.2" | "10.0.3.2")
}

pub fn parse_http_data(line: &str) -> Option<NetworkEntry> {
    let parsed = logcat::parse(line)?;
    if parsed.tag != "Measure" {
        return None;
    }
    let msg = parsed.message;
    let marker = "EventProcessor: HTTP, HttpData(";
    let start = msg.find(marker)? + marker.len();
    let end = msg.rfind(')')?;
    if end <= start {
        return None;
    }
    let data = &msg[start..end];

    let url = find_field_value(data, FIELD_KEYS[0], Some(FIELD_KEYS[1]))?.to_string();
    if is_localhost_url(&url) {
        return None;
    }
    let method = find_field_value(data, FIELD_KEYS[1], Some(FIELD_KEYS[2]))?.to_string();
    let status_str = find_field_value(data, FIELD_KEYS[2], Some(FIELD_KEYS[3]))?;
    let start_time_str = find_field_value(data, FIELD_KEYS[3], Some(FIELD_KEYS[4]))?;
    let end_time_str = find_field_value(data, FIELD_KEYS[4], Some(FIELD_KEYS[5]))?;
    let failure_reason_str = find_field_value(data, FIELD_KEYS[5], Some(FIELD_KEYS[6]))?;
    let failure_desc_str = find_field_value(data, FIELD_KEYS[6], Some(FIELD_KEYS[7]))?;
    let request_headers = find_field_value(data, FIELD_KEYS[7], Some(FIELD_KEYS[8]))?.to_string();
    let response_headers = find_field_value(data, FIELD_KEYS[8], Some(FIELD_KEYS[9]))?.to_string();
    let request_body = find_field_value(data, FIELD_KEYS[9], Some(FIELD_KEYS[10]))?.to_string();
    let response_body = find_field_value(data, FIELD_KEYS[10], Some(FIELD_KEYS[11]))?.to_string();
    let client = find_field_value(data, FIELD_KEYS[11], None)?.to_string();

    let status_code: u16 = status_str.parse().unwrap_or(0);
    let start_time: u64 = start_time_str.parse().unwrap_or(0);
    let end_time: u64 = end_time_str.parse().unwrap_or(0);
    let latency_ms = end_time.saturating_sub(start_time);

    Some(NetworkEntry {
        url,
        method,
        status_code,
        latency_ms,
        failure_reason: parse_optional(failure_reason_str),
        failure_description: parse_optional(failure_desc_str),
        client,
        timestamp: parsed.timestamp.to_string(),
        request_headers,
        response_headers,
        request_body,
        response_body,
    })
}

pub fn spawn_traffic_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<TrafficSample> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(2);
        let mut prev: Option<(u64, u64, Instant)> = None;
        loop {
            let now = Instant::now();
            if let Ok(bytes) = adb.get_network_bytes(&serial, &package) {
                if let Some((p_rx, p_tx, p_at)) = prev {
                    let elapsed = now.duration_since(p_at).as_secs_f64().max(0.001);
                    let rx_bps = ((bytes.rx.saturating_sub(p_rx)) as f64 / elapsed) as u64;
                    let tx_bps = ((bytes.tx.saturating_sub(p_tx)) as f64 / elapsed) as u64;
                    let sample = TrafficSample {
                        rx_bps,
                        tx_bps,
                        rx_total: bytes.rx,
                        tx_total: bytes.tx,
                    };
                    if tx.send(sample).is_err() {
                        return;
                    }
                }
                prev = Some((bytes.rx, bytes.tx, now));
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    const SAMPLE_LINE: &str = "04-08 09:17:37.261 23922 23938 D Measure : EventProcessor: HTTP, HttpData(url=https://postman-echo.com/status/200, method=get, status_code=200, start_time=54525376, end_time=54525792, failure_reason=null, failure_description=null, request_headers={}, response_headers={}, request_body=null, response_body=null, client=okhttp)";

    #[test]
    fn parse_valid_line() {
        let entry = parse_http_data(SAMPLE_LINE).unwrap();
        assert_eq!(entry.url, "https://postman-echo.com/status/200");
        assert_eq!(entry.method, "get");
        assert_eq!(entry.status_code, 200);
        assert_eq!(entry.latency_ms, 416);
        assert!(entry.failure_reason.is_none());
        assert_eq!(entry.client, "okhttp");
        assert_eq!(entry.timestamp, "09:17:37.261");
        assert_eq!(entry.request_headers, "{}");
        assert_eq!(entry.response_headers, "{}");
        assert_eq!(entry.request_body, "null");
        assert_eq!(entry.response_body, "null");
    }

    #[test]
    fn parse_non_measure_line() {
        let line = "04-08 09:17:37.261 23922 23938 D OkHttp  : some other log";
        assert!(parse_http_data(line).is_none());
    }

    #[test]
    fn parse_non_http_measure_line() {
        let line = "04-08 09:17:37.261 23922 23938 D Measure : EventProcessor: Lifecycle, something";
        assert!(parse_http_data(line).is_none());
    }

    #[test]
    fn parse_malformed_line() {
        let line = "not a logcat line at all";
        assert!(parse_http_data(line).is_none());
    }

    #[test]
    fn parse_drops_localhost() {
        let line = "04-08 09:17:37.261 23922 23938 D Measure : EventProcessor: HTTP, HttpData(url=http://localhost:8081/message?device=Pixel&app=com.foo.debug&clientid=BridgelessDevSupportManager, method=get, status_code=200, start_time=1, end_time=2, failure_reason=null, failure_description=null, request_headers={}, response_headers={}, request_body=null, response_body=null, client=okhttp)";
        assert!(parse_http_data(line).is_none());
    }

    #[test]
    fn parse_drops_loopback_ip() {
        let line = "04-08 09:17:37.261 23922 23938 D Measure : EventProcessor: HTTP, HttpData(url=http://127.0.0.1:8081/inspector/device, method=get, status_code=200, start_time=1, end_time=2, failure_reason=null, failure_description=null, request_headers={}, response_headers={}, request_body=null, response_body=null, client=okhttp)";
        assert!(parse_http_data(line).is_none());
    }

    #[test]
    fn parse_drops_emulator_host() {
        let line = "04-08 09:17:37.261 23922 23938 D Measure : EventProcessor: HTTP, HttpData(url=http://10.0.2.2:3000/api, method=get, status_code=200, start_time=1, end_time=2, failure_reason=null, failure_description=null, request_headers={}, response_headers={}, request_body=null, response_body=null, client=okhttp)";
        assert!(parse_http_data(line).is_none());
    }

    #[test]
    fn parse_keeps_non_localhost_urls() {
        let line = "04-08 09:17:37.261 23922 23938 D Measure : EventProcessor: HTTP, HttpData(url=https://api.example.com/inspector/devices/42, method=get, status_code=200, start_time=1, end_time=2, failure_reason=null, failure_description=null, request_headers={}, response_headers={}, request_body=null, response_body=null, client=okhttp)";
        let entry = parse_http_data(line).unwrap();
        assert_eq!(entry.url, "https://api.example.com/inspector/devices/42");
    }

    #[test]
    fn state_entry_count() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.push(make_entry(404, 50));
        assert_eq!(state.entries.len(), 2);
    }

    #[test]
    fn cursor_navigation() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.push(make_entry(200, 200));
        state.push(make_entry(200, 300));
        // auto-follow puts cursor at last
        assert_eq!(state.selected, 2);
        state.handle_key(key(KeyCode::Char('k')));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Char('k')));
        assert_eq!(state.selected, 0);
        state.handle_key(key(KeyCode::Char('k')));
        assert_eq!(state.selected, 0); // clamped
        state.handle_key(key(KeyCode::Char('j')));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn cursor_clamps_at_end() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.push(make_entry(200, 200));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Char('j')));
        assert_eq!(state.selected, 1); // can't go past end
    }

    #[test]
    fn enter_opens_detail_and_zooms() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        let action = state.handle_key(key(KeyCode::Enter));
        assert!(state.detail_open);
        assert!(matches!(action, Some(Action::ZoomIn)));
    }

    #[test]
    fn enter_noop_on_empty() {
        let mut state = NetworkState::new();
        let action = state.handle_key(key(KeyCode::Enter));
        assert!(!state.detail_open);
        assert!(matches!(action, Some(Action::Noop)));
    }

    #[test]
    fn esc_in_detail_closes_and_unfocuses() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.detail_open = true;
        let action = state.handle_key(key(KeyCode::Esc));
        assert!(!state.detail_open);
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn esc_in_list_unfocuses() {
        let mut state = NetworkState::new();
        let action = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn auto_follow_when_at_last() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        assert_eq!(state.selected, 0);
        state.push(make_entry(200, 200));
        assert_eq!(state.selected, 1); // followed
        state.push(make_entry(200, 300));
        assert_eq!(state.selected, 2); // followed
    }

    #[test]
    fn push_traffic_caps_at_max() {
        let mut state = NetworkState::new();
        for i in 0..(MAX_TRAFFIC + 50) {
            state.push_traffic(TrafficSample {
                rx_bps: i as u64,
                tx_bps: 0,
                rx_total: 0,
                tx_total: 0,
            });
        }
        assert_eq!(state.traffic.len(), MAX_TRAFFIC);
        assert_eq!(state.traffic.first().unwrap().rx_bps, 50);
        assert_eq!(state.traffic.last().unwrap().rx_bps, (MAX_TRAFFIC + 49) as u64);
    }

    #[test]
    fn push_traffic_records_baseline() {
        let mut state = NetworkState::new();
        assert_eq!(state.traffic_baseline, None);
        state.push_traffic(TrafficSample {
            rx_bps: 0,
            tx_bps: 0,
            rx_total: 1000,
            tx_total: 500,
        });
        assert_eq!(state.traffic_baseline, Some((1000, 500)));
    }

    #[test]
    fn push_traffic_keeps_first_baseline() {
        let mut state = NetworkState::new();
        state.push_traffic(TrafficSample {
            rx_bps: 0,
            tx_bps: 0,
            rx_total: 1000,
            tx_total: 500,
        });
        state.push_traffic(TrafficSample {
            rx_bps: 0,
            tx_bps: 0,
            rx_total: 2000,
            tx_total: 1500,
        });
        assert_eq!(state.traffic_baseline, Some((1000, 500)));
    }

    #[test]
    fn push_traffic_baseline_survives_buffer_rotation() {
        let mut state = NetworkState::new();
        for i in 0..(MAX_TRAFFIC + 10) {
            state.push_traffic(TrafficSample {
                rx_bps: 0,
                tx_bps: 0,
                rx_total: 1000 + i as u64,
                tx_total: 500 + i as u64,
            });
        }
        assert_eq!(state.traffic_baseline, Some((1000, 500)));
    }

    #[test]
    fn no_follow_when_scrolled_up() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.push(make_entry(200, 200));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Char('k'))); // move up
        assert_eq!(state.selected, 0);
        state.push(make_entry(200, 300));
        assert_eq!(state.selected, 0); // stayed put
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_entry(status: u16, latency: u64) -> NetworkEntry {
        NetworkEntry {
            url: "https://example.com".into(),
            method: "GET".into(),
            status_code: status,
            latency_ms: latency,
            failure_reason: None,
            failure_description: None,
            client: "okhttp".into(),
            timestamp: "09:17:37.261".into(),
            request_headers: String::new(),
            response_headers: String::new(),
            request_body: String::new(),
            response_body: String::new(),
        }
    }
}
