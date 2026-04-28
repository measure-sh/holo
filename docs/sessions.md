# Sessions

Holo captures every attach to disk and lets you replay it later. Logcat,
monitor (CPU / memory / threads / network), HTTP traffic, crashes, ANRs,
and Perfetto traces all show up in the same panels you used live —
just rendered from files instead of from an attached device.

## When a session opens and closes

A session begins the moment Holo's process poller resolves a PID for the
selected app, and ends when that PID disappears (force-stop, crash,
uninstall, or you switching app). Reattaching after a crash starts a fresh
session — one PID = one session.

A brief disconnect (USB jiggle, adb hiccup) does **not** close the session.
The PID-becomes-`None` transition is debounced by 5 seconds; if the same
PID returns within that window the writer keeps appending. This keeps a
two-second cable wiggle from fragmenting one run into multiple
session directories in the panel list.

Switching app or device closes the current session and opens a new one for
the new attach.

## On-disk layout

```
$XDG_CACHE_HOME/holo/sessions/         (~/Library/Caches/holo/sessions on macOS)
  <package>/
    <YYYYMMDD_HHMMSS>_pid<pid>_h<host_pid>/
      metadata.json     -- SessionMeta
      logcat.log        -- raw logcat, one line per entry
      vitals.bin        -- JVMTI frames: [u8 kind][u32 len_be][payload]
      monitor_disk.jsonl-- 1 Hz disk usage samples
      issues.jsonl      -- de-duped crashes + ANRs
      traces/           -- Perfetto traces pulled during the session
      screenshots/      -- PNGs from Action::Screenshot
      db/               -- SQLite databases pulled from the app
      files/            -- files pulled from the app's data dir
      dumps/            -- editor-open text (issues / network detail)
    _unsessioned/       -- pre-attach captures (e.g. screenshots before
                           an app is selected). Sibling to session dirs;
                           the leading `_` keeps it out of the history
                           dialog (which filters by metadata.json).
```

Every artifact holo writes — captures from JVMTI, pulled databases,
pulled files, screenshots, perfetto traces, editor dumps — lives under
the active session directory. No other path is touched.

Sessions roll up per-package across every device the user has captured
against — the same app on an emulator vs a physical device is one app
in the dialog. The captured device serial lives in `metadata.json` if a
row ever needs to surface it.

The host PID disambiguates two Holo instances racing on the same
package. It also makes the directory name self-describing if you ever
share a session as a tarball.

`SessionMeta` (`src/session.rs`) carries everything Holo needs to
reconstruct the live state on replay — device + app identity, the app
version Holo probed at attach time, the `has_measure_sdk` flag that drives
the Network panel, and the eight initial toggle states (`apply_initial_state`
in dispatch). Replay restores those alongside the captured streams.

## Format choices

- **logcat.log** — raw lines. Already line-oriented; wrapping each line
  in JSON would just waste space and break `grep`.
- **vitals.bin** — the same `[u8 kind][u32 len_be][payload]` format the
  JVMTI agent emits, re-encoded on the host side via
  `vitals::encode_event`. Replay reuses the existing decoder. The wire
  format lives in `src/vitals/mod.rs` — one source of truth.
- **monitor_disk.jsonl / issues.jsonl** — small, structured, JSONL via
  `serde_json` (already a dependency). One record per line so the writer
  can append without rewriting the whole file.
- **Network entries are not stored separately.** They're parsed from
  logcat lines on replay using `network::parse_http_data`, the same
  function the live path uses. One source of truth.

## Schema versioning

`SessionMeta::schema_version` is bumped any time the on-disk format changes
in a way an older Holo would mis-decode (a new vitals kind, a different
issue dedup strategy, etc.). The replay loader rejects sessions whose
`schema_version` doesn't match the running build instead of trying to
silently recover. This is a hard cut: there is no migration today.

## Retention

On startup Holo spawns a background thread that walks the sessions root
and deletes any session whose `started_at` (or directory mtime, if
`metadata.json` is missing) is older than 5 days. The walk is best-effort
— failures are silent so a transient permission issue can't keep Holo
from booting.

There is no per-app or per-device cap today; if you run a chatty app
daily, traces inside captured sessions can dominate disk usage. The
sessions tree is yours to delete by hand or via the history dialog.

## Sharing a session

A session directory is self-contained. Tarball it:

```sh
cd ~/Library/Caches/holo/sessions/emulator-5554/com.example.app
tar czf my-bug.tar.gz 20260428_103000_pid12345_h99/
```

Drop the tarball into a teammate's sessions root with the same shape and
they can replay it. There's no cross-machine identity — the `device`
folder name is just the adb serial we captured under.
