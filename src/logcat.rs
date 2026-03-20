use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

pub struct LogcatLine<'a> {
    pub timestamp: &'a str,
    pub pid: &'a str,
    pub tid: &'a str,
    pub level: char,
    pub tag: &'a str,
    pub message: &'a str,
}

pub fn parse(line: &str) -> Option<LogcatLine<'_>> {
    let mut parts = line.splitn(6, ' ');
    let _date = parts.next()?;
    let timestamp = parts.next()?;
    let pid = parts.next()?;
    let tid = parts.next()?;
    let level_str = parts.next()?;
    let level = level_str.chars().next()?;
    if !matches!(level, 'V' | 'D' | 'I' | 'W' | 'E' | 'F') {
        return None;
    }
    let rest = parts.next()?;
    let (tag, message) = if let Some((t, m)) = rest.split_once(": ") {
        (t.trim(), m)
    } else if let Some(t) = rest.strip_suffix(':') {
        (t.trim(), "")
    } else {
        (rest.trim(), "")
    };

    Some(LogcatLine {
        timestamp,
        pid,
        tid,
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
        assert_eq!(parsed.pid, "17010");
        assert_eq!(parsed.tid, "17026");
        assert_eq!(parsed.level, 'D');
        assert_eq!(parsed.tag, "Measure");
        assert_eq!(parsed.message, "EventProcessor started");
    }

    #[test]
    fn parse_main_thread() {
        let line = "03-20 17:20:19.256 17010 17010 I ActivityManager : Displayed com.example/.MainActivity";
        let parsed = parse(line).unwrap();
        assert_eq!(parsed.pid, "17010");
        assert_eq!(parsed.tid, "17010");
        assert_eq!(parsed.level, 'I');
        assert_eq!(parsed.tag, "ActivityManager");
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
