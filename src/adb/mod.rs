mod real;

pub use real::RealAdb;

use std::collections::HashMap;

use color_eyre::Result;

#[derive(Debug, Clone, Default)]
pub struct MemInfo {
    pub rss_kb: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GfxInfo {
    pub total_frames: u64,
    pub slow_frames: u64,
    pub frozen_frames: u64,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub model: Option<String>,
    pub device: Option<String>,
}

pub trait Adb: Send + Sync {
    fn list_devices(&self) -> Result<Vec<Device>>;
    fn get_battery_level(&self, serial: &str) -> Result<u8>;
    fn list_packages(&self, serial: &str) -> Result<Vec<String>>;
    fn list_processes(&self, serial: &str) -> Result<HashMap<String, u32>>;
    fn launch_app(&self, serial: &str, package: &str) -> Result<()>;
    fn kill_app(&self, serial: &str, package: &str) -> Result<()>;
    fn clear_app_data(&self, serial: &str, package: &str) -> Result<()>;
    fn uninstall_app(&self, serial: &str, package: &str) -> Result<()>;
    fn list_databases(&self, serial: &str, package: &str) -> Result<Vec<String>>;
    fn query_database(&self, serial: &str, package: &str, db_name: &str, sql: &str) -> Result<String>;
    fn pull_database(&self, serial: &str, package: &str, db_name: &str, dest: &std::path::Path) -> Result<()>;
    fn wake_screen(&self, serial: &str) -> Result<()>;
    fn get_layout_bounds(&self, serial: &str) -> Result<bool>;
    fn set_layout_bounds(&self, serial: &str, enabled: bool) -> Result<()>;
    fn get_airplane_mode(&self, serial: &str) -> Result<bool>;
    fn set_airplane_mode(&self, serial: &str, enabled: bool) -> Result<()>;
    fn list_permissions(&self, serial: &str, package: &str) -> Result<Vec<(String, bool)>>;
    fn grant_permission(&self, serial: &str, package: &str, permission: &str) -> Result<()>;
    fn revoke_permission(&self, serial: &str, package: &str, permission: &str) -> Result<()>;
    fn get_app_version(&self, serial: &str, package: &str) -> Result<(String, String)>;
    fn list_files(&self, serial: &str, package: &str, path: &str) -> Result<Vec<(String, bool)>>;
    fn pull_file(&self, serial: &str, package: &str, remote_path: &str, dest: &std::path::Path) -> Result<()>;
    fn get_meminfo(&self, serial: &str, package: &str) -> Result<MemInfo>;
    fn get_cpu_usage(&self, serial: &str, package: &str) -> Result<f32>;
    fn get_gfx_info(&self, serial: &str, package: &str) -> Result<GfxInfo>;
    fn start_trace(&self, serial: &str, config: &str) -> Result<()>;
    fn stop_and_pull_trace(&self, serial: &str, dest: &std::path::Path) -> Result<()>;
    fn take_screenshot(&self, serial: &str, dest: &std::path::Path) -> Result<()>;
    fn get_wifi_enabled(&self, serial: &str) -> Result<bool>;
    fn set_wifi_enabled(&self, serial: &str, enabled: bool) -> Result<()>;
    fn enable_wireless_adb(&self, serial: &str) -> Result<String>;
    fn get_disk_usage(&self, serial: &str, package: &str) -> Result<(u64, u64)>;
}
