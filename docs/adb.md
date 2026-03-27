# ADB Commands Reference

All ADB commands used by msh, organized by feature area.

## Polling Summary

| Interval | Feature | Command | Source |
|----------|---------|---------|--------|
| Streaming | Logcat | `adb logcat --pid=<pid>` | `logcat.rs:104` |
| 1s | Process PID | `adb shell pidof -s <package>` | `processes.rs:11` |
| 1s | CPU | `adb shell top -b -n 1 -q` | `monitor.rs:139` |
| 1s | Memory | `adb shell 'PID=$(pidof -s <pkg>); [ -n "$PID" ] && cat /proc/$PID/status'` | `monitor.rs:137` |
| 1s | Frames | `adb shell dumpsys gfxinfo <package>` | `monitor.rs:141` |
| 5s | Disk | `adb shell run-as <package> du -s . ./cache` | `monitor.rs:134` |
| 5s | Permissions | `adb shell dumpsys package <package>` | `permissions.rs:74` |
| 30s | Battery | `adb shell dumpsys battery` | `battery.rs:41` |

## Commands by Feature

### Device Management

| Command | Purpose |
|---------|---------|
| `adb devices -l` | List connected devices with details |
| `adb -s <serial> get-state` | Get device connection state |

### App Management

| Command | Purpose |
|---------|---------|
| `adb shell pm list packages -3` | List third-party installed apps |
| `adb shell monkey -p <package> -c android.intent.category.LAUNCHER 1` | Launch app |
| `adb shell am force-stop <package>` | Force stop app |
| `adb shell pm clear <package>` | Clear app data and cache |
| `adb uninstall <package>` | Uninstall app |
| `adb shell dumpsys package <package>` | Get app version info |

### Battery

| Command | Purpose |
|---------|---------|
| `adb shell dumpsys battery` | Read battery level, status, temperature |

### Logcat

| Command | Purpose |
|---------|---------|
| `adb shell pidof -s <package>` | Get app PID for log filtering |
| `adb logcat --pid=<pid>` | Stream logs filtered by PID |

### Monitor (CPU, Memory, Frames, Disk)

| Command | Purpose |
|---------|---------|
| `adb shell top -b -n 1 -q` | CPU usage snapshot |
| `adb shell 'PID=$(pidof -s <pkg>); [ -n "$PID" ] && cat /proc/$PID/status'` | RSS memory from proc filesystem |
| `adb shell dumpsys gfxinfo <package>` | Frame render times, slow/frozen frame counts |
| `adb shell run-as <package> du -s . ./cache` | App data and cache size on disk |

### Permissions

| Command | Purpose |
|---------|---------|
| `adb shell dumpsys package <package>` | List declared vs granted permissions |
| `adb shell pm grant <package> <permission>` | Grant a runtime permission |
| `adb shell pm revoke <package> <permission>` | Revoke a runtime permission |

### Files

| Command | Purpose |
|---------|---------|
| `adb shell run-as <package> ls -p <path>` | List files in app's private storage |
| `adb exec-out run-as <package> cat <path>` | Download/pull a file from app storage |

### Database

| Command | Purpose |
|---------|---------|
| `adb shell run-as <package> ls databases/` | List app databases |
| `adb shell run-as <package> sqlite3 databases/<db> '<sql>'` | Execute SQL query on app database |
| `adb shell run-as <package> ls databases/<db>` | Check if db file exists before pull |
| `adb exec-out run-as <package> cat databases/<db>` | Download database file (also -wal, -shm) |

### Trace (Perfetto)

| Command | Purpose |
|---------|---------|
| `adb shell perfetto -d --txt -c - -o <path>` | Start system tracing with config via stdin |
| `adb shell pkill -INT perfetto` | Stop tracing (waits 2s before pull) |
| `adb pull <trace-path> <dest>` | Download trace file to host |

### Screenshot

| Command | Purpose |
|---------|---------|
| `adb shell settings put global sysui_demo_allowed 1` | Enable system UI demo mode |
| `adb shell am broadcast -a com.android.systemui.demo -e command enter` | Enter demo mode |
| `adb shell am broadcast -a com.android.systemui.demo -e command clock -e hhmm 1000` | Set clock to 10:00 |
| `adb shell am broadcast -a com.android.systemui.demo -e command battery -e level 100 -e plugged false` | Set battery to 100% unplugged |
| `adb shell am broadcast -a com.android.systemui.demo -e command network -e wifi show -e level 4` | Set full WiFi signal |
| `adb shell am broadcast -a com.android.systemui.demo -e command notifications -e visible false` | Hide notifications |
| `adb exec-out screencap -p` | Capture screenshot as PNG (after 500ms delay) |
| `adb shell am broadcast -a com.android.systemui.demo -e command exit` | Exit demo mode |

### System Settings

| Command | Purpose |
|---------|---------|
| `adb shell getprop debug.layout` | Get layout bounds toggle state |
| `adb shell setprop debug.layout true/false` | Toggle layout bounds |
| `adb shell service call activity 1599295570` | Refresh system UI after setprop |
| `adb shell settings get global airplane_mode_on` | Get airplane mode state |
| `adb shell cmd connectivity airplane-mode enable/disable` | Toggle airplane mode |
| `adb shell settings get global wifi_on` | Get WiFi state |
| `adb shell svc wifi enable/disable` | Toggle WiFi |
| `adb shell settings get secure ui_night_mode` | Get dark mode state |
| `adb shell cmd uimode night yes/no` | Toggle dark mode |
| `adb shell input keyevent KEYCODE_WAKEUP` | Wake device screen |
| `adb shell settings get system show_touches` | Get show taps state |
| `adb shell settings put system show_touches 1/0` | Toggle show taps overlay |
| `adb shell settings get system pointer_location` | Get pointer location state |
| `adb shell settings put system pointer_location 1/0` | Toggle pointer location overlay |
| `adb shell getprop debug.hwui.profile` | Get GPU rendering bars state |
| `adb shell setprop debug.hwui.profile visual_bars/false` | Toggle GPU rendering bars overlay |

### Wireless ADB

| Command | Purpose |
|---------|---------|
| `adb tcpip 5555` | Enable wireless ADB on port 5555 |
| `adb shell ip route` | Detect device IP address (after 1s delay) |
| `adb connect <ip>:5555` | Connect to device over WiFi |

### Emulator

| Command | Purpose |
|---------|---------|
| `emulator -list-avds` | List available Android Virtual Devices |
| `emulator -avd <name>` | Launch emulator in background |
| `adb emu avd name` | Get AVD name for a running emulator |
