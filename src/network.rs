use crossterm::event::{KeyCode, KeyEvent};

use crate::app::Action;
use crate::logcat;

const MAX_ENTRIES: usize = 500;

#[allow(dead_code)]
pub struct NetworkEntry {
    pub url: String,
    pub method: String,
    pub status_code: u16,
    pub latency_ms: u64,
    pub failure_reason: Option<String>,
    pub client: String,
    pub timestamp: String,
}

pub struct NetworkState {
    pub entries: Vec<NetworkEntry>,
    pub scroll: usize,
    pub wrap: bool,
    pub failure_count: usize,
}

impl NetworkState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scroll: 0,
            wrap: false,
            failure_count: 0,
        }
    }

    fn is_failure(entry: &NetworkEntry) -> bool {
        entry.status_code >= 400 || entry.failure_reason.is_some()
    }

    pub fn push(&mut self, entry: NetworkEntry) {
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
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll += 1;
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Char(' ') => {
                self.scroll += 20;
                Some(Action::Noop)
            }
            KeyCode::Char('w') => {
                self.wrap = !self.wrap;
                Some(Action::Noop)
            }
            KeyCode::Esc if self.scroll > 0 => {
                self.scroll = 0;
                Some(Action::Noop)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }

    pub fn clamp_scroll(&mut self, total: usize, visible_height: usize) {
        let max = total.saturating_sub(visible_height);
        self.scroll = self.scroll.min(max);
    }

    pub fn adjust_scroll_for_new_lines(&mut self, count: usize) {
        if self.scroll > 0 {
            self.scroll += count;
        }
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
    let method = find_field_value(data, FIELD_KEYS[1], Some(FIELD_KEYS[2]))?.to_string();
    let status_str = find_field_value(data, FIELD_KEYS[2], Some(FIELD_KEYS[3]))?;
    let start_time_str = find_field_value(data, FIELD_KEYS[3], Some(FIELD_KEYS[4]))?;
    let end_time_str = find_field_value(data, FIELD_KEYS[4], Some(FIELD_KEYS[5]))?;
    let failure_reason_str = find_field_value(data, FIELD_KEYS[5], Some(FIELD_KEYS[6]))?;
    let _failure_desc = find_field_value(data, FIELD_KEYS[6], Some(FIELD_KEYS[7]))?;
    let _req_headers = find_field_value(data, FIELD_KEYS[7], Some(FIELD_KEYS[8]))?;
    let _resp_headers = find_field_value(data, FIELD_KEYS[8], Some(FIELD_KEYS[9]))?;
    let _req_body = find_field_value(data, FIELD_KEYS[9], Some(FIELD_KEYS[10]))?;
    let _resp_body = find_field_value(data, FIELD_KEYS[10], Some(FIELD_KEYS[11]))?;
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
        client,
        timestamp: parsed.timestamp.to_string(),
    })
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
    fn state_entry_count() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.push(make_entry(404, 50));
        assert_eq!(state.entries.len(), 2);
    }

    #[test]
    fn handle_key_navigation() {
        let mut state = NetworkState::new();
        state.push(make_entry(200, 100));
        state.push(make_entry(200, 200));

        assert_eq!(state.scroll, 0);
        state.handle_key(key(KeyCode::Char('k')));
        assert_eq!(state.scroll, 1);
        state.handle_key(key(KeyCode::Char('k')));
        assert_eq!(state.scroll, 2);
        state.handle_key(key(KeyCode::Char('j')));
        assert_eq!(state.scroll, 1);
    }

    #[test]
    fn handle_key_space_scrolls_page() {
        let mut state = NetworkState::new();
        state.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(state.scroll, 20);
    }

    #[test]
    fn handle_key_esc_returns_to_bottom_first() {
        let mut state = NetworkState::new();
        state.scroll = 5;
        let action = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Noop)));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn handle_key_esc_unfocuses_at_bottom() {
        let mut state = NetworkState::new();
        let action = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn clamp_scroll_limits_to_max() {
        let mut state = NetworkState::new();
        state.scroll = 100;
        state.clamp_scroll(50, 20);
        assert_eq!(state.scroll, 30);
    }

    #[test]
    fn adjust_scroll_increments_when_scrolled() {
        let mut state = NetworkState::new();
        state.scroll = 5;
        state.adjust_scroll_for_new_lines(3);
        assert_eq!(state.scroll, 8);
    }

    #[test]
    fn adjust_scroll_noop_when_at_bottom() {
        let mut state = NetworkState::new();
        state.scroll = 0;
        state.adjust_scroll_for_new_lines(3);
        assert_eq!(state.scroll, 0);
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
            client: "okhttp".into(),
            timestamp: "09:17:37.261".into(),
        }
    }
}
