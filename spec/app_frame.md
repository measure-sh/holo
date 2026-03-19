# Outer App Frame

- Full-screen alternate buffer with ratatui
- Border on all sides, styled in surface color
- Top-left title: device label, bold + accent color
- Top-center title: live clock (`HH:MM:SS`), updates every second
- Bottom title: `q/Esc to exit` hint in muted color
- Show device battery on the top right
- Event loop polls every 1 second; redraws on each tick
