# Changelog

## 0.3.0

- Linux arm64 (`aarch64-unknown-linux-gnu`) prebuilt binary added to releases,
  thanks to [@budius](https://github.com/budius) for the nudge in #1.
  The `install.sh` script picks it up automatically on arm64 Linux.
- Windows x86_64 (`x86_64-pc-windows-msvc`) prebuilt binary added to releases,
  packaged as a `.zip`. Download from the Releases page and place `holo.exe`
  on your `PATH`.
- CI now cross-builds every release target on each PR, catching breakage
  before tagging.

## 0.2.2

Fix a panic when wrapping lines that contain multi-byte UTF-8 characters
(e.g. curly quotes in logcat output).

## 0.2.1

Re-release of 0.2.0 to attach prebuilt binaries that were missing from the
0.2.0 GitHub release. No code changes.

## 0.2.0

### New panels

- **Database panel** — browse SQLite databases on the device, open tables
  side-by-side with a detail viewer, navigate the tree with arrow keys, and
  run queries in the built-in REPL.
- **Network panel** — for apps integrated with the Measure SDK, every
  OkHttp request is tracked live: URL, method, status, timing, request
  and response headers and bodies, and failure reason. Filter with
  search, open requests in a split detail view, and read color-coded
  status codes at a glance.
- **Monitor panel** — unified CPU, memory, disk, and network traffic view
  with inline sparklines, per-metric color ramps, and session-total
  traffic counters. For apps integrated with the Measure SDK, the memory
  row switches from RSS to the more accurate total PSS reported by the
  SDK.
- **Issues panel** — crashes and ANRs now live together in a single panel,
  and downloads are saved under the app's package folder.
- **Files panel** — split master/detail layout for browsing device files
  with `stat` and `cat` previews; press `o` to open a file in your editor.

### Updated Keybindings

- `Ctrl+Q` quits (replaces the `qq` chord).
- `Ctrl+,` opens settings (replaces `ss`).
- `Ctrl+Z` toggles zoom (replaces bare `z`).
- Command shortcuts in the commands panel now use the same accent color as
  the top-bar hints, so `^q` and `^o` read consistently.

### Logcat improvements

- Measure SDK logs are filtered out by default.
- Wider tag column (15 chars) with middle-truncation for long tags.
- Lines always wrap to make parsing easier.
