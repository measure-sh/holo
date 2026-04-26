use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ABIS: &[(&str, &str)] = &[
    ("arm64-v8a", "aarch64-linux-android"),
    ("x86_64", "x86_64-linux-android"),
];
const API: u32 = 28;
const NDK_ENV_VARS: &[&str] = &[
    "ANDROID_NDK_HOME",
    "ANDROID_NDK_ROOT",
    "ANDROID_NDK_LATEST_HOME",
];

fn main() {
    println!("cargo:rerun-if-changed=agent/agent.cpp");
    println!("cargo:rerun-if-changed=agent/jvmti.h");
    println!("cargo:rerun-if-changed=build.rs");
    for var in NDK_ENV_VARS {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let Some(ndk) = find_ndk() else {
        println!(
            "cargo:warning=Android NDK not found (set ANDROID_NDK_HOME); \
             building holo without the JVMTI agent — GC pause monitoring will be disabled"
        );
        for (abi, _) in ABIS {
            fs::write(out_dir.join(format!("libholoagent-{abi}.so")), []).unwrap();
        }
        return;
    };

    let toolchain = ndk.join("toolchains/llvm/prebuilt").join(host_tag());
    assert!(
        toolchain.is_dir(),
        "NDK toolchain not found at {} — wrong host tag?",
        toolchain.display()
    );

    for (abi, triple) in ABIS {
        build_agent(&toolchain, abi, triple, &out_dir);
    }
}

fn find_ndk() -> Option<PathBuf> {
    for var in NDK_ENV_VARS {
        if let Ok(val) = env::var(var) {
            let p = PathBuf::from(val);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let home = env::var("HOME").ok()?;
    for root in [
        PathBuf::from(&home).join("Library/Android/sdk/ndk"),
        PathBuf::from(&home).join("Android/Sdk/ndk"),
    ] {
        if let Some(latest) = newest_subdir(&root) {
            return Some(latest);
        }
    }
    None
}

fn newest_subdir(root: &Path) -> Option<PathBuf> {
    let mut versions: Vec<_> = fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    versions.sort();
    versions.pop()
}

fn host_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        panic!("unsupported host for Android NDK toolchain");
    }
}

fn build_agent(toolchain: &Path, abi: &str, triple: &str, out_dir: &Path) {
    let clang_name = format!("{triple}{API}-clang++");
    let clang = toolchain.join("bin").join(if cfg!(target_os = "windows") {
        format!("{clang_name}.cmd")
    } else {
        clang_name
    });
    let so_path = out_dir.join(format!("libholoagent-{abi}.so"));

    // Flags mirror agent/build.sh and are load-bearing for 16KB-page + ART
    // compatibility. Don't drop -fno-exceptions/-fno-rtti or the -Wl,-z,*
    // page-size and segment flags.
    let status = Command::new(&clang)
        .args([
            "-std=c++17",
            "-fPIC",
            "-shared",
            "-O2",
            "-g",
            "-Wall",
            "-Wextra",
            "-fno-exceptions",
            "-fno-rtti",
            "-nostdlib++",
            "-fvisibility=hidden",
            "-Wl,-z,max-page-size=16384",
            "-Wl,-z,common-page-size=16384",
            "-Wl,-z,separate-loadable-segments",
            "-Wl,-z,now",
            "-Wl,-z,relro",
            "-Wl,--exclude-libs,ALL",
            "-Iagent",
        ])
        .arg("-o")
        .arg(&so_path)
        .arg("agent/agent.cpp")
        .arg("-llog")
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke {}: {e}", clang.display()));
    assert!(status.success(), "agent compile for {abi} failed");

    let strip = toolchain.join("bin/llvm-strip");
    let strip_status = Command::new(&strip).arg(&so_path).status();
    assert!(
        matches!(strip_status, Ok(s) if s.success()),
        "llvm-strip failed for {abi}"
    );
}
