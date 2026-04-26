const ARM64_V8A: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libholoagent-arm64-v8a.so"));
const X86_64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libholoagent-x86_64.so"));

pub fn for_abi(abi: &str) -> Option<&'static [u8]> {
    let bytes = match abi {
        "arm64-v8a" => ARM64_V8A,
        "x86_64" => X86_64,
        _ => return None,
    };
    if bytes.is_empty() { None } else { Some(bytes) }
}
