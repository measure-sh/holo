mod real;

pub use real::RealAdb;

use color_eyre::Result;

#[derive(Debug, Clone, Default)]
pub struct MemInfo {
    pub rss_kb: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkBytes {
    pub rx: u64,
    pub tx: u64,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub model: Option<String>,
    pub device: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub size_bytes: u64,
    pub modified: Option<String>,
    pub mode: String,
}

pub trait Adb: Send + Sync {
    fn list_devices(&self) -> Result<Vec<Device>>;
    fn get_battery_level(&self, serial: &str) -> Result<u8>;
    fn list_packages(&self, serial: &str) -> Result<Vec<String>>;
    fn pidof(&self, serial: &str, package: &str) -> Result<Option<u32>>;
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
    fn stat_file(&self, serial: &str, package: &str, remote_path: &str) -> Result<FileMeta>;
    fn cat_file(&self, serial: &str, package: &str, remote_path: &str, max_bytes: u64) -> Result<Vec<u8>>;
    fn get_meminfo(&self, serial: &str, package: &str) -> Result<MemInfo>;
    fn get_cpu_usage(&self, serial: &str, package: &str) -> Result<f32>;
    fn start_trace(&self, serial: &str, config: &str) -> Result<()>;
    fn stop_and_pull_trace(&self, serial: &str, dest: &std::path::Path) -> Result<()>;
    fn take_screenshot(&self, serial: &str, dest: &std::path::Path) -> Result<()>;
    fn get_wifi_enabled(&self, serial: &str) -> Result<bool>;
    fn set_wifi_enabled(&self, serial: &str, enabled: bool) -> Result<()>;
    fn enable_wireless_adb(&self, serial: &str) -> Result<String>;
    fn get_disk_usage(&self, serial: &str, package: &str) -> Result<(u64, u64)>;
    fn get_dark_mode(&self, serial: &str) -> Result<bool>;
    fn set_dark_mode(&self, serial: &str, enabled: bool) -> Result<()>;
    fn list_avds(&self) -> Result<Vec<String>>;
    fn launch_emulator(&self, avd_name: &str) -> Result<()>;
    fn get_avd_name(&self, serial: &str) -> Result<String>;
    fn get_state(&self, serial: &str) -> Result<String>;
    fn is_debuggable(&self, serial: &str, package: &str) -> bool;
    fn get_show_taps(&self, serial: &str) -> Result<bool>;
    fn set_show_taps(&self, serial: &str, enabled: bool) -> Result<()>;
    fn get_pointer_location(&self, serial: &str) -> Result<bool>;
    fn set_pointer_location(&self, serial: &str, enabled: bool) -> Result<()>;
    fn get_gpu_rendering(&self, serial: &str) -> Result<bool>;
    fn set_gpu_rendering(&self, serial: &str, enabled: bool) -> Result<()>;
    fn get_talkback_enabled(&self, serial: &str) -> Result<bool>;
    fn set_talkback_enabled(&self, serial: &str, enabled: bool) -> Result<()>;
    fn get_dropbox_crashes(&self, serial: &str) -> Result<String>;
    fn get_dropbox_anrs(&self, serial: &str) -> Result<String>;
    fn has_measure_sdk(&self, serial: &str, package: &str) -> bool;
    fn get_network_bytes(&self, serial: &str, package: &str) -> Result<NetworkBytes>;
}
