use crate::adb::Device;

fn device_label(d: &Device) -> String {
    match (&d.model, &d.device) {
        (Some(model), Some(device)) => format!("{model} ({device})"),
        (Some(model), None) => model.clone(),
        (None, Some(device)) => device.clone(),
        (None, None) => d.serial.clone(),
    }
}

pub fn selector_label(d: &Device) -> String {
    let detail = device_label(d);
    if detail == d.serial {
        d.serial.clone()
    } else {
        format!("{}: {detail}", d.serial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::Device;

    fn make_device(serial: &str, model: Option<&str>, device: Option<&str>) -> Device {
        Device {
            serial: serial.to_string(),
            model: model.map(String::from),
            device: device.map(String::from),
        }
    }

    #[test]
    fn device_label_model_and_device() {
        let d = make_device("ABC", Some("Pixel 7"), Some("panther"));
        assert_eq!(device_label(&d), "Pixel 7 (panther)");
    }

    #[test]
    fn device_label_model_only() {
        let d = make_device("ABC", Some("Pixel 7"), None);
        assert_eq!(device_label(&d), "Pixel 7");
    }

    #[test]
    fn device_label_device_only() {
        let d = make_device("ABC", None, Some("panther"));
        assert_eq!(device_label(&d), "panther");
    }

    #[test]
    fn device_label_serial_fallback() {
        let d = make_device("ABC123", None, None);
        assert_eq!(device_label(&d), "ABC123");
    }

    #[test]
    fn selector_label_with_detail() {
        let d = make_device("ABC123", Some("Pixel 7"), None);
        assert_eq!(selector_label(&d), "ABC123: Pixel 7");
    }

    #[test]
    fn selector_label_serial_only() {
        let d = make_device("ABC123", None, None);
        assert_eq!(selector_label(&d), "ABC123");
    }
}
