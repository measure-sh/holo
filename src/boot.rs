use ratatui::widgets::ListState;

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

pub struct DeviceSelector {
    pub devices: Vec<Device>,
    pub list_state: ListState,
}

impl DeviceSelector {
    pub fn new(devices: Vec<Device>) -> Self {
        let mut list_state = ListState::default();
        if !devices.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            devices,
            list_state,
        }
    }

    pub fn select_next(&mut self) {
        if self.devices.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((i + 1).min(self.devices.len() - 1)));
    }

    pub fn select_previous(&mut self) {
        if self.devices.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(1)));
    }

    pub fn confirm(&self) -> Device {
        let i = self.list_state.selected().unwrap_or(0);
        self.devices[i].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_devices(n: usize) -> Vec<Device> {
        (0..n)
            .map(|i| Device {
                serial: format!("device-{i}"),
                description: String::new(),
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

    #[test]
    fn selector_starts_at_first_device() {
        let selector = DeviceSelector::new(make_devices(3));
        assert_eq!(selector.list_state.selected(), Some(0));
    }

    #[test]
    fn selector_navigates_down() {
        let mut selector = DeviceSelector::new(make_devices(3));
        selector.select_next();
        assert_eq!(selector.list_state.selected(), Some(1));
        selector.select_next();
        assert_eq!(selector.list_state.selected(), Some(2));
    }

    #[test]
    fn selector_does_not_go_past_last_device() {
        let mut selector = DeviceSelector::new(make_devices(2));
        selector.select_next();
        selector.select_next();
        selector.select_next();
        assert_eq!(selector.list_state.selected(), Some(1));
    }

    #[test]
    fn selector_does_not_go_before_first_device() {
        let mut selector = DeviceSelector::new(make_devices(3));
        selector.select_previous();
        assert_eq!(selector.list_state.selected(), Some(0));
    }

    #[test]
    fn selector_confirm_returns_selected_device() {
        let mut selector = DeviceSelector::new(make_devices(3));
        selector.select_next();
        let device = selector.confirm();
        assert_eq!(device.serial, "device-1");
    }
}
