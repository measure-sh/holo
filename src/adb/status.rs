use std::sync::{mpsc, OnceLock};

/// Surfaced to the UI when an adb invocation deadline expires. Without this
/// signal the failure is invisible — the call returns `Err`, callers fall
/// back to defaults, and the user just sees a stale-looking pane.
#[derive(Debug, Clone)]
pub enum AdbStatus {
    Timeout { args: String },
}

static STATUS_TX: OnceLock<mpsc::Sender<AdbStatus>> = OnceLock::new();

/// Wire the adb layer to the UI loop. Called once at startup; later calls are
/// silently ignored so tests can `set_status_sender` without poisoning prod.
pub fn set_status_sender(tx: mpsc::Sender<AdbStatus>) {
    let _ = STATUS_TX.set(tx);
}

pub fn report_timeout(args: String) {
    if let Some(tx) = STATUS_TX.get() {
        let _ = tx.send(AdbStatus::Timeout { args });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_a_noop_without_a_sender() {
        // Other tests in this binary may install a sender first; the contract
        // is just that an uninstalled sender doesn't panic. Calling here is a
        // smoke check.
        report_timeout("adb -s X shell true".into());
    }
}
