# Outer App Frame

- Full-screen alternate buffer with ratatui
- Border on all sides, styled in surface color
- Top-left title: device label, bold + accent color
- Top-center title: live clock (`HH:MM:SS`), updates every second
- Bottom title: key hints — keys (`1`-`7`, `q`, `Esc`) in bold accent, separators/labels in muted
- Show device battery on the top right
- Event loop polls every 1 second; redraws on each tick

## Panel Layout (fixed-position, btop-style)

- 7 panels in fixed spatial positions; keys `1`–`7` toggle visibility
- Panel titles: number in bold accent color, name in muted (btop-style)
- At least one panel must remain visible
- Hidden panels cause neighbors to expand into vacated space (no reflow)
- Layout tree:
  - **Top row (50%)**: Panel 1 (40%) | Panel 2 (60%)
  - **Bottom section (50%)**: Left col | Mid col | Right col (equal width)
    - Left col splits vertically: Panel 3 (50%) | Panel 4 (50%)
    - Mid col splits vertically: Panel 5 (50%) | Panel 6 (50%)
    - Right col: Panel 7
- Three-level resolution:
  - L1: top vs bottom — both visible → 50/50; one only → 100%
  - L2a: top row — both → 40/60; one only → 100%
  - L2b: bottom columns — equal width among visible columns
  - L3a: left column (3,4) — both → 50/50; one only → 100%
  - L3b: mid column (5,6) — both → 50/50; one only → 100%
