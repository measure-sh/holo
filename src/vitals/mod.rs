mod attach;
mod blobs;
mod reader;

pub use reader::VitalsHandle;

pub(crate) use attach::attach_running;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VitalsEvent {
    Gc { ts_ns: i64, duration_us: u32 },
    Memory {
        ts_ns: i64,
        rss_kb: u64,
        java_heap_kb: Option<u64>,
        native_heap_kb: Option<u64>,
    },
}

pub(crate) const KIND_GC: u8 = 0x01;
pub(crate) const KIND_MEMORY: u8 = 0x02;

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
        KIND_MEMORY => {
            // Mandatory: ts_ns + rss_kb. Trailing u32s are forward-compatible
            // slots for java_heap_kb / native_heap_kb that older agents simply
            // omit. New agents will set the bytes; old hosts ignore them.
            if payload.len() < 12 {
                return None;
            }
            let ts_ns = i64::from_be_bytes(payload[0..8].try_into().ok()?);
            let rss_kb = u32::from_be_bytes(payload[8..12].try_into().ok()?) as u64;
            let java_heap_kb = (payload.len() >= 16)
                .then(|| u32::from_be_bytes(payload[12..16].try_into().ok().unwrap()) as u64);
            let native_heap_kb = (payload.len() >= 20)
                .then(|| u32::from_be_bytes(payload[16..20].try_into().ok().unwrap()) as u64);
            Some(VitalsEvent::Memory { ts_ns, rss_kb, java_heap_kb, native_heap_kb })
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

    #[test]
    fn decodes_memory_v1_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&42_000i64.to_be_bytes());
        payload.extend_from_slice(&98_765u32.to_be_bytes());
        assert_eq!(
            decode_payload(KIND_MEMORY, &payload).unwrap(),
            VitalsEvent::Memory {
                ts_ns: 42_000,
                rss_kb: 98_765,
                java_heap_kb: None,
                native_heap_kb: None,
            },
        );
    }

    #[test]
    fn decodes_memory_with_trailing_heap_fields() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1i64.to_be_bytes());
        payload.extend_from_slice(&100u32.to_be_bytes()); // rss
        payload.extend_from_slice(&200u32.to_be_bytes()); // java
        payload.extend_from_slice(&300u32.to_be_bytes()); // native
        assert_eq!(
            decode_payload(KIND_MEMORY, &payload).unwrap(),
            VitalsEvent::Memory {
                ts_ns: 1,
                rss_kb: 100,
                java_heap_kb: Some(200),
                native_heap_kb: Some(300),
            },
        );
    }

    #[test]
    fn ignores_short_memory_payload() {
        assert!(decode_payload(KIND_MEMORY, &[0u8; 8]).is_none());
    }
}
