mod real;

pub use real::RealAdb;

use color_eyre::Result;

#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub model: Option<String>,
    pub device: Option<String>,
}

pub trait Adb {
    fn list_devices(&self) -> Result<Vec<Device>>;
}
