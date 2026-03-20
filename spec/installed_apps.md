# Installed Apps Panel

## Overview

- Panel 1 displays third-party packages installed on the connected device
- Sorted alphabetically for stable display

## Data Source

- ADB command: `adb -s <serial> shell pm list packages -3`
- Output format: `package:com.example.app` per line
- `package:` prefix is stripped during parsing; lines without the prefix are skipped
- Polled every 60 seconds in a background thread

## Layout

- Renders as a ratatui `List` widget inside the panel border
- Each package name is a single `ListItem`
- Text color: `theme::FG`

## Loading State

- Shows "Loading…" in `theme::MUTED` until the first poll completes

## Interactions

- None yet

## Open Questions

- Scrolling for long package lists
- Per-package details (version, size) via `dumpsys package`
- Search/filter within the list
