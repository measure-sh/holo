# JVMTI Agent

Holo's Monitor panel shows live CPU%, thread count, Java / native / RSS
memory, GC pause marks, and per-app rx/tx bytes. All of it comes from a
tiny C++ agent (`libholoagent.so`) that Holo loads into the target app on
the device. This doc explains, in plain terms, how that works end to end.

## Supported configurations

- **ABIs:** `arm64-v8a` and `x86_64` only. On any other ABI (e.g.
  `armeabi-v7a`, `x86`) Holo quietly skips vitals — the Monitor panel
  shows nothing for CPU, memory, threads, or GC.
- **Debuggable apps only:** the target must ship with
  `android:debuggable="true"`. Release builds are skipped by design.

## What you see in the UI

When you focus an app, Holo checks if it's debuggable. If it is, the Monitor
panel starts streaming once the agent is attached:

- **CPU view** — process CPU% averaged across cores (utime+stime delta from
  `/proc/self/stat`), with the live thread count from the same file shown on
  the chip row (`23 threads`).
- **Memory view** — three lines on the same chart: Java heap
  (`Runtime.totalMemory() - freeMemory()`), native heap
  (`Debug.getNativeHeapAllocatedSize()`), and RSS (from `/proc/self/statm`).
- **Network view** — download / upload rate in B/s plus cumulative totals
  (`↓ 1.2 KB/s (4.5 MB)` / `↑ 200 B/s (180 KB)`) sourced from
  `TrafficStats.getUidRxBytes(myUid())` / `getUidTxBytes(myUid())`.
- **GC marks** — every time ART runs a garbage collection, a `◆` mark shows
  up at the moment the GC finished. The chip row shows the count in the
  visible window (`◆ 7 GC`).

All five signals share the agent's `CLOCK_MONOTONIC` clock, so GC marks line
up with the CPU, memory, and network curves they correspond to.

Nothing shows up for non-debuggable (release) builds. That is by design —
the OS only lets us inject code into apps that opted in via
`android:debuggable="true"`.

## The pieces

```
   ┌──────────────┐                         ┌──────────────────────┐
   │  Holo (host) │                         │  target app (device) │
   │              │                         │                      │
   │  vitals/     │   adb push + run-as     │   /data/data/<pkg>/  │
   │   reader.rs  │ ──────────────────────▶ │   libholoagent.so    │
   │              │                         │                      │
   │              │   cmd activity          │   ART loads .so via  │
   │              │   attach-agent          │   Agent_OnAttach()   │
   │              │ ──────────────────────▶ │                      │
   │              │                         │   bind() abstract    │
   │              │                         │   socket             │
   │              │                         │   @holoagent-<pid>   │
   │              │                         │                      │
   │              │   adb forward           │                      │
   │              │   tcp:0 localabstract   │                      │
   │              │ ──────────────────────▶ │                      │
   │              │                         │                      │
   │  TCP read    │ ◀───── binary frames ── │  sampler thread      │
   │  loop        │                         │  + GC callbacks      │
   │              │                         │  enqueue + serve     │
   └──────────────┘                         └──────────────────────┘
```

There are three things at play:

1. **The agent** — a ~50 KB native library (`agent/agent.cpp`, cross-compiled
   into the holo binary at build time — see below) that runs _inside_ the
   app process. Two producers feed a small ring buffer:
   - **JVMTI GC callbacks** — `GarbageCollectionStart` /
     `GarbageCollectionFinish` time each pause and enqueue a GC frame.
   - **A 1 Hz sampler thread** — reads `/proc/self/stat` for CPU jiffies
     and thread count, `/proc/self/statm` for RSS, and JNI-calls
     `Runtime.totalMemory()` / `freeMemory()` /
     `Debug.getNativeHeapAllocatedSize()` for Java and native heap, plus
     `TrafficStats.getUidRxBytes(myUid())` / `getUidTxBytes(myUid())` for
     cumulative per-app network bytes.
   A background pthread serves the ring over an abstract Unix socket.

2. **The transport** — an abstract Unix socket named `@holoagent-<pid>`
   bound by the agent. Holo bridges it to a host loopback port using
   `adb forward tcp:0 localabstract:holoagent-<pid>`. The host then opens a
   regular TCP connection to that port.

3. **The host reader** — `src/vitals/reader.rs` reads length-prefixed
   binary frames off the TCP socket, decodes them into `VitalsEvent::{Gc,
   Cpu, Memory, Network}` values, which `data.rs` forwards into the
   Monitor state for rendering.

## Wire format

Frames are big-endian: `[u8 kind][u32 payload_len][payload]`.

| Kind   | Name         | Payload                                                                     |
| ------ | ------------ | --------------------------------------------------------------------------- |
| `0x01` | GC pause     | `[i64 ts_ns][u32 duration_us]` (12 bytes)                                   |
| `0x02` | MemorySample | `[i64 ts_ns][u32 rss_kb][u32 java_heap_kb][u32 native_heap_kb]` (20 bytes)  |
| `0x03` | CpuSample    | `[i64 ts_ns][u32 cpu_centi_percent][u32 num_threads]` (16 bytes)            |
| `0x04` | NetworkSample| `[i64 ts_ns][u64 rx_bytes][u64 tx_bytes]` (24 bytes)                        |

`ts_ns` is the agent's `CLOCK_MONOTONIC` reading at the moment the event was
produced. `cpu_centi_percent` is the divided-by-cores percentage × 100
(range `0..=10000`). `rx_bytes` / `tx_bytes` are cumulative per-uid totals
since boot (host computes bps deltas). The host enforces strict
payload-length checks; agent and host ship together via `build.rs`, so
there is no version negotiation.

## How it's built and bundled

`build.rs` cross-compiles `agent/agent.cpp` for both ABIs at `cargo build`
time using the Android NDK's clang. The resulting `.so` files land in
`OUT_DIR` and are embedded into the holo binary via `include_bytes!` (see
`src/vitals/blobs.rs`). Nothing is committed to git — there is no
`agent/prebuilt/`, and there is no separate build step for the agent.

The build is gated on finding an NDK. `build.rs` probes
`ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, and `ANDROID_NDK_LATEST_HOME`, then
falls back to `~/Library/Android/sdk/ndk/*` and `~/Android/Sdk/ndk/*`,
picking the newest installed version. If no NDK is found, it writes
zero-byte stubs and prints a `cargo:warning` — holo still builds, but
`blobs::for_abi` returns `None` for every ABI, so the Monitor panel won't
show CPU, memory, or GC marks at runtime. This keeps casual contributors
who only touch the TUI from needing an NDK install.

On CI, both `ci.yml` and `release.yml` install NDK r28 via
`nttld/setup-ndk@v1` before `cargo build`, so PR checks and release
artifacts always include a real agent. The link flags in `build.rs`
(`-fno-exceptions`, `-fno-rtti`, the `-Wl,-z,*` page-size and segment
flags) are load-bearing for 16KB-page support and ART's unwinder — read
the comment in `build.rs` before changing them.
