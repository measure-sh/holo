const ARM64_V8A: &[u8] = include_bytes!("../../agent/prebuilt/arm64-v8a/libholoagent.so");
const X86_64: &[u8] = include_bytes!("../../agent/prebuilt/x86_64/libholoagent.so");

pub fn for_abi(abi: &str) -> Option<&'static [u8]> {
    match abi {
        "arm64-v8a" => Some(ARM64_V8A),
        "x86_64" => Some(X86_64),
        _ => None,
    }
}
