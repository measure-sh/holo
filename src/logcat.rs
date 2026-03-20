use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

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
