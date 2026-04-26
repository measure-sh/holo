use color_eyre::{Result, eyre::eyre};

use crate::adb::Adb;

use super::blobs;

/// Push the agent .so to the app's data dir then ask the JVM to load it via
/// `cmd activity attach-agent`. Idempotent on the device side: the agent's
/// `Agent_OnAttach` short-circuits subsequent calls.
pub fn attach_running(adb: &dyn Adb, serial: &str, package: &str) -> Result<()> {
    let abi = adb.get_abi(serial)?;
    let bytes = blobs::for_abi(&abi)
        .ok_or_else(|| eyre!("no embedded agent for ABI {abi}"))?;
    let so_path = adb.push_agent(serial, package, bytes)?;
    adb.attach_agent(serial, package, &so_path)
}
