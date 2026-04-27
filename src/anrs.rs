use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::adb::Adb;
use crate::dropbox;

pub struct AnrEntry {
    pub timestamp: String,
    pub reason: String,
    pub full_text: String,
}

pub fn spawn_poller(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    connectivity: Arc<AtomicBool>,
) -> mpsc::Receiver<Result<Vec<AnrEntry>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs(5);
        loop {
            if connectivity.load(Ordering::Relaxed) {
                let result = adb
                    .get_dropbox_anrs(&serial)
                    .map(|output| {
                        let mut entries: Vec<AnrEntry> = dropbox::parse_dropbox_entries(&output, "data_app_anr", &package)
                            .into_iter()
                            .map(|(timestamp, body)| {
                                let reason = dropbox::extract_field(&body, "Subject: ")
                                    .or_else(|| dropbox::extract_field(&body, "Reason: "))
                                    .unwrap_or_default();
                                AnrEntry { timestamp, reason, full_text: body }
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

