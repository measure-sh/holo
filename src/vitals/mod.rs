mod attach;
mod blobs;
mod reader;

pub use reader::VitalsHandle;

pub(crate) use attach::attach_running;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VitalsEvent {
    Gc { ts_ns: i64, duration_us: u32 },
}

pub(crate) const KIND_GC: u8 = 0x01;

pub(crate) fn decode_payload(kind: u8, payload: &[u8]) -> Option<VitalsEvent> {
    match kind {
        KIND_GC => {
            if payload.len() != 12 {
                return None;
            }
            let ts_ns = i64::from_be_bytes(payload[0..8].try_into().ok()?);
            let duration_us = u32::from_be_bytes(payload[8..12].try_into().ok()?);
            Some(VitalsEvent::Gc { ts_ns, duration_us })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_gc_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1234567890i64.to_be_bytes());
        payload.extend_from_slice(&12_345u32.to_be_bytes());
        assert_eq!(
            decode_payload(KIND_GC, &payload).unwrap(),
            VitalsEvent::Gc { ts_ns: 1234567890, duration_us: 12_345 },
        );
    }

    #[test]
    fn ignores_unknown_kind() {
        assert!(decode_payload(0xff, &[0u8; 12]).is_none());
    }

    #[test]
    fn ignores_wrong_length() {
        assert!(decode_payload(KIND_GC, &[0u8; 8]).is_none());
        assert!(decode_payload(KIND_GC, &[0u8; 16]).is_none());
    }
}
