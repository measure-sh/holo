use std::path::PathBuf;
use std::process::Command;

use color_eyre::{Result, eyre::bail};
use std::io::Write;

use super::{Adb, Device, MemInfo};

fn emulator_path() -> PathBuf {
    std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(|sdk| PathBuf::from(sdk).join("emulator").join("emulator"))
        .unwrap_or_else(|| PathBuf::from("emulator"))
}

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

    fn pidof(&self, serial: &str, package: &str) -> Result<Option<u32>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "pidof", "-s", package])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(trimmed.parse::<u32>().ok())
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

    fn uninstall_app(&self, serial: &str, package: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "uninstall", package])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb uninstall failed: {stderr}");
        }
        Ok(())
    }

    fn list_databases(&self, serial: &str, package: &str) -> Result<Vec<String>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "run-as", package, "ls", "databases/"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("listing databases failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_database_list(&stdout))
    }

    fn query_database(&self, serial: &str, package: &str, db_name: &str, sql: &str) -> Result<String> {
        let escaped_sql = sql.replace('\'', "'\\''");
        let shell_cmd = format!(
            "run-as {package} sqlite3 databases/{db_name} '{escaped_sql}'"
        );
        let output = Command::new("adb")
            .args(["-s", serial, "shell", &shell_cmd])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{stderr}");
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn pull_database(&self, serial: &str, package: &str, db_name: &str, dest: &std::path::Path) -> Result<()> {
        for suffix in ["", "-wal", "-shm"] {
            let remote_path = format!("databases/{db_name}{suffix}");
            let check = Command::new("adb")
                .args(["-s", serial, "shell", "run-as", package, "ls", &remote_path])
                .output()?;
            if !check.status.success() {
                continue;
            }
            let output = Command::new("adb")
                .args(["-s", serial, "exec-out", "run-as", package, "cat", &remote_path])
                .output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("pull {db_name}{suffix} failed: {stderr}");
            }
            let dest_file = dest.join(format!("{db_name}{suffix}"));
            let mut file = std::fs::File::create(&dest_file)?;
            file.write_all(&output.stdout)?;
        }
        Ok(())
    }

    fn wake_screen(&self, serial: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "input", "keyevent", "KEYCODE_WAKEUP"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb wake screen failed: {stderr}");
        }
        Ok(())
    }

    fn get_layout_bounds(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "getprop", "debug.layout"])
            .output()?;
        let value = String::from_utf8_lossy(&output.stdout);
        Ok(value.trim() == "true")
    }

    fn set_layout_bounds(&self, serial: &str, enabled: bool) -> Result<()> {
        let value = if enabled { "true" } else { "false" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "setprop", "debug.layout", value])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("setprop debug.layout failed: {stderr}");
        }
        let _ = Command::new("adb")
            .args(["-s", serial, "shell", "service", "call", "activity", "1599295570"])
            .output();
        Ok(())
    }
    fn get_airplane_mode(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "get", "global", "airplane_mode_on"])
            .output()?;
        let value = String::from_utf8_lossy(&output.stdout);
        Ok(parse_airplane_mode(&value))
    }

    fn set_airplane_mode(&self, serial: &str, enabled: bool) -> Result<()> {
        let mode = if enabled { "enable" } else { "disable" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "cmd", "connectivity", "airplane-mode", mode])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("airplane-mode {mode} failed: {stderr}");
        }
        Ok(())
    }

    fn list_permissions(&self, serial: &str, package: &str) -> Result<Vec<(String, bool)>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "package", package])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("dumpsys package failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_runtime_permissions(&stdout))
    }

    fn grant_permission(&self, serial: &str, package: &str, permission: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "pm", "grant", package, permission])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("pm grant failed: {stderr}");
        }
        Ok(())
    }

    fn revoke_permission(&self, serial: &str, package: &str, permission: &str) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "pm", "revoke", package, permission])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("pm revoke failed: {stderr}");
        }
        Ok(())
    }

    fn get_app_version(&self, serial: &str, package: &str) -> Result<(String, String)> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "package", package])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("dumpsys package failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_app_version(&stdout))
    }

    fn list_files(&self, serial: &str, package: &str, path: &str) -> Result<Vec<(String, bool)>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "run-as", package, "ls", "-p", path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("ls failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_file_list(&stdout))
    }

    fn get_cpu_usage(&self, serial: &str, package: &str) -> Result<f32> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "top", "-b", "-n", "1", "-q"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("top failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_top_cpu(&stdout, package))
    }

    fn get_meminfo(&self, serial: &str, package: &str) -> Result<MemInfo> {
        let cmd = format!(
            "PID=$(pidof -s {0}); [ -n \"$PID\" ] && cat /proc/$PID/status 2>/dev/null; true",
            package
        );
        let output = Command::new("adb")
            .args(["-s", serial, "shell", &cmd])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_proc_mem(&stdout))
    }

    fn pull_file(&self, serial: &str, package: &str, remote_path: &str, dest: &std::path::Path) -> Result<()> {
        let output = Command::new("adb")
            .args(["-s", serial, "exec-out", "run-as", package, "cat", remote_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("pull failed: {stderr}");
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(dest)?;
        file.write_all(&output.stdout)?;
        Ok(())
    }

    fn start_trace(&self, serial: &str, config: &str) -> Result<()> {
        let mut child = Command::new("adb")
            .args([
                "-s", serial, "shell", "perfetto", "-d", "--txt", "-c", "-",
                "-o", "/data/misc/perfetto-traces/holo_trace.perfetto-trace",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(config.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("perfetto start failed: {stderr}");
        }
        Ok(())
    }

    fn stop_and_pull_trace(&self, serial: &str, dest: &std::path::Path) -> Result<()> {
        let _ = Command::new("adb")
            .args(["-s", serial, "shell", "pkill", "-INT", "perfetto"])
            .output();

        std::thread::sleep(std::time::Duration::from_secs(2));

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("adb")
            .args([
                "-s", serial, "pull",
                "/data/misc/perfetto-traces/holo_trace.perfetto-trace",
                &dest.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("trace pull failed: {stderr}");
        }
        Ok(())
    }

    fn take_screenshot(&self, serial: &str, dest: &std::path::Path) -> Result<()> {
        let shell = |args: &[&str]| -> Result<()> {
            let mut cmd_args = vec!["-s", serial, "shell"];
            cmd_args.extend_from_slice(args);
            let output = Command::new("adb").args(&cmd_args).output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("adb shell failed: {stderr}");
            }
            Ok(())
        };

        shell(&["settings", "put", "global", "sysui_demo_allowed", "1"])?;
        shell(&["am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", "enter"])?;
        shell(&["am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", "clock", "-e", "hhmm", "1000"])?;
        shell(&["am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", "battery", "-e", "level", "100", "-e", "plugged", "false"])?;
        shell(&["am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", "network", "-e", "wifi", "show", "-e", "level", "4"])?;
        shell(&["am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", "notifications", "-e", "visible", "false"])?;

        std::thread::sleep(std::time::Duration::from_millis(500));

        let result = Command::new("adb")
            .args(["-s", serial, "exec-out", "screencap", "-p"])
            .output();

        let _ = shell(&["am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", "exit"]);

        let output = result?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("screencap failed: {stderr}");
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(dest)?;
        file.write_all(&output.stdout)?;
        Ok(())
    }

    fn get_wifi_enabled(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "get", "global", "wifi_on"])
            .output()?;
        let value = String::from_utf8_lossy(&output.stdout);
        Ok(parse_wifi_enabled(&value))
    }

    fn set_wifi_enabled(&self, serial: &str, enabled: bool) -> Result<()> {
        let state = if enabled { "enable" } else { "disable" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "svc", "wifi", state])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("svc wifi {state} failed: {stderr}");
        }
        Ok(())
    }

    fn enable_wireless_adb(&self, serial: &str) -> Result<String> {
        let output = Command::new("adb")
            .args(["-s", serial, "tcpip", "5555"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb tcpip failed: {stderr}");
        }

        std::thread::sleep(std::time::Duration::from_secs(1));

        let output = Command::new("adb")
            .args(["-s", serial, "shell", "ip", "route"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let ip = parse_device_ip(&stdout)
            .ok_or_else(|| color_eyre::eyre::eyre!("could not detect device IP"))?;

        let addr = format!("{ip}:5555");
        let output = Command::new("adb")
            .args(["connect", &addr])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb connect failed: {stderr}");
        }
        Ok(addr)
    }

    fn get_disk_usage(&self, serial: &str, package: &str) -> Result<(u64, u64)> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "run-as", package, "du", "-s", ".", "./cache"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("du failed: {stderr}");
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_du_output(&stdout))
    }

    fn get_dark_mode(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "get", "secure", "ui_night_mode"])
            .output()?;
        let value = String::from_utf8_lossy(&output.stdout);
        Ok(value.trim() == "2")
    }

    fn set_dark_mode(&self, serial: &str, enabled: bool) -> Result<()> {
        let mode = if enabled { "yes" } else { "no" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "cmd", "uimode", "night", mode])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("cmd uimode night {mode} failed: {stderr}");
        }
        Ok(())
    }

    fn list_avds(&self) -> Result<Vec<String>> {
        let output = Command::new(emulator_path()).arg("-list-avds").output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("emulator -list-avds failed: {stderr}");
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_avd_list(&stdout))
    }

    fn launch_emulator(&self, avd_name: &str) -> Result<()> {
        Command::new(emulator_path())
            .args(["-avd", avd_name])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        Ok(())
    }

    fn get_avd_name(&self, serial: &str) -> Result<String> {
        let output = Command::new("adb")
            .args(["-s", serial, "emu", "avd", "name"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let name = stdout.lines().next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            bail!("could not get AVD name for {serial}");
        }
        Ok(name)
    }

    fn get_state(&self, serial: &str) -> Result<String> {
        let output = Command::new("adb")
            .args(["-s", serial, "get-state"])
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn is_debuggable(&self, serial: &str, package: &str) -> bool {
        Command::new("adb")
            .args(["-s", serial, "shell", "run-as", package, "id"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn get_show_taps(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "get", "system", "show_touches"])
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim() == "1")
    }

    fn set_show_taps(&self, serial: &str, enabled: bool) -> Result<()> {
        let val = if enabled { "1" } else { "0" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "put", "system", "show_touches", val])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("settings put show_touches failed: {stderr}");
        }
        Ok(())
    }

    fn get_pointer_location(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "get", "system", "pointer_location"])
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim() == "1")
    }

    fn set_pointer_location(&self, serial: &str, enabled: bool) -> Result<()> {
        let val = if enabled { "1" } else { "0" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "put", "system", "pointer_location", val])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("settings put pointer_location failed: {stderr}");
        }
        Ok(())
    }

    fn get_gpu_rendering(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "getprop", "debug.hwui.profile"])
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim() == "visual_bars")
    }

    fn set_gpu_rendering(&self, serial: &str, enabled: bool) -> Result<()> {
        let val = if enabled { "visual_bars" } else { "false" };
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "setprop", "debug.hwui.profile", val])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("setprop debug.hwui.profile failed: {stderr}");
        }
        Command::new("adb")
            .args(["-s", serial, "shell", "service", "call", "activity", "1599295570"])
            .output()?;
        Ok(())
    }

    fn get_talkback_enabled(&self, serial: &str) -> Result<bool> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "get", "secure", "enabled_accessibility_services"])
            .output()?;
        let value = String::from_utf8_lossy(&output.stdout);
        Ok(value.to_lowercase().contains("talkback"))
    }

    fn set_talkback_enabled(&self, serial: &str, enabled: bool) -> Result<()> {
        const TALKBACK: &str = "com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService";

        let current = {
            let output = Command::new("adb")
                .args(["-s", serial, "shell", "settings", "get", "secure", "enabled_accessibility_services"])
                .output()?;
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        let services: Vec<&str> = current
            .split(':')
            .filter(|s| !s.is_empty() && *s != "null")
            .filter(|s| !s.to_lowercase().contains("talkback"))
            .collect();

        let new_value = if enabled {
            let mut with_tb = services.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            with_tb.push(TALKBACK.to_string());
            with_tb.join(":")
        } else if services.is_empty() {
            "null".to_string()
        } else {
            services.join(":")
        };

        let output = Command::new("adb")
            .args(["-s", serial, "shell", "settings", "put", "secure", "enabled_accessibility_services", &new_value])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("settings put enabled_accessibility_services failed: {stderr}");
        }
        Ok(())
    }

    fn get_dropbox_crashes(&self, serial: &str) -> Result<String> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "dropbox", "--print", "data_app_crash"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("dumpsys dropbox failed: {stderr}");
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn get_dropbox_anrs(&self, serial: &str) -> Result<String> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "dropbox", "--print", "data_app_anr"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("dumpsys dropbox failed: {stderr}");
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn has_measure_sdk(&self, serial: &str, package: &str) -> bool {
        Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "package", package])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("sh.measure"))
            .unwrap_or(false)
    }
}

fn parse_du_output(output: &str) -> (u64, u64) {
    let mut data_kb = 0u64;
    let mut cache_kb = 0u64;
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2
            && let Ok(size) = parts[0].trim().parse::<u64>()
        {
            let path = parts[1].trim();
            if path == "./cache" {
                cache_kb = size;
            } else if path == "." {
                data_kb = size;
            }
        }
    }
    (data_kb, cache_kb)
}

fn parse_device_ip(output: &str) -> Option<String> {
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let has_cellular = parts.iter().any(|p| p.starts_with("rmnet"));
        if has_cellular {
            continue;
        }
        for (i, &part) in parts.iter().enumerate() {
            if part == "src" {
                return parts.get(i + 1).map(|s| s.to_string());
            }
        }
    }
    None
}

fn parse_wifi_enabled(output: &str) -> bool {
    output.trim() == "1"
}

fn parse_app_version(output: &str) -> (String, String) {
    let mut version_name = String::new();
    let mut version_code = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("versionName=") {
            if version_name.is_empty() {
                version_name = val.to_string();
            }
        } else if let Some(val) = trimmed.strip_prefix("versionCode=")
            && version_code.is_empty()
        {
            version_code = val.split_whitespace().next().unwrap_or("").to_string();
        }
    }

    (version_name, version_code)
}

fn parse_file_list(output: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            if let Some(name) = l.strip_suffix('/') {
                (name.to_string(), true)
            } else {
                (l.to_string(), false)
            }
        })
        .collect();
    entries.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });
    entries
}

fn parse_runtime_permissions(output: &str) -> Vec<(String, bool)> {
    let mut results = Vec::new();
    let mut in_runtime_section = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("runtime permissions:") {
            in_runtime_section = true;
            continue;
        }
        if in_runtime_section {
            if trimmed.is_empty() || (!trimmed.starts_with("android.permission.") && !trimmed.contains(": granted=")) {
                in_runtime_section = false;
                continue;
            }
            if let Some((perm, rest)) = trimmed.split_once(": granted=") {
                let granted = rest.starts_with("true");
                results.push((perm.to_string(), granted));
            }
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.dedup_by(|a, b| a.0 == b.0);
    results
}

fn parse_airplane_mode(output: &str) -> bool {
    output.trim() == "1"
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

fn parse_top_cpu(output: &str, package: &str) -> f32 {
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.ends_with(package) {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        if let Some(last) = cols.last()
            && *last != package
        {
            continue;
        }
        if let Ok(val) = cols[8].parse::<f32>() {
            return val;
        }
    }
    0.0
}

fn parse_kb_value(line: &str, prefix: &str) -> Option<u64> {
    line.strip_prefix(prefix)?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()
}

fn parse_proc_mem(output: &str) -> MemInfo {
    let mut info = MemInfo::default();
    for line in output.lines() {
        if let Some(val) = parse_kb_value(line, "VmRSS:") {
            info.rss_kb = val;
            break;
        }
    }
    info
}

pub fn parse_database_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.ends_with("-journal") && !l.ends_with("-wal") && !l.ends_with("-shm"))
        .map(|l| l.to_string())
        .collect()
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
                connected: true,
            })
        })
        .collect()
}

pub fn parse_avd_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
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
    fn parses_database_list() {
        let output = "app.db\napp.db-journal\napp.db-wal\napp.db-shm\ncache.db\n";
        let dbs = parse_database_list(output);
        assert_eq!(dbs, vec!["app.db", "cache.db"]);
    }

    #[test]
    fn returns_empty_for_no_databases() {
        assert!(parse_database_list("").is_empty());
        assert!(parse_database_list("\n\n").is_empty());
    }

    #[test]
    fn filters_all_journal_variants() {
        let output = "main.db\nmain.db-journal\nmain.db-wal\nmain.db-shm\n";
        let dbs = parse_database_list(output);
        assert_eq!(dbs, vec!["main.db"]);
    }

    #[test]
    fn parses_airplane_mode_on() {
        assert!(parse_airplane_mode("1\n"));
        assert!(parse_airplane_mode("1"));
    }

    #[test]
    fn parses_airplane_mode_off() {
        assert!(!parse_airplane_mode("0\n"));
        assert!(!parse_airplane_mode("0"));
        assert!(!parse_airplane_mode("null\n"));
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

    #[test]
    fn parses_runtime_permissions() {
        let output = "    runtime permissions:\n\
                       android.permission.CAMERA: granted=true\n\
                       android.permission.READ_CONTACTS: granted=false, flags=[ USER_SET ]\n\
                       \n\
                       some other section:\n";
        let perms = parse_runtime_permissions(output);
        assert_eq!(perms.len(), 2);
        assert_eq!(perms[0], ("android.permission.CAMERA".into(), true));
        assert_eq!(perms[1], ("android.permission.READ_CONTACTS".into(), false));
    }

    #[test]
    fn returns_empty_for_no_permissions() {
        assert!(parse_runtime_permissions("").is_empty());
        assert!(parse_runtime_permissions("some unrelated output\n").is_empty());
    }

    #[test]
    fn deduplicates_permissions() {
        let output = "    runtime permissions:\n\
                       android.permission.CAMERA: granted=true\n\
                       android.permission.CAMERA: granted=false\n\
                       \n";
        let perms = parse_runtime_permissions(output);
        assert_eq!(perms.len(), 1);
    }

    #[test]
    fn parses_app_version() {
        let output = "  versionCode=42 minSdk=21 targetSdk=34\n  versionName=1.2.3\n";
        let (name, code) = parse_app_version(output);
        assert_eq!(name, "1.2.3");
        assert_eq!(code, "42");
    }

    #[test]
    fn returns_empty_for_missing_version() {
        let (name, code) = parse_app_version("some unrelated output\n");
        assert!(name.is_empty());
        assert!(code.is_empty());
    }

    #[test]
    fn takes_first_version_match() {
        let output = "  versionCode=10 minSdk=21\n  versionName=1.0\n  versionCode=20 minSdk=21\n  versionName=2.0\n";
        let (name, code) = parse_app_version(output);
        assert_eq!(name, "1.0");
        assert_eq!(code, "10");
    }

    #[test]
    fn parses_file_list_with_dirs_and_files() {
        let output = "cache/\nfiles/\nshared_prefs/\napp.conf\ndata.bin\n";
        let entries = parse_file_list(output);
        assert_eq!(entries, vec![
            ("cache".into(), true),
            ("files".into(), true),
            ("shared_prefs".into(), true),
            ("app.conf".into(), false),
            ("data.bin".into(), false),
        ]);
    }

    #[test]
    fn returns_empty_for_no_files() {
        assert!(parse_file_list("").is_empty());
        assert!(parse_file_list("\n\n").is_empty());
    }

    #[test]
    fn sorts_dirs_before_files() {
        let output = "z_file\na_dir/\nb_file\n";
        let entries = parse_file_list(output);
        assert_eq!(entries[0], ("a_dir".into(), true));
        assert_eq!(entries[1], ("b_file".into(), false));
        assert_eq!(entries[2], ("z_file".into(), false));
    }

    #[test]
    fn parses_proc_mem() {
        let output = "\
Name:\tcom.example.app
VmRSS:\t  128000 kB
VmSwap:\t       0 kB
";
        let info = parse_proc_mem(output);
        assert_eq!(info.rss_kb, 128000);
    }

    #[test]
    fn proc_mem_empty_output() {
        let info = parse_proc_mem("");
        assert_eq!(info.rss_kb, 0);
    }

    #[test]
    fn parses_top_cpu_for_package() {
        let output = "\
  PID USER         PR  NI VIRT  RES  SHR S[%CPU] %MEM     TIME+ ARGS
  567 system       20   0  15G 120M  80M S  1.1   2.0   0:05.00 system_server
12345 u0_a123      20   0  12G  90M  60M S  5.2   3.1   1:23.45 com.example.app
  890 u0_a456      20   0  10G  50M  30M S  0.3   1.0   0:01.00 com.other.app
";
        assert!((parse_top_cpu(output, "com.example.app") - 5.2).abs() < 0.01);
    }

    #[test]
    fn top_cpu_zero_when_not_found() {
        let output = "  PID USER PR NI VIRT RES SHR S[%CPU] %MEM TIME+ ARGS\n  567 system 20 0 15G 120M 80M S 1.1 2.0 0:05.00 system_server\n";
        assert_eq!(parse_top_cpu(output, "com.example.app"), 0.0);
    }

    #[test]
    fn top_cpu_no_partial_match() {
        let output = "  PID USER         PR  NI VIRT  RES  SHR S[%CPU] %MEM     TIME+ ARGS\n12345 u0_a123      20   0  12G  90M  60M S  5.2   3.1   1:23.45 com.example.app.debug\n";
        assert_eq!(parse_top_cpu(output, "com.example.app"), 0.0);
    }

    #[test]
    fn parses_wifi_enabled() {
        assert!(parse_wifi_enabled("1\n"));
        assert!(parse_wifi_enabled("1"));
        assert!(!parse_wifi_enabled("0\n"));
        assert!(!parse_wifi_enabled(""));
        assert!(!parse_wifi_enabled("null\n"));
    }

    #[test]
    fn parses_device_ip_skipping_cellular() {
        let output = "100.109.184.96/27 dev rmnet_data1 proto kernel scope link src 100.109.184.112\n\
            192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.66\n";
        assert_eq!(parse_device_ip(output), Some("192.168.1.66".to_string()));
    }

    #[test]
    fn parses_device_ip_from_eth0() {
        let output = "10.0.0.0/24 dev eth0 proto kernel scope link src 10.0.0.42\n";
        assert_eq!(parse_device_ip(output), Some("10.0.0.42".to_string()));
    }

    #[test]
    fn parses_device_ip_with_default_route() {
        let output = "default via 192.168.1.1 dev wlan0 proto dhcp src 192.168.1.42 metric 600\n\
            192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.42\n";
        assert_eq!(parse_device_ip(output), Some("192.168.1.42".to_string()));
    }

    #[test]
    fn device_ip_none_when_only_cellular() {
        let output = "100.109.184.96/27 dev rmnet_data1 proto kernel scope link src 100.109.184.112\n";
        assert_eq!(parse_device_ip(output), None);
    }

    #[test]
    fn parses_du_output() {
        let output = "1234\t./cache\n5678\t.\n";
        assert_eq!(parse_du_output(output), (5678, 1234));
    }

    #[test]
    fn parses_du_output_no_cache() {
        let output = "5678\t.\n";
        assert_eq!(parse_du_output(output), (5678, 0));
    }

    #[test]
    fn parses_du_output_empty() {
        assert_eq!(parse_du_output(""), (0, 0));
    }

    #[test]
    fn parses_avd_list() {
        let output = "Pixel_7_API_34\nMedium_Phone_API_35\n";
        let avds = parse_avd_list(output);
        assert_eq!(avds, vec!["Pixel_7_API_34", "Medium_Phone_API_35"]);
    }

    #[test]
    fn parses_avd_list_empty() {
        assert!(parse_avd_list("").is_empty());
        assert!(parse_avd_list("\n\n").is_empty());
    }

    #[test]
    fn parses_avd_list_with_whitespace() {
        let output = "  Pixel_7_API_34  \n  Medium_Phone  \n";
        let avds = parse_avd_list(output);
        assert_eq!(avds, vec!["Pixel_7_API_34", "Medium_Phone"]);
    }

    #[test]
    fn parsed_devices_are_connected() {
        let output = "List of devices attached\nemulator-5554 device\n\n";
        let devices = parse_device_list(output);
        assert!(devices[0].connected);
    }
}
