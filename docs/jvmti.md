# JVMTI Agent (GC marks)

Holo shows GC pauses as diamond marks on the Monitor panel. The data comes
from a tiny C++ agent (`libholoagent.so`) that Holo loads into the target app
on the device. This doc explains, in plain terms, how that works end to end.

## Supported configurations

- **ABIs:** `arm64-v8a` and `x86_64` only. On any other ABI (e.g.
  `armeabi-v7a`, `x86`) Holo quietly skips vitals — no GC marks will appear.
- **Debuggable apps only:** the target must ship with
  `android:debuggable="true"`. Release builds are skipped by design.

## What you see in the UI

When you focus an app, Holo checks if it's debuggable. If it is, every time
ART runs a garbage collection inside the app, a `◆` mark shows up on the
Monitor chart at the moment the GC finished. The footer line under the chart
says something like `◆ 7 GC` — the number of GCs in the visible window.

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
   │  TCP read    │ ◀───── binary frames ── │  GC start/finish     │
   │  loop        │                         │  enqueued + sent     │
   └──────────────┘                         └──────────────────────┘
```

There are three things at play:

1. **The agent** — a ~50 KB native library (`agent/agent.cpp`, cross-compiled
   into the holo binary at build time — see below) that runs _inside_ the
   app process. It registers JVMTI callbacks for GC start and finish,
   timestamps each pair, and puts a frame in a small ring buffer. A
   background pthread serves frames over an abstract Unix socket.

2. **The transport** — an abstract Unix socket named `@holoagent-<pid>`
   bound by the agent. Holo bridges it to a host loopback port using
   `adb forward tcp:0 localabstract:holoagent-<pid>`. The host then opens a
   regular TCP connection to that port.

3. **The host reader** — `src/vitals/reader.rs` reads length-prefixed
   binary frames off the TCP socket and turns them into `VitalsEvent::Gc`
   values, which `data.rs` forwards into the Monitor state for rendering.

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
`blobs::for_abi` returns `None` for every ABI, so GC marks won't appear at
runtime. This keeps casual contributors who only touch the TUI from needing
an NDK install.

On CI, both `ci.yml` and `release.yml` install NDK r28 via
`nttld/setup-ndk@v1` before `cargo build`, so PR checks and release
artifacts always include a real agent. The link flags in `build.rs`
(`-fno-exceptions`, `-fno-rtti`, the `-Wl,-z,*` page-size and segment
flags) are load-bearing for 16KB-page support and ART's unwinder — read
the comment in `build.rs` before changing them.
