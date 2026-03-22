use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

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
