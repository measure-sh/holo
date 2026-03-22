mod real;

pub use real::RealAdb;

use std::collections::HashMap;

use color_eyre::Result;

#[derive(Debug, Clone, Default)]
pub struct MemInfo {
    pub total_pss_kb: u64,
    pub java_heap_kb: u64,
    pub native_heap_kb: u64,
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
}
