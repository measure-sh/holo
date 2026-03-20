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
- Shows "Loading…" in `theme::MUTED` until the first poll completes

## Title Bar

- Panel number (superscript) + name with first letter as hotkey
- When focused: `───` separator then filter hint (`f` to activate)
- When filtering: shows typed filter text with cursor block

## Bottom Hints (focused only)

- `open`, `kill`, `erase` separated by `───` border segments
- When a filter is active: additional `Esc clear filter` hint
- Hotkey first letters use `theme::KEY_HINT` (red), rest in `theme::MUTED`

## Filter

- Press `f` to enter filter mode; typed text fuzzy-matches package names
- `Esc` exits filter mode; if filter text exists, `Esc` clears it
- Filter text shown inline in the title bar

## Interactions

- `o` — open (launch) the selected app
- `k` — kill (force-stop) the selected app
- `e` — erase (uninstall) the selected app
- Arrow keys / `j`/`k` — navigate the list
