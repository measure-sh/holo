# Multi-Panel Layout

## Layout

```
+----------------------+---------------------+
| 1. Installed Apps    | 2. Logcat           |
| 40%w, 50%h           | 60%w, 50%h          |
+----------+-----------+----------+----------+
| 3. Network           | 4. CPU   | 6. Disk  |
| 40%w, 50%h           | 30%w,25%h|   Usage  |
|                      +----------+ 30%w,50%h|
|                      | 5. Memory|          |
|                      | 30%w,25%h|          |
+----------------------+----------+----------+
```

## Panels

| # | Title          | Width | Height |
|---|----------------|-------|--------|
| 1 | Installed Apps | 40%   | 50%    |
| 2 | Logcat         | 60%   | 50%    |
| 3 | Network        | 40%   | 50%    |
| 4 | CPU            | 30%   | 25%    |
| 5 | Memory         | 30%   | 25%    |
| 6 | Disk Usage     | 30%   | 50%    |

## Selection

- Press `1`–`6` to select the corresponding panel.
- Pressing the same key again deselects (toggles).
- At most one panel is selected at a time.
- Selected panel border: `theme::ACCENT`
- Unselected panel border: `theme::SURFACE`

## Rendering

Nested ratatui `Layout` splits:
1. Vertical 50/50 → top_row, bottom_row
2. Top row: horizontal 40/60 → panel 1, panel 2
3. Bottom row: horizontal 40/30/30 → panel 3, mid_right, panel 6
4. mid_right: vertical 50/50 → panel 4, panel 5
