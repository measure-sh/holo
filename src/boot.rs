use crossterm::event::KeyCode;

use crate::adb::Device;
use crate::apps;

pub enum BootPhase {
    WaitingForDevice,
    SelectDevice {
        devices: Vec<Device>,
        cursor: usize,
    },
    SelectApp {
        device: Device,
        packages: Option<Vec<String>>,
        cursor: usize,
        filter: String,
    },
    Done {
        device: Device,
        package: String,
    },
}

pub enum BootAction {
    Continue,
    Quit,
}

impl BootPhase {
    pub fn handle_key(&mut self, code: KeyCode) -> BootAction {
        match self {
            Self::WaitingForDevice => match code {
                KeyCode::Char('q') => BootAction::Quit,
                _ => BootAction::Continue,
            },
            Self::SelectDevice { devices, cursor } => match code {
                KeyCode::Char('q') => BootAction::Quit,
                KeyCode::Up => {
                    *cursor = cursor.saturating_sub(1);
                    BootAction::Continue
                }
                KeyCode::Down => {
                    *cursor = (*cursor + 1).min(devices.len().saturating_sub(1));
                    BootAction::Continue
                }
                KeyCode::Enter => {
                    let device = devices[*cursor].clone();
                    *self = Self::SelectApp {
                        device,
                        packages: None,
                        cursor: 0,
                        filter: String::new(),
                    };
                    BootAction::Continue
                }
                _ => BootAction::Continue,
            },
            Self::SelectApp {
                device,
                packages,
                cursor,
                filter,
            } => match code {
                KeyCode::Char('q') if filter.is_empty() => BootAction::Quit,
                KeyCode::Esc => {
                    filter.clear();
                    *cursor = 0;
                    BootAction::Continue
                }
                KeyCode::Backspace => {
                    filter.pop();
                    *cursor = 0;
                    BootAction::Continue
                }
                KeyCode::Up => {
                    *cursor = cursor.saturating_sub(1);
                    BootAction::Continue
                }
                KeyCode::Down => {
                    let count = filtered_count(packages.as_deref(), filter);
                    *cursor = (*cursor + 1).min(count.saturating_sub(1));
                    BootAction::Continue
                }
                KeyCode::Enter => {
                    let count = filtered_count(packages.as_deref(), filter);
                    if count > 0 {
                        if let Some(pkgs) = packages.as_ref() {
                            let filtered = apps::filtered_packages(pkgs, filter);
                            if let Some(&pkg) = filtered.get(*cursor) {
                                *self = Self::Done {
                                    device: device.clone(),
                                    package: pkg.to_string(),
                                };
                            }
                        }
                    }
                    BootAction::Continue
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    *cursor = 0;
                    BootAction::Continue
                }
                _ => BootAction::Continue,
            },
            Self::Done { .. } => BootAction::Continue,
        }
    }

    pub fn receive_devices(&mut self, devices: Vec<Device>) {
        if !matches!(self, Self::WaitingForDevice) {
            return;
        }
        match devices.len() {
            0 => {}
            1 => {
                *self = Self::SelectApp {
                    device: devices.into_iter().next().unwrap(),
                    packages: None,
                    cursor: 0,
                    filter: String::new(),
                };
            }
            _ => {
                *self = Self::SelectDevice {
                    devices,
                    cursor: 0,
                };
            }
        }
    }

    pub fn receive_packages(&mut self, pkgs: Vec<String>) {
        if let Self::SelectApp { packages, .. } = self {
            *packages = Some(pkgs);
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done { .. })
    }

    pub fn into_result(self) -> Option<(Device, String)> {
        if let Self::Done { device, package } = self {
            Some((device, package))
        } else {
            None
        }
    }
}

fn filtered_count(packages: Option<&[String]>, filter: &str) -> usize {
    packages.map_or(0, |pkgs| apps::filtered_packages(pkgs, filter).len())
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
    fn waiting_stays_on_zero_devices() {
        let mut phase = BootPhase::WaitingForDevice;
        phase.receive_devices(vec![]);
        assert!(matches!(phase, BootPhase::WaitingForDevice));
    }

    #[test]
    fn waiting_advances_to_select_app_on_single_device() {
        let mut phase = BootPhase::WaitingForDevice;
        phase.receive_devices(make_devices(1));
        match &phase {
            BootPhase::SelectApp { device, .. } => assert_eq!(device.serial, "device-0"),
            _ => panic!("expected SelectApp"),
        }
    }

    #[test]
    fn waiting_advances_to_select_device_on_multiple() {
        let mut phase = BootPhase::WaitingForDevice;
        phase.receive_devices(make_devices(3));
        match &phase {
            BootPhase::SelectDevice { devices, cursor } => {
                assert_eq!(devices.len(), 3);
                assert_eq!(*cursor, 0);
            }
            _ => panic!("expected SelectDevice"),
        }
    }

    #[test]
    fn waiting_quits_on_q() {
        let mut phase = BootPhase::WaitingForDevice;
        assert!(matches!(
            phase.handle_key(KeyCode::Char('q')),
            BootAction::Quit
        ));
    }

    #[test]
    fn waiting_ignores_esc() {
        let mut phase = BootPhase::WaitingForDevice;
        assert!(matches!(
            phase.handle_key(KeyCode::Esc),
            BootAction::Continue
        ));
    }

    #[test]
    fn select_device_moves_cursor() {
        let mut phase = BootPhase::SelectDevice {
            devices: make_devices(3),
            cursor: 0,
        };
        phase.handle_key(KeyCode::Down);
        if let BootPhase::SelectDevice { cursor, .. } = &phase {
            assert_eq!(*cursor, 1);
        }
        phase.handle_key(KeyCode::Down);
        if let BootPhase::SelectDevice { cursor, .. } = &phase {
            assert_eq!(*cursor, 2);
        }
        phase.handle_key(KeyCode::Down);
        if let BootPhase::SelectDevice { cursor, .. } = &phase {
            assert_eq!(*cursor, 2);
        }
        phase.handle_key(KeyCode::Up);
        if let BootPhase::SelectDevice { cursor, .. } = &phase {
            assert_eq!(*cursor, 1);
        }
    }

    #[test]
    fn select_device_enter_advances_to_select_app() {
        let mut phase = BootPhase::SelectDevice {
            devices: make_devices(2),
            cursor: 1,
        };
        phase.handle_key(KeyCode::Enter);
        match &phase {
            BootPhase::SelectApp { device, .. } => assert_eq!(device.serial, "device-1"),
            _ => panic!("expected SelectApp"),
        }
    }

    #[test]
    fn select_device_quits_on_q() {
        let mut phase = BootPhase::SelectDevice {
            devices: make_devices(2),
            cursor: 0,
        };
        assert!(matches!(
            phase.handle_key(KeyCode::Char('q')),
            BootAction::Quit
        ));
    }

    #[test]
    fn select_app_typing_builds_filter() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec!["com.example.app".to_string()]),
            cursor: 0,
            filter: String::new(),
        };
        phase.handle_key(KeyCode::Char('c'));
        phase.handle_key(KeyCode::Char('o'));
        if let BootPhase::SelectApp { filter, .. } = &phase {
            assert_eq!(filter, "co");
        }
    }

    #[test]
    fn select_app_backspace_removes_char() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec!["com.example.app".to_string()]),
            cursor: 0,
            filter: "co".to_string(),
        };
        phase.handle_key(KeyCode::Backspace);
        if let BootPhase::SelectApp { filter, .. } = &phase {
            assert_eq!(filter, "c");
        }
    }

    #[test]
    fn select_app_esc_clears_filter() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec!["com.example.app".to_string()]),
            cursor: 0,
            filter: "co".to_string(),
        };
        phase.handle_key(KeyCode::Esc);
        if let BootPhase::SelectApp { filter, cursor, .. } = &phase {
            assert_eq!(filter, "");
            assert_eq!(*cursor, 0);
        }
    }

    #[test]
    fn select_app_esc_ignored_when_filter_empty() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec!["com.example.app".to_string()]),
            cursor: 0,
            filter: String::new(),
        };
        assert!(matches!(
            phase.handle_key(KeyCode::Esc),
            BootAction::Continue
        ));
    }

    #[test]
    fn select_app_enter_selects_package() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec![
                "com.alpha".to_string(),
                "com.beta".to_string(),
            ]),
            cursor: 1,
            filter: String::new(),
        };
        phase.handle_key(KeyCode::Enter);
        match &phase {
            BootPhase::Done { package, .. } => assert_eq!(package, "com.beta"),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn select_app_enter_does_nothing_without_packages() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: None,
            cursor: 0,
            filter: String::new(),
        };
        phase.handle_key(KeyCode::Enter);
        assert!(matches!(phase, BootPhase::SelectApp { .. }));
    }

    #[test]
    fn select_app_cursor_moves() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ]),
            cursor: 0,
            filter: String::new(),
        };
        phase.handle_key(KeyCode::Down);
        if let BootPhase::SelectApp { cursor, .. } = &phase {
            assert_eq!(*cursor, 1);
        }
        phase.handle_key(KeyCode::Up);
        if let BootPhase::SelectApp { cursor, .. } = &phase {
            assert_eq!(*cursor, 0);
        }
    }

    #[test]
    fn select_app_cursor_resets_on_type() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: Some(vec![
                "com.alpha".to_string(),
                "com.beta".to_string(),
            ]),
            cursor: 1,
            filter: String::new(),
        };
        phase.handle_key(KeyCode::Char('a'));
        if let BootPhase::SelectApp { cursor, .. } = &phase {
            assert_eq!(*cursor, 0);
        }
    }

    #[test]
    fn receive_devices_ignored_when_not_waiting() {
        let mut phase = BootPhase::SelectDevice {
            devices: make_devices(2),
            cursor: 0,
        };
        phase.receive_devices(make_devices(5));
        if let BootPhase::SelectDevice { devices, .. } = &phase {
            assert_eq!(devices.len(), 2);
        }
    }

    #[test]
    fn receive_packages_updates_select_app() {
        let mut phase = BootPhase::SelectApp {
            device: make_devices(1).remove(0),
            packages: None,
            cursor: 0,
            filter: String::new(),
        };
        phase.receive_packages(vec!["com.test".to_string()]);
        if let BootPhase::SelectApp { packages, .. } = &phase {
            assert_eq!(packages.as_ref().unwrap().len(), 1);
        }
    }

    #[test]
    fn done_reports_is_done() {
        let phase = BootPhase::Done {
            device: make_devices(1).remove(0),
            package: "com.test".to_string(),
        };
        assert!(phase.is_done());
    }

    #[test]
    fn into_result_extracts_device_and_package() {
        let phase = BootPhase::Done {
            device: make_devices(1).remove(0),
            package: "com.test".to_string(),
        };
        let (device, package) = phase.into_result().unwrap();
        assert_eq!(device.serial, "device-0");
        assert_eq!(package, "com.test");
    }
}
