use std::sync::atomic::{AtomicU8, Ordering};
use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub surface: Color,
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
    muted:    Color::Rgb(0x56, 0x5f, 0x89),
};

const LIGHT: Theme = Theme {
    bg:       Color::Rgb(0xf3, 0xe4, 0xcb),
    fg:       Color::Rgb(0x4d, 0x2e, 0x1a),
    accent:   Color::Rgb(0xa3, 0x2f, 0x1a),
    red:      Color::Rgb(0xa4, 0x37, 0x3c),
    green:    Color::Rgb(0xa4, 0x6d, 0x2d),
    yellow:   Color::Rgb(0xa8, 0x61, 0x1f),
    magenta:  Color::Rgb(0x9c, 0x35, 0x21),
    cyan:     Color::Rgb(0x75, 0x58, 0x33),
    surface:  Color::Rgb(0xc8, 0xb4, 0x94),
    muted:    Color::Rgb(0x9f, 0x82, 0x53),
};

const THEMES: [&Theme; 2] = [&DARK, &LIGHT];
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> &'static Theme {
    let idx = CURRENT.load(Ordering::Relaxed) as usize;
    THEMES[idx.min(THEMES.len() - 1)]
}

pub fn toggle() {
    let idx = CURRENT.load(Ordering::Relaxed);
    let next = (idx + 1) % THEMES.len() as u8;
    CURRENT.store(next, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    CURRENT.load(Ordering::Relaxed) == 0
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
