# Sessions

Every attach is captured to disk and can be replayed in the same panels
used live. `src/session.rs` owns the writer; `src/replay.rs` owns the
reader.

## Lifecycle

One PID = one session. Opens when the process poller resolves a PID,
closes when that PID disappears. Reattach after a crash = new session.

PID-loss is debounced 5 s so a USB jiggle or adb hiccup doesn't fragment
one run into two session directories.

## On-disk layout

```
$XDG_CACHE_HOME/holo/sessions/         (~/Library/Caches/holo/sessions on macOS)
  <package>/
    <YYYYMMDD_HHMMSS>_pid<pid>_h<host_pid>/
      metadata.json     -- SessionMeta
      logcat.log        -- raw logcat
      vitals.bin        -- agent frames (see docs/agent.md)
      monitor_disk.jsonl
      issues.jsonl      -- de-duped crashes + ANRs
      traces/ screenshots/ db/ files/ dumps/
    _unsessioned/       -- pre-attach captures; leading `_` hides it
                           from the history dialog
```

Sessions roll up per-package across devices — same app on emulator vs
physical device is one row. Device serial lives in `metadata.json`, not
the path. Host PID in the dir name disambiguates two Holo instances on
the same package.

## Format choices

- **logcat.log** — raw lines, grep-friendly. Wrapping in JSON would just
  waste space.
- **vitals.bin** — same wire format the agent emits, re-encoded via
  `vitals::encode_event` so replay reuses the live decoder.
- **\*.jsonl** — one record per line so the writer can append without
  rewriting.
- **Network entries** are not stored — replay re-parses them from
  `logcat.log` via `network::parse_http_data`, the same function the
  live path uses.

## Schema versioning

Bump `SessionMeta::schema_version` whenever an older Holo would mis-decode
the new format. The replay loader rejects mismatched versions outright —
no migrations.

## Retention

Background thread on startup deletes sessions older than 5 days
(best-effort; failures are silent). No per-app cap — the sessions tree
is yours to prune.

A session directory is self-contained, so `tar czf` it to share.
