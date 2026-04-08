use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::Action;

pub struct LogcatLine<'a> {
    pub timestamp: &'a str,
    pub level: char,
    pub tag: &'a str,
    pub message: &'a str,
}

fn split_next_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    match s.find(' ') {
        Some(i) => Some((&s[..i], &s[i + 1..])),
        None => Some((s, "")),
    }
}

pub fn parse(line: &str) -> Option<LogcatLine<'_>> {
    let mut rest = line;

    let (_date, r) = split_next_token(rest)?;
    let (timestamp, r) = split_next_token(r)?;
    let (_pid, r) = split_next_token(r)?;
    let (_tid, r) = split_next_token(r)?;
    let (level_str, r) = split_next_token(r)?;
    rest = r;

    let level = level_str.chars().next()?;
    if !matches!(level, 'V' | 'D' | 'I' | 'W' | 'E' | 'F') {
        return None;
    }
    let (tag, message) = if let Some((t, m)) = rest.split_once(": ") {
        (t.trim(), m)
    } else if let Some(t) = rest.strip_suffix(':') {
        (t.trim(), "")
    } else {
        (rest.trim(), "")
    };

    Some(LogcatLine {
        timestamp,
        level,
        tag,
        message,
    })
}

pub struct LogcatHandle {
    child: Child,
    rx: Receiver<String>,
}

impl LogcatHandle {
    pub fn spawn(serial: &str, pid: u32) -> Option<Self> {
        let mut child = Command::new("adb")
            .args(["-s", serial, "logcat", &format!("--pid={pid}")])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        Some(Self { child, rx })
    }

    pub fn rx(&self) -> &Receiver<String> {
        &self.rx
    }
}

impl Drop for LogcatHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogcatEditTarget {
    Tag,
    Search,
}

const LEVELS: [Option<char>; 7] = [
    None,
    Some('V'),
    Some('D'),
    Some('I'),
    Some('W'),
    Some('E'),
    Some('F'),
];

pub struct LogcatFilter {
    pub tag: String,
    pub search: String,
    pub level: Option<char>,
}

impl LogcatFilter {
    fn new() -> Self {
        Self {
            tag: String::new(),
            search: String::new(),
            level: None,
        }
    }

    pub fn matches(&self, line: &str) -> bool {
        let Some(parsed) = parse(line) else {
            return true;
        };
        let tag_ok = self.tag.is_empty()
            || parsed.tag.to_lowercase().contains(&self.tag.to_lowercase());
        let search_ok = self.search.is_empty()
            || line.to_lowercase().contains(&self.search.to_lowercase());
        let level_ok = self.level.is_none() || Some(parsed.level) == self.level;
        tag_ok && search_ok && level_ok
    }
}

pub struct LogcatState {
    pub filter: LogcatFilter,
    pub scroll: usize,
    pub wrap: bool,
    pub editing: Option<LogcatEditTarget>,
}

impl LogcatState {
    pub fn new() -> Self {
        Self {
            filter: LogcatFilter::new(),
            scroll: 0,
            wrap: false,
            editing: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;
        if let Some(target) = self.editing {
            match code {
                KeyCode::Char(c) => match target {
                    LogcatEditTarget::Tag => self.filter.tag.push(c),
                    LogcatEditTarget::Search => self.filter.search.push(c),
                },
                KeyCode::Backspace => match target {
                    LogcatEditTarget::Tag => { self.filter.tag.pop(); }
                    LogcatEditTarget::Search => { self.filter.search.pop(); }
                },
                KeyCode::Enter | KeyCode::Esc => self.editing = None,
                _ => {}
            }
            return Some(Action::Noop);
        }

        match code {
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
            KeyCode::Esc if self.scroll > 0 => {
                self.scroll = 0;
                Some(Action::Noop)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            KeyCode::Char('t') => {
                self.editing = Some(LogcatEditTarget::Tag);
                Some(Action::Noop)
            }
            KeyCode::Char('/') => {
                self.editing = Some(LogcatEditTarget::Search);
                Some(Action::Noop)
            }
            KeyCode::Right => {
                self.cycle_level(true);
                Some(Action::Noop)
            }
            KeyCode::Left => {
                self.cycle_level(false);
                Some(Action::Noop)
            }
            KeyCode::Char('o') => {
                Some(Action::OpenLogcat)
            }
            KeyCode::Char('r') => {
                self.reset();
                Some(Action::ResetLogcat)
            }
            KeyCode::Char('w') => {
                self.wrap = !self.wrap;
                Some(Action::Noop)
            }
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        self.filter = LogcatFilter::new();
        self.scroll = 0;
        self.wrap = false;
    }

    pub fn cycle_level(&mut self, forward: bool) {
        let current = LEVELS.iter().position(|l| *l == self.filter.level).unwrap_or(0);
        let next = if forward {
            (current + 1) % LEVELS.len()
        } else {
            (current + LEVELS.len() - 1) % LEVELS.len()
        };
        self.filter.level = LEVELS[next];
    }

    pub fn clamp_scroll(&mut self, total_lines: usize, visible_height: usize) {
        let max = total_lines.saturating_sub(visible_height);
        self.scroll = self.scroll.min(max);
    }

    pub fn adjust_scroll_for_new_lines(&mut self, count: usize) {
        if self.scroll > 0 {
            self.scroll += count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_line() {
        let line = "03-20 17:20:19.256 17010 17026 D Measure : EventProcessor started";
        let parsed = parse(line).unwrap();
        assert_eq!(parsed.timestamp, "17:20:19.256");
        assert_eq!(parsed.level, 'D');
        assert_eq!(parsed.tag, "Measure");
        assert_eq!(parsed.message, "EventProcessor started");
    }

    #[test]
    fn parse_line_with_padded_fields() {
        let line = "03-20 21:35:22.958  3725  3742 D Measure : Span processed: SampleApp.onCreate, 102ms";
        let parsed = parse(line).unwrap();
        assert_eq!(parsed.timestamp, "21:35:22.958");
        assert_eq!(parsed.level, 'D');
        assert_eq!(parsed.tag, "Measure");
        assert_eq!(parsed.message, "Span processed: SampleApp.onCreate, 102ms");
    }

    #[test]
    fn parse_missing_fields() {
        assert!(parse("03-20 17:20:19.256").is_none());
    }

    #[test]
    fn parse_invalid_level() {
        let line = "03-20 17:20:19.256 17010 17026 X Tag : msg";
        assert!(parse(line).is_none());
    }

    #[test]
    fn parse_no_message() {
        let line = "03-20 17:20:19.256 17010 17026 D Tag :";
        let parsed = parse(line).unwrap();
        assert_eq!(parsed.tag, "Tag");
        assert_eq!(parsed.message, "");
    }

    #[test]
    fn new_state_has_empty_filter_and_zero_scroll() {
        let state = LogcatState::new();
        assert_eq!(state.filter.tag, "");
        assert_eq!(state.filter.search, "");
        assert_eq!(state.filter.level, None);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn reset_clears_filter_and_scroll() {
        let mut state = LogcatState::new();
        state.filter.tag = "MyTag".into();
        state.filter.search = "error".into();
        state.filter.level = Some('E');
        state.scroll = 5;
        state.reset();
        assert_eq!(state.filter.tag, "");
        assert_eq!(state.filter.search, "");
        assert_eq!(state.filter.level, None);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn cycle_level_forward() {
        let mut state = LogcatState::new();
        assert_eq!(state.filter.level, None);
        state.cycle_level(true);
        assert_eq!(state.filter.level, Some('V'));
        state.cycle_level(true);
        assert_eq!(state.filter.level, Some('D'));
    }

    #[test]
    fn cycle_level_backward() {
        let mut state = LogcatState::new();
        state.cycle_level(false);
        assert_eq!(state.filter.level, Some('F'));
    }

    #[test]
    fn cycle_level_wraps() {
        let mut state = LogcatState::new();
        for _ in 0..7 {
            state.cycle_level(true);
        }
        assert_eq!(state.filter.level, None);
    }

    #[test]
    fn clamp_scroll_limits_to_max() {
        let mut state = LogcatState::new();
        state.scroll = 100;
        state.clamp_scroll(50, 20);
        assert_eq!(state.scroll, 30);
    }

    #[test]
    fn adjust_scroll_increments_when_scrolled() {
        let mut state = LogcatState::new();
        state.scroll = 5;
        state.adjust_scroll_for_new_lines(3);
        assert_eq!(state.scroll, 8);
    }

    #[test]
    fn adjust_scroll_noop_when_at_bottom() {
        let mut state = LogcatState::new();
        state.scroll = 0;
        state.adjust_scroll_for_new_lines(3);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn filter_matches_all_by_default() {
        let filter = LogcatFilter::new();
        assert!(filter.matches("01-01 12:00:00.000  1234  5678 D MyTag   : hello"));
    }

    #[test]
    fn filter_by_level() {
        let mut filter = LogcatFilter::new();
        filter.level = Some('E');
        assert!(!filter.matches("01-01 12:00:00.000  1234  5678 D MyTag   : hello"));
        assert!(filter.matches("01-01 12:00:00.000  1234  5678 E MyTag   : error"));
    }

    #[test]
    fn filter_by_tag() {
        let mut filter = LogcatFilter::new();
        filter.tag = "MyTag".into();
        assert!(filter.matches("01-01 12:00:00.000  1234  5678 D MyTag   : hello"));
        assert!(!filter.matches("01-01 12:00:00.000  1234  5678 D Other   : hello"));
    }

    #[test]
    fn filter_by_search() {
        let mut filter = LogcatFilter::new();
        filter.search = "error".into();
        assert!(filter.matches("01-01 12:00:00.000  1234  5678 D MyTag   : some error here"));
        assert!(!filter.matches("01-01 12:00:00.000  1234  5678 D MyTag   : hello world"));
    }
}
