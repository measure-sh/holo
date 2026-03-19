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
        Ok(parse_device_list(&stdout))
    }
}

pub fn parse_device_list(output: &str) -> Vec<Device> {
    output
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
            let mut model = None;
            let mut device = None;
            for part in parts {
                if let Some(val) = part.strip_prefix("model:").filter(|v| !v.is_empty()) {
                    model = Some(val.replace('_', " "));
                } else if let Some(val) = part.strip_prefix("device:").filter(|v| !v.is_empty()) {
                    device = Some(val.to_string());
                }
            }
            Some(Device {
                serial,
                model,
                device,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_device() {
        let output = "List of devices attached\nR5CT32MKXYJ device usb:1-1 product:a53xnaxx model:SM_A536E device:a53x transport_id:1\n\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "R5CT32MKXYJ");
        assert_eq!(devices[0].model.as_deref(), Some("SM A536E"));
        assert_eq!(devices[0].device.as_deref(), Some("a53x"));
    }

    #[test]
    fn parses_multiple_devices() {
        let output = "List of devices attached\n\
            R5CT32MKXYJ device usb:1-1 model:SM_A536E\n\
            emulator-5554 device product:sdk_phone model:sdk_phone\n\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "R5CT32MKXYJ");
        assert_eq!(devices[0].model.as_deref(), Some("SM A536E"));
        assert_eq!(devices[1].model.as_deref(), Some("sdk phone"));
    }

    #[test]
    fn skips_offline_and_unauthorized_devices() {
        let output = "List of devices attached\n\
            R5CT32MKXYJ device model:SM_A536E\n\
            ABCDEF123456 offline\n\
            GHIJKL789012 unauthorized\n\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "R5CT32MKXYJ");
    }

    #[test]
    fn returns_empty_for_no_devices() {
        let output = "List of devices attached\n\n";
        let devices = parse_device_list(output);
        assert!(devices.is_empty());
    }

    #[test]
    fn handles_device_with_no_description() {
        let output = "List of devices attached\nemulator-5554 device\n\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "emulator-5554");
        assert!(devices[0].model.is_none());
        assert!(devices[0].device.is_none());
    }
}
