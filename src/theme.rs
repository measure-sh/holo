use std::sync::atomic::{AtomicU8, Ordering};
use ratatui::style::Color;

pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub overlay: Color,
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

const TOKYO_NIGHT: Theme = Theme {
    name:     "Tokyo Night",
    bg:       Color::Rgb(0x00, 0x00, 0x00),
    overlay:  Color::Rgb(0x16, 0x16, 0x22),
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

const AKAITO: Theme = Theme {
    name:     "Akaito",
    bg:       Color::Rgb(0xf3, 0xe4, 0xcb),
    overlay:  Color::Rgb(0xd8, 0xc8, 0xab),
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

const DARK: Theme = Theme {
    name:     "Dark",
    bg:       Color::Rgb(0x00, 0x00, 0x00),
    overlay:  Color::Rgb(0x12, 0x12, 0x12),
    fg:       Color::Rgb(0xff, 0xff, 0xff),
    accent:   Color::Rgb(0xff, 0xd2, 0x30),
    red:      Color::Rgb(0xff, 0x5b, 0x5b),
    green:    Color::Rgb(0x4a, 0xde, 0x80),
    yellow:   Color::Rgb(0xff, 0xae, 0x04),
    magenta:  Color::Rgb(0xc0, 0x84, 0xfc),
    cyan:     Color::Rgb(0x22, 0xd3, 0xee),
    surface:  Color::Rgb(0x24, 0x24, 0x24),
    muted:    Color::Rgb(0xa4, 0xa4, 0xa4),
};

const LIGHT: Theme = Theme {
    name:     "Light",
    bg:       Color::Rgb(0xfc, 0xfc, 0xfc),
    overlay:  Color::Rgb(0xf5, 0xf5, 0xf5),
    fg:       Color::Rgb(0x00, 0x00, 0x00),
    accent:   Color::Rgb(0xb8, 0x96, 0x0a),
    red:      Color::Rgb(0xe5, 0x4b, 0x4f),
    green:    Color::Rgb(0x16, 0xa3, 0x4a),
    yellow:   Color::Rgb(0xa1, 0x62, 0x07),
    magenta:  Color::Rgb(0x93, 0x33, 0xea),
    cyan:     Color::Rgb(0x08, 0x91, 0xb2),
    surface:  Color::Rgb(0xd4, 0xd4, 0xd4),
    muted:    Color::Rgb(0x52, 0x52, 0x52),
};

const THEMES: [&Theme; 4] = [&TOKYO_NIGHT, &AKAITO, &DARK, &LIGHT];
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> &'static Theme {
    let idx = CURRENT.load(Ordering::Relaxed) as usize;
    THEMES[idx.min(THEMES.len() - 1)]
}

pub fn set_theme(id: usize) {
    CURRENT.store(id.min(THEMES.len() - 1) as u8, Ordering::Relaxed);
}

pub fn current_index() -> usize {
    CURRENT.load(Ordering::Relaxed) as usize
}

pub fn theme_count() -> usize {
    THEMES.len()
}

pub fn theme_name(idx: usize) -> &'static str {
    THEMES[idx.min(THEMES.len() - 1)].name
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
