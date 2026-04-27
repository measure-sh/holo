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
        java_heap_kb: u64,
        native_heap_kb: u64,
    },
    Cpu { ts_ns: i64, cpu_centi_percent: u32, num_threads: u32 },
    Network { ts_ns: i64, rx_bytes: u64, tx_bytes: u64 },
}

pub(crate) const KIND_GC: u8 = 0x01;
pub(crate) const KIND_MEMORY: u8 = 0x02;
pub(crate) const KIND_CPU: u8 = 0x03;
pub(crate) const KIND_NETWORK: u8 = 0x04;

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
            if payload.len() != 20 {
                return None;
            }
            let ts_ns = i64::from_be_bytes(payload[0..8].try_into().ok()?);
            let rss_kb = u32::from_be_bytes(payload[8..12].try_into().ok()?) as u64;
            let java_heap_kb = u32::from_be_bytes(payload[12..16].try_into().ok()?) as u64;
            let native_heap_kb = u32::from_be_bytes(payload[16..20].try_into().ok()?) as u64;
            Some(VitalsEvent::Memory { ts_ns, rss_kb, java_heap_kb, native_heap_kb })
        }
        KIND_CPU => {
            if payload.len() != 16 {
                return None;
            }
            let ts_ns = i64::from_be_bytes(payload[0..8].try_into().ok()?);
            let cpu_centi_percent = u32::from_be_bytes(payload[8..12].try_into().ok()?);
            let num_threads = u32::from_be_bytes(payload[12..16].try_into().ok()?);
            Some(VitalsEvent::Cpu { ts_ns, cpu_centi_percent, num_threads })
        }
        KIND_NETWORK => {
            if payload.len() != 24 {
                return None;
            }
            let ts_ns = i64::from_be_bytes(payload[0..8].try_into().ok()?);
            let rx_bytes = u64::from_be_bytes(payload[8..16].try_into().ok()?);
            let tx_bytes = u64::from_be_bytes(payload[16..24].try_into().ok()?);
            Some(VitalsEvent::Network { ts_ns, rx_bytes, tx_bytes })
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
    fn decodes_memory_payload() {
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
                java_heap_kb: 200,
                native_heap_kb: 300,
            },
        );
    }

    #[test]
    fn ignores_wrong_memory_payload_length() {
        assert!(decode_payload(KIND_MEMORY, &[0u8; 12]).is_none());
        assert!(decode_payload(KIND_MEMORY, &[0u8; 16]).is_none());
        assert!(decode_payload(KIND_MEMORY, &[0u8; 24]).is_none());
    }

    #[test]
    fn decodes_cpu_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&7_777_777i64.to_be_bytes());
        payload.extend_from_slice(&4321u32.to_be_bytes());
        payload.extend_from_slice(&42u32.to_be_bytes());
        assert_eq!(
            decode_payload(KIND_CPU, &payload).unwrap(),
            VitalsEvent::Cpu { ts_ns: 7_777_777, cpu_centi_percent: 4321, num_threads: 42 },
        );
    }

    #[test]
    fn ignores_wrong_cpu_payload_length() {
        assert!(decode_payload(KIND_CPU, &[0u8; 12]).is_none());
        assert!(decode_payload(KIND_CPU, &[0u8; 20]).is_none());
    }

    #[test]
    fn decodes_network_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&42i64.to_be_bytes());
        payload.extend_from_slice(&1_000_000u64.to_be_bytes());
        payload.extend_from_slice(&2_500_000u64.to_be_bytes());
        assert_eq!(
            decode_payload(KIND_NETWORK, &payload).unwrap(),
            VitalsEvent::Network { ts_ns: 42, rx_bytes: 1_000_000, tx_bytes: 2_500_000 },
        );
    }

    #[test]
    fn ignores_wrong_network_payload_length() {
        assert!(decode_payload(KIND_NETWORK, &[0u8; 16]).is_none());
        assert!(decode_payload(KIND_NETWORK, &[0u8; 32]).is_none());
    }
}
