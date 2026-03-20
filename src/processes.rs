use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::adb::Adb;

pub fn spawn_poller(adb: Arc<dyn Adb>, serial: String) -> mpsc::Receiver<HashMap<String, u32>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(5);
        loop {
            if let Ok(procs) = adb.list_processes(&serial) {
                if tx.send(procs).is_err() {
                    return;
                }
            }
            std::thread::sleep(interval);
        }
    });
    rx
}
