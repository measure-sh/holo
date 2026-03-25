use ratatui::style::Color;

// Omarchy Tokyo Night OLED palette
pub const BG: Color = Color::Rgb(0x00, 0x00, 0x00);
pub const FG: Color = Color::Rgb(0xa9, 0xb1, 0xd6);
pub const ACCENT: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);
pub const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const MAGENTA: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
pub const CYAN: Color = Color::Rgb(0x0d, 0xb9, 0xd7);
pub const SURFACE: Color = Color::Rgb(0x32, 0x34, 0x4a);
pub const POPUP_BG: Color = Color::Rgb(0x1a, 0x1b, 0x26);
pub const MUTED: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const KEY_HINT: Color = Color::Rgb(0xf7, 0x76, 0x8e);

// Dimmed panel colors — toned-down variants for borders/titles
pub const DIM_BLUE: Color = Color::Rgb(0x20, 0x2a, 0x3c);
pub const DIM_GREEN: Color = Color::Rgb(0x24, 0x32, 0x1a);
pub const DIM_YELLOW: Color = Color::Rgb(0x38, 0x2e, 0x1c);
pub const DIM_CYAN: Color = Color::Rgb(0x12, 0x2e, 0x36);
pub const DIM_MAGENTA: Color = Color::Rgb(0x2e, 0x26, 0x3c);
pub const DIM_RED: Color = Color::Rgb(0x3c, 0x20, 0x26);
pub const ORANGE: Color = Color::Rgb(0xff, 0x9e, 0x64);
pub const DIM_ORANGE: Color = Color::Rgb(0x3c, 0x2a, 0x1a);
pub const DIM_TEAL: Color = Color::Rgb(0x1a, 0x32, 0x2e);

pub fn level_color(level: char) -> Color {
    match level {
        'V' => MUTED,
        'D' => MAGENTA,
        'I' => CYAN,
        'W' => YELLOW,
        'E' => RED,
        'F' => RED,
        _ => MUTED,
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
