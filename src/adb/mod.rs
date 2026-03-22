mod real;

pub use real::RealAdb;

use std::collections::HashMap;

use color_eyre::Result;

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
    fn list_databases(&self, serial: &str, package: &str) -> Result<Vec<String>>;
    fn query_database(&self, serial: &str, package: &str, db_name: &str, sql: &str) -> Result<String>;
    fn pull_database(&self, serial: &str, package: &str, db_name: &str, dest: &std::path::Path) -> Result<()>;
    fn wake_screen(&self, serial: &str) -> Result<()>;
    fn get_layout_bounds(&self, serial: &str) -> Result<bool>;
    fn set_layout_bounds(&self, serial: &str, enabled: bool) -> Result<()>;
    fn get_airplane_mode(&self, serial: &str) -> Result<bool>;
    fn set_airplane_mode(&self, serial: &str, enabled: bool) -> Result<()>;
}
