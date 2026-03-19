## Device Selection

- Shown when multiple devices are connected
- Renders directly in the terminal (not full-screen)
- Header: "Select Device" in accent color
- Each device shown as a row; selected row highlighted with `▶` prefix, accent background, bold
- Footer hint: `j/k to move, Enter to select, q to quit` in muted color
- Navigation: `j`/`Down` move down, `k`/`Up` move up, `Enter` confirms, `q`/`Esc` exits
- Device label format priority: `Model (Device)` > `Model` > `Device` > `Serial`
- In the selector list, items display as `serial: label` when label differs from serial
