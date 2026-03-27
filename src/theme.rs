use std::sync::atomic::{AtomicU8, Ordering};
use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub popup_bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub magenta: Color,
    pub cyan: Color,
}

const DARK: Theme = Theme {
    bg:       Color::Rgb(0x00, 0x00, 0x00),
    fg:       Color::Rgb(0xa9, 0xb1, 0xd6),
    accent:   Color::Rgb(0x7a, 0xa2, 0xf7),
    red:      Color::Rgb(0xf7, 0x76, 0x8e),
    green:    Color::Rgb(0x9e, 0xce, 0x6a),
    yellow:   Color::Rgb(0xe0, 0xaf, 0x68),
    magenta:  Color::Rgb(0xbb, 0x9a, 0xf7),
    cyan:     Color::Rgb(0x0d, 0xb9, 0xd7),
    surface:  Color::Rgb(0x32, 0x34, 0x4a),
    popup_bg: Color::Rgb(0x1a, 0x1b, 0x26),
    muted:    Color::Rgb(0x56, 0x5f, 0x89),
};

const THEMES: [&Theme; 1] = [&DARK];
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> &'static Theme {
    let idx = CURRENT.load(Ordering::Relaxed) as usize;
    THEMES[idx.min(THEMES.len() - 1)]
}

pub fn set_theme(id: u8) {
    CURRENT.store(id, Ordering::Relaxed);
}

pub fn level_color(level: char) -> Color {
    let t = current();
    match level {
        'V' => t.muted,
        'D' => t.magenta,
        'I' => t.cyan,
        'W' => t.yellow,
        'E' | 'F' => t.red,
        _ => t.muted,
    }
}

pub fn level_label(level: char) -> &'static str {
    match level {
        'V' => "V",
        'D' => "D",
        'I' => "I",
        'W' => "W",
        'E' => "E",
        'F' => "F",
        _ => "?",
    }
}

pub fn level_name(level: char) -> &'static str {
    match level {
        'V' => "Verbose",
        'D' => "Debug",
        'I' => "Info",
        'W' => "Warn",
        'E' => "Error",
        'F' => "Fatal",
        _ => "Unknown",
    }
}
