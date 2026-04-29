# Agent

Holo's Monitor panel (CPU%, threads, RSS / Java heap / native heap, GC
marks, per-app rx/tx) is fed by a tiny C++ agent loaded into the target
app on the device. Most signals come from `/proc/self/*` and JNI calls
into `Runtime` / `Debug` / `TrafficStats`; GC pauses come from JVMTI
callbacks. The agent unifies all of them into one binary stream.

## Supported configs

- ABIs: `arm64-v8a` and `x86_64`. Other ABIs silently skip vitals.
- Debuggable apps only — release builds can't be injected.

## Architecture

End-to-end data flow:

```
agent (inside app)  ──►  abstract Unix socket @holoagent-<pid>
                    ──►  adb forward → host loopback TCP
                    ──►  src/vitals/reader.rs  ──►  data.rs  ──►  Monitor UI
```

1. **Agent** (`agent/agent.cpp`) — a small C++ library Holo loads into
   the target app via `cmd activity attach-agent`. Once loaded, a 1 Hz
   timer samples CPU / memory / threads / network counters, and JVMTI
   GC callbacks fire on every garbage collection. Both sources push
   into a small in-memory queue; a background thread sends the queue
   out to whoever's connected. The queue keeps the sampler from
   stalling if the host reads slowly.

2. **Transport** — the agent listens on a socket inside the app
   (`@holoagent-<pid>`, in-memory so it leaves no files in the app's
   data dir). `adb forward` exposes that socket as a regular TCP port
   on your machine.

3. **Host reader** (`src/vitals/reader.rs`) — connects to that port,
   parses the bytes the agent sent into events, and hands them to
   `data.rs`, which feeds the Monitor panel.

Every sample is timestamped on the agent side using `CLOCK_MONOTONIC`,
so GC marks line up with the CPU / memory / network curves.

## Wire format

Big-endian: `[u8 kind][u32 payload_len][payload]`.

| Kind   | Name          | Payload                                                         |
| ------ | ------------- | --------------------------------------------------------------- |
| `0x01` | GC pause      | `[i64 ts_ns][u32 duration_us]`                                  |
| `0x02` | MemorySample  | `[i64 ts_ns][u32 rss_kb][u32 java_heap_kb][u32 native_heap_kb]` |
| `0x03` | CpuSample     | `[i64 ts_ns][u32 cpu_centi_percent][u32 num_threads]`           |
| `0x04` | NetworkSample | `[i64 ts_ns][u64 rx_bytes][u64 tx_bytes]`                       |

Agent and host ship together (`build.rs`), so there is no version
negotiation. Session capture re-encodes the same frames into `vitals.bin`
— bump `SessionMeta::schema_version` if you change the format. See
[sessions.md](sessions.md).

## Build

The agent is an Android `.so` but holo is a host binary, so `build.rs`
has to produce the `.so` at `cargo build` time and embed it via
`include_bytes!` (`src/vitals/blobs.rs`). It tries three sources, in
order:

1. **`HOLOAGENT_PREBUILT_DIR=<dir>`** — copy a pre-built
   `libholoagent-<abi>.so` from the dir. No NDK needed.
2. **NDK on the host** — probes `ANDROID_NDK_{HOME,ROOT,LATEST_HOME}`,
   then `~/Library/Android/sdk/ndk/*` / `~/Android/Sdk/ndk/*`, then
   cross-compiles `agent/agent.cpp`.
3. **Nothing** — writes empty stubs + a `cargo:warning`. Holo still
   builds; vitals are silently disabled.

## CI

The host build matrix has 5 targets (mac arm64/x86, linux arm64/x86,
windows). The Android `.so` is identical across all of them, so the NDK
only needs to run once:

- One `agent-build` job on `ubuntu-latest` installs NDK r28, builds the
  agent, uploads it as `agent-blobs/`.
- Every other job (`test` + the per-target `build` matrix in `ci.yml`
  and `release.yml`) downloads `agent-blobs/` and sets
  `HOLOAGENT_PREBUILT_DIR=$GITHUB_WORKSPACE/agent-blobs` — so `build.rs`
  takes path 1 above and no other runner needs an NDK.

## Link flags

The flags in `build.rs` (`-fno-exceptions`, `-fno-rtti`, the `-Wl,-z,*`
page-size + segment + lazy flags) are load-bearing for 16KB-page support
and ART's loader/unwinder. Read the comment in `build.rs` before
changing them.
