mod real;

pub use real::RealAdb;

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
}
