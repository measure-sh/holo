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

1. **The agent** — a ~50 KB native library (`agent/agent.cpp`, prebuilt under
   `agent/prebuilt/{arm64-v8a,x86_64}/libholoagent.so`) that runs _inside_
   the app process. It registers JVMTI callbacks for GC start and finish,
   timestamps each pair, and puts a frame in a small ring buffer. A
   background pthread serves frames over an abstract Unix socket.

2. **The transport** — an abstract Unix socket named `@holoagent-<pid>`
   bound by the agent. Holo bridges it to a host loopback port using
   `adb forward tcp:0 localabstract:holoagent-<pid>`. The host then opens a
   regular TCP connection to that port.

3. **The host reader** — `src/vitals/reader.rs` reads length-prefixed
   binary frames off the TCP socket and turns them into `VitalsEvent::Gc`
   values, which `data.rs` forwards into the Monitor state for rendering.
