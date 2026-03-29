# Holo

A terminal UI for Android. For developers who don't like leaving the terminal.
Manage app data, browse logs, record traces, run database queries, and control your device
settings directly from the TUI with simple commands.

![holo screenshot](screenshots/hero.png)

## Features

### A beautiful logcat
Filter by tag, search text, or log level. Scroll back through history or tail live. No more piping `adb logcat` through grep.

### Query your SQLite databases
Open any app database and run SQL queries right in the TUI. Browse results, scroll through history, or pull the whole db to your machine.

### See memory and CPU stats
Live CPU, memory, and disk usage with sparkline graphs. Spot leaks and runaway allocations at a glance.

### Record Perfetto traces with one keystroke
Start and stop system traces from the TUI. Holo spins up a local server so you can open them in ui.perfetto.dev with one keystroke.

### Browse files, pull what you need
Navigate your app's data directory as a tree. Pull any file to your machine with a single key.

### Crashes and ANRs, front and center
See crash stack traces and ANR reasons as they happen.

### Lots more
- **Permission management** — grant and revoke runtime permissions
- **Quick commands** — open, kill, uninstall, screenshot, toggle dark mode, layout bounds, show taps, and more
- **Device mirroring** via scrcpy
- **Wireless ADB** setup
- **Keyboard-driven** with vim-style navigation

## Setup

### Prerequisites

- [adb](https://developer.android.com/tools/adb) must be in your `PATH`
- [scrcpy](https://github.com/Genymobile/scrcpy) (optional, required for device mirroring)

### Install

**macOS (Homebrew)**

```sh
brew install holo
```

**Linux (Homebrew)**

```sh
brew install holo
```

**From source (any platform with Rust)**

```sh
cargo install holo
```

## Usage

```sh
$ holo
```

## Themes

Holo ships with 4 built-in themes. Switch themes from the settings menu.

### Dark
![Dark theme](screenshots/theme_default.png)

### Light
![Light theme](screenshots/theme_light.png)

### Tokyo Night
![Tokyo Night theme](screenshots/theme_tokyo_night.png)

### Akaito
![Akaito theme](screenshots/theme_akaito.png)
