use std::collections::HashMap;
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

    fn get_battery_level(&self, serial: &str) -> Result<u8> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "battery"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb dumpsys battery failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_battery_level(&stdout).ok_or_else(|| color_eyre::eyre::eyre!("could not parse battery level"))
    }

    fn list_packages(&self, serial: &str) -> Result<Vec<String>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "pm", "list", "packages", "-3"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb pm list packages failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_package_list(&stdout))
    }

    fn list_processes(&self, serial: &str) -> Result<HashMap<String, u32>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "ps"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb shell ps failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_process_list(&stdout))
    }

    fn launch_app(&self, serial: &str, package: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "monkey", "-p", package, "-c", "android.intent.category.LAUNCHER", "1"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb monkey launch failed: {stderr}");
        }
        Ok(())
    }

    fn kill_app(&self, serial: &str, package: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "am", "force-stop", package])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb force-stop failed: {stderr}");
        }
        Ok(())
    }

    fn clear_app_data(&self, serial: &str, package: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "pm", "clear", package])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb pm clear failed: {stderr}");
        }
        Ok(())
    }
}

fn parse_package_list(output: &str) -> Vec<String> {
    let mut packages: Vec<String> = output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .collect();
    packages.sort();
    packages
}

fn parse_battery_level(output: &str) -> Option<u8> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix("level:")?;
        value.trim().parse().ok()
    })
}

fn parse_process_list(output: &str) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let Some(pid) = parts.get(1).and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(name) = parts.last() else {
            continue;
        };
        map.insert(name.to_string(), pid);
    }
    map
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
    fn parses_battery_level_from_dumpsys() {
        let output = "Current Battery Service state:\n  AC powered: false\n  USB powered: true\n  status: 2\n  level: 72\n  temperature: 250\n";
        assert_eq!(parse_battery_level(output), Some(72));
    }

    #[test]
    fn parses_full_battery() {
        let output = "  level: 100\n";
        assert_eq!(parse_battery_level(output), Some(100));
    }

    #[test]
    fn returns_none_for_missing_level() {
        let output = "Current Battery Service state:\n  AC powered: false\n";
        assert_eq!(parse_battery_level(output), None);
    }

    #[test]
    fn parses_package_list() {
        let output = "package:com.spotify.music\npackage:com.whatsapp\npackage:com.android.chrome\n";
        let packages = parse_package_list(output);
        assert_eq!(packages, vec!["com.android.chrome", "com.spotify.music", "com.whatsapp"]);
    }

    #[test]
    fn returns_empty_for_no_packages() {
        assert!(parse_package_list("").is_empty());
        assert!(parse_package_list("\n\n").is_empty());
    }

    #[test]
    fn skips_lines_without_package_prefix() {
        let output = "package:com.example.app\nWarning: some adb warning\npackage:com.other.app\n";
        let packages = parse_package_list(output);
        assert_eq!(packages, vec!["com.example.app", "com.other.app"]);
    }

    #[test]
    fn parses_process_list() {
        let output = "USER           PID  PPID     VSZ    RSS WCHAN            ADDR S NAME\n\
                       root             1     0   12345   6789 SyS_epoll_wait      0 S init\n\
                       u0_a123      12345     1   98765  43210 SyS_epoll_wait      0 S com.example.app\n\
                       u0_a456      67890     1   11111  22222 futex_wait          0 S com.other.app\n";
        let procs = parse_process_list(output);
        assert_eq!(procs.get("com.example.app"), Some(&12345));
        assert_eq!(procs.get("com.other.app"), Some(&67890));
        assert_eq!(procs.get("init"), Some(&1));
    }

    #[test]
    fn parses_empty_process_list() {
        assert!(parse_process_list("").is_empty());
        assert!(parse_process_list("USER PID NAME\n").is_empty());
    }

    #[test]
    fn skips_malformed_process_lines() {
        let output = "USER PID NAME\n\
                       partial\n\
                       root 1 init\n";
        let procs = parse_process_list(output);
        assert_eq!(procs.get("init"), Some(&1));
        assert_eq!(procs.len(), 1);
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
