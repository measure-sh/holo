# Outer App Frame

## Chrome

- Full-screen alternate buffer with ratatui
- Rounded border on all sides, styled in `theme::SURFACE`
- Top-left: device label, bold + accent color
- Top-center: live clock (`HH:MM:SS`), updates every second
- Top-right: device battery level bar
- Bottom: `quit` hint — hotkey letter in `theme::KEY_HINT`, label in `theme::MUTED`

## Panel Layout (fixed-position, btop-style)

- 7 panels in fixed spatial positions; keys `1`–`7` toggle visibility
- Panel titles: superscript number (bold, border color) + name (first letter in `KEY_HINT`, rest in `FG`)
- Focused panel: brighter border color; unfocused: dimmed variant
- `i`/`l`/`c` focus the respective focusable panel
- At least one panel must remain visible
- Hidden panels cause neighbors to expand into vacated space

## Layout Tree

- **Top row (50%)**: Panel 1 (40%) | Panel 2 (60%)
- **Bottom section (50%)**: Left col | Mid col | Right col (equal width)
  - Left col: Panel 3 (50%) | Panel 4 (50%)
  - Mid col: Panel 5 (50%) | Panel 6 (50%)
  - Right col: Panel 7
- Collapse rules: if one side is hidden, the other takes 100%

## Hint Bar Style

- btop-style: `───` border segments between items, single-space padding around text
- Hotkey first letters in `theme::KEY_HINT` (red), labels in `theme::MUTED`
