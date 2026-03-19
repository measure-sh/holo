mod real;

pub use real::RealAdb;

use color_eyre::Result;

#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub description: String,
}

pub trait Adb {
    fn list_devices(&self) -> Result<Vec<Device>>;
}
