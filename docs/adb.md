# ADB Commands Reference

All ADB commands used by holo, organized by feature area.

## Polling Summary

| Interval  | Feature       | Command                                            |
| --------- | ------------- | -------------------------------------------------- |
| Streaming | Logcat        | `adb logcat --pid=<pid>`                           |
| 1s        | Process PID   | `adb shell pidof -s <package>`                     |
| 1s        | CPU           | `adb shell top -b -n 1 -q`                         |
| 1s        | Memory        | `adb shell cat /proc/<pid>/status`                 |
| 5s        | Disk          | `adb shell run-as <package> du -s . ./cache`       |
| 5s        | Connectivity  | `adb get-state`                                    |
| 2s        | Network bytes | `adb shell dumpsys netstats detail`                |
| 5s        | Permissions   | `adb shell dumpsys package <package>`              |
| 5s        | Crashes       | `adb shell dumpsys dropbox --print data_app_crash` |
| 5s        | ANRs          | `adb shell dumpsys dropbox --print data_app_anr`   |
| 30s       | Battery       | `adb shell dumpsys battery`                        |

## Commands by Feature

### Device Management

| Command                     | Purpose                                         |
| --------------------------- | ----------------------------------------------- |
| `adb devices -l`            | List connected devices with details             |
| `adb -s <serial> get-state` | Check device connection state (polled every 5s) |

### App Management

| Command                                                               | Purpose                                         |
| --------------------------------------------------------------------- | ----------------------------------------------- |
| `adb shell pm list packages -3`                                       | List third-party installed apps                 |
| `adb shell monkey -p <package> -c android.intent.category.LAUNCHER 1` | Launch app                                      |
| `adb shell am force-stop <package>`                                   | Force stop app                                  |
| `adb shell pm clear <package>`                                        | Clear app data and cache                        |
| `adb uninstall <package>`                                             | Uninstall app                                   |
| `adb shell dumpsys package <package>`                                 | Get app version info (versionName, versionCode) |
| `adb shell run-as <package> id`                                       | Check if app is debuggable                      |

### Battery

| Command                     | Purpose                               |
| --------------------------- | ------------------------------------- |
| `adb shell dumpsys battery` | Read battery level (polled every 30s) |

### Logcat

| Command                        | Purpose                                         |
| ------------------------------ | ----------------------------------------------- |
| `adb shell pidof -s <package>` | Get app PID for log filtering (polled every 1s) |
| `adb logcat --pid=<pid>`       | Stream logs filtered by PID                     |

### Monitor (CPU, Memory, Disk)

| Command                                      | Purpose                                           |
| -------------------------------------------- | ------------------------------------------------- |
| `adb shell top -b -n 1 -q`                   | CPU usage snapshot (polled every 1s)              |
| `adb shell cat /proc/<pid>/status`           | RSS memory from proc filesystem (polled every 1s) |
| `adb shell run-as <package> du -s . ./cache` | App data and cache size on disk (polled every 5s) |

### Network (no Measure SDK)

| Command                               | Purpose                                            |
| ------------------------------------- | -------------------------------------------------- |
| `adb shell dumpsys package <package>` | Resolve app UID via `userId=` line                 |
| `adb shell dumpsys netstats detail`   | Per-UID rxBytes/txBytes counters (polled every 2s) |

### Permissions

| Command                                      | Purpose                                                |
| -------------------------------------------- | ------------------------------------------------------ |
| `adb shell dumpsys package <package>`        | List declared vs granted permissions (polled every 5s) |
| `adb shell pm grant <package> <permission>`  | Grant a runtime permission                             |
| `adb shell pm revoke <package> <permission>` | Revoke a runtime permission                            |

### Crashes & ANRs

| Command                                            | Purpose                                    |
| -------------------------------------------------- | ------------------------------------------ |
| `adb shell dumpsys dropbox --print data_app_crash` | Get recent crash reports (polled every 5s) |
| `adb shell dumpsys dropbox --print data_app_anr`   | Get recent ANR reports (polled every 5s)   |

### Files

| Command                                    | Purpose                             |
| ------------------------------------------ | ----------------------------------- |
| `adb shell run-as <package> ls -p <path>`  | List files in app's private storage |
| `adb exec-out run-as <package> cat <path>` | Pull a file from app storage        |

### Database

| Command                                                     | Purpose                              |
| ----------------------------------------------------------- | ------------------------------------ |
| `adb shell run-as <package> ls databases/`                  | List app databases                   |
| `adb shell run-as <package> sqlite3 databases/<db> '<sql>'` | Execute SQL query on app database    |
| `adb shell run-as <package> ls databases/<db>`              | Check if db file exists before pull  |
| `adb exec-out run-as <package> cat databases/<db>`          | Pull database file (also -wal, -shm) |

### Vitals (JVMTI agent)

Used to attach the embedded `libholoagent.so` to a debuggable app for GC pause
events. The agent is pushed once per app, attached at runtime, and streams
binary frames over an abstract Unix socket bridged to host loopback.

| Command                                                                                                                  | Purpose                                                    |
| ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| `adb shell getprop ro.product.cpu.abi`                                                                                   | Pick the matching prebuilt agent (`arm64-v8a` or `x86_64`) |
| `adb push <local> /data/local/tmp/libholoagent.so`                                                                       | Stage the agent .so to a world-readable location           |
| `adb shell run-as <package> sh -c 'cp /data/local/tmp/libholoagent.so ./libholoagent.so && chmod 700 ./libholoagent.so'` | Copy the agent into the app's private data dir             |
| `adb shell cmd activity attach-agent <package> <so_path>`                                                                | Ask the JVM to load the agent into the running app         |
| `adb forward tcp:0 localabstract:holoagent-<pid>`                                                                        | Bridge the agent's abstract socket to a host loopback port |
| `adb forward --remove tcp:<port>`                                                                                        | Tear down the forward when the pid changes or Holo exits   |

### Trace (Perfetto)

| Command                                      | Purpose                                      |
| -------------------------------------------- | -------------------------------------------- |
| `adb shell perfetto -d --txt -c - -o <path>` | Start system tracing with config via stdin   |
| `adb shell pkill -INT perfetto`              | Stop tracing (waits 2s for file to finalize) |
| `adb -s <serial> pull <trace-path> <dest>`   | Pull trace file to host                      |

### Screenshot

| Command                                                                                                | Purpose                                       |
| ------------------------------------------------------------------------------------------------------ | --------------------------------------------- |
| `adb shell settings put global sysui_demo_allowed 1`                                                   | Enable system UI demo mode                    |
| `adb shell am broadcast -a com.android.systemui.demo -e command enter`                                 | Enter demo mode                               |
| `adb shell am broadcast -a com.android.systemui.demo -e command clock -e hhmm 1000`                    | Set clock to 10:00                            |
| `adb shell am broadcast -a com.android.systemui.demo -e command battery -e level 100 -e plugged false` | Set battery to 100% unplugged                 |
| `adb shell am broadcast -a com.android.systemui.demo -e command network -e wifi show -e level 4`       | Set full WiFi signal                          |
| `adb shell am broadcast -a com.android.systemui.demo -e command notifications -e visible false`        | Hide notifications                            |
| `adb exec-out screencap -p`                                                                            | Capture screenshot as PNG (after 500ms delay) |
| `adb shell am broadcast -a com.android.systemui.demo -e command exit`                                  | Exit demo mode                                |

### System Settings

| Command                                                   | Purpose                         |
| --------------------------------------------------------- | ------------------------------- |
| `adb shell getprop debug.layout`                          | Get layout bounds state         |
| `adb shell setprop debug.layout true/false`               | Toggle layout bounds            |
| `adb shell service call activity 1599295570`              | Refresh system UI after setprop |
| `adb shell settings get global airplane_mode_on`          | Get airplane mode state         |
| `adb shell cmd connectivity airplane-mode enable/disable` | Toggle airplane mode            |
| `adb shell settings get global wifi_on`                   | Get WiFi state                  |
| `adb shell svc wifi enable/disable`                       | Toggle WiFi                     |
| `adb shell settings get secure ui_night_mode`             | Get dark mode state             |
| `adb shell cmd uimode night yes/no`                       | Toggle dark mode                |
| `adb shell input keyevent KEYCODE_WAKEUP`                 | Wake device screen              |
| `adb shell settings get system show_touches`              | Get show taps state             |
| `adb shell settings put system show_touches 1/0`          | Toggle show taps overlay        |
| `adb shell settings get system pointer_location`          | Get pointer location state      |
| `adb shell settings put system pointer_location 1/0`      | Toggle pointer location overlay |
| `adb shell getprop debug.hwui.profile`                    | Get GPU rendering bars state    |
| `adb shell setprop debug.hwui.profile visual_bars/false`  | Toggle GPU rendering bars       |

### Accessibility

| Command                                                                | Purpose                      |
| ---------------------------------------------------------------------- | ---------------------------- |
| `adb shell settings get secure enabled_accessibility_services`         | Check if TalkBack is enabled |
| `adb shell settings put secure enabled_accessibility_services <value>` | Enable/disable TalkBack      |

### Wireless ADB

| Command                 | Purpose                                   |
| ----------------------- | ----------------------------------------- |
| `adb tcpip 5555`        | Enable wireless ADB on port 5555          |
| `adb shell ip route`    | Detect device IP address (after 1s delay) |
| `adb connect <ip>:5555` | Connect to device over WiFi               |

### Emulator

| Command                | Purpose                                |
| ---------------------- | -------------------------------------- |
| `emulator -list-avds`  | List available Android Virtual Devices |
| `emulator -avd <name>` | Launch emulator in background          |
| `adb emu avd name`     | Get AVD name for a running emulator    |
