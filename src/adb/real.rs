use std::process::Command;

use color_eyre::{Result, eyre::bail};

use super::{Adb, Device};

pub struct RealAdb;

impl Adb for RealAdb {
    fn list_devices(&self) -> Result<Vec<Device>> {
        let output = Command::new("adb").args(["devices", "-l"]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb devices failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices = stdout
            .lines()
            .skip(1) // skip "List of devices attached" header
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let serial = parts.next()?.to_string();
                let status = parts.next()?;
                if status != "device" {
                    return None; // skip offline/unauthorized
                }
                let description = parts.collect::<Vec<_>>().join(" ");
                Some(Device {
                    serial,
                    description,
                })
            })
            .collect();

        Ok(devices)
    }
}
