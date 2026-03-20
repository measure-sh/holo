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
pub const OVERLAY: Color = Color::Rgb(0x41, 0x48, 0x68);
pub const MUTED: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const KEY_HINT: Color = Color::Rgb(0xf7, 0x76, 0x8e);

// Dimmed panel colors — toned-down variants for borders/titles
pub const DIM_BLUE: Color = Color::Rgb(0x4a, 0x63, 0x8c);
pub const DIM_GREEN: Color = Color::Rgb(0x5a, 0x76, 0x3e);
pub const DIM_YELLOW: Color = Color::Rgb(0x80, 0x6a, 0x40);
pub const DIM_CYAN: Color = Color::Rgb(0x2a, 0x6a, 0x7a);
pub const DIM_MAGENTA: Color = Color::Rgb(0x6a, 0x5a, 0x8c);
pub const DIM_RED: Color = Color::Rgb(0x8c, 0x4a, 0x54);
pub const DIM_TEAL: Color = Color::Rgb(0x3a, 0x70, 0x6a);

pub fn level_color(level: char) -> Color {
    match level {
        'V' => MUTED,
        'D' => ACCENT,
        'I' => GREEN,
        'W' => YELLOW,
        'E' => RED,
        'F' => MAGENTA,
        _ => MUTED,
    }
}
