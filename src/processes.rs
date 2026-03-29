use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::adb::Adb;

pub fn spawn_poller(adb: Arc<dyn Adb>, serial: String, package: String) -> mpsc::Receiver<Option<u32>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let interval = Duration::from_secs(1);
        loop {
            if let Ok(pid) = adb.pidof(&serial, &package)
                && tx.send(pid).is_err()
            {
                return;
            }
            std::thread::sleep(interval);
        }
    });
    rx
}
