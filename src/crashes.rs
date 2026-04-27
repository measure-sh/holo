use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::adb::Adb;
use crate::dropbox;

pub struct CrashEntry {
    pub timestamp: String,
    pub exception: String,
    pub full_text: String,
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    connectivity: Arc<AtomicBool>,
) -> mpsc::Receiver<Result<Vec<CrashEntry>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(5);
        loop {
            if connectivity.load(Ordering::Relaxed) {
                let result = adb
                    .get_dropbox_crashes(&serial)
                    .map(|output| {
                        let mut entries: Vec<CrashEntry> = dropbox::parse_dropbox_entries(&output, "data_app_crash", &package)
                            .into_iter()
                            .map(|(timestamp, body)| {
                                let exception = dropbox::extract_exception(&body);
                                CrashEntry { timestamp, exception, full_text: body }
                            })
                            .collect();
                        entries.reverse();
                        entries
                    })
                    .map_err(|e| e.to_string());
                if tx.send(result).is_err() {
                    return;
                }
            }
            std::thread::sleep(interval);
        }
    });
    rx
}

