use crate::adb::Device;

pub enum BootResult {
    NoDevices,
    Selected(Device),
    NeedsSelection(Vec<Device>),
}

pub fn resolve_device(devices: Vec<Device>) -> BootResult {
    match devices.len() {
        0 => BootResult::NoDevices,
        1 => BootResult::Selected(devices.into_iter().next().unwrap()),
        _ => BootResult::NeedsSelection(devices),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_devices(n: usize) -> Vec<Device> {
        (0..n)
            .map(|i| Device {
                serial: format!("device-{i}"),
                model: None,
                device: None,
            })
            .collect()
    }

    #[test]
    fn no_devices_returns_no_devices() {
        let result = resolve_device(vec![]);
        assert!(matches!(result, BootResult::NoDevices));
    }

    #[test]
    fn single_device_is_auto_selected() {
        let devices = make_devices(1);
        let result = resolve_device(devices);
        match result {
            BootResult::Selected(d) => assert_eq!(d.serial, "device-0"),
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn multiple_devices_need_selection() {
        let devices = make_devices(3);
        let result = resolve_device(devices);
        match result {
            BootResult::NeedsSelection(ds) => assert_eq!(ds.len(), 3),
            _ => panic!("expected NeedsSelection"),
        }
    }
}
