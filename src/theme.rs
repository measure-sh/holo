use std::path::PathBuf;
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
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub info: Color,
    pub secondary: Color,
    pub spark_cpu: Color,
    pub spark_mem: Color,
    pub spark_disk: Color,
    pub spark_rx: Color,
    pub spark_tx: Color,
}

const TOKYO_NIGHT: Theme = Theme {
    name:         "Tokyo Night (Omarchy)",
    bg:           Color::Rgb(0x00, 0x00, 0x00),
    overlay:      Color::Rgb(0x16, 0x16, 0x22),
    fg:           Color::Rgb(0xa9, 0xb1, 0xd6),
    accent:       Color::Rgb(0x7a, 0xa2, 0xf7),
    danger:       Color::Rgb(0xf7, 0x76, 0x8e),
    success:      Color::Rgb(0x9e, 0xce, 0x6a),
    warning:      Color::Rgb(0xe0, 0xaf, 0x68),
    secondary:    Color::Rgb(0xbb, 0x9a, 0xf7),
    info:         Color::Rgb(0x0d, 0xb9, 0xd7),
    surface:      Color::Rgb(0x32, 0x34, 0x4a),
    muted:        Color::Rgb(0x56, 0x5f, 0x89),
    spark_cpu:    Color::Rgb(0x7d, 0xcf, 0xff),
    spark_mem:    Color::Rgb(0x7a, 0xa2, 0xf7),
    spark_disk:   Color::Rgb(0x9d, 0x7c, 0xd8),
    spark_rx:     Color::Rgb(0xbb, 0x9a, 0xf7),
    spark_tx:     Color::Rgb(0xf7, 0x76, 0x8e),
};

const AKAITO: Theme = Theme {
    name:         "Akaito (Omarchy)",
    bg:           Color::Rgb(0xf3, 0xe4, 0xcb),
    overlay:      Color::Rgb(0xd8, 0xc8, 0xab),
    fg:           Color::Rgb(0x4d, 0x2e, 0x1a),
    accent:       Color::Rgb(0xa3, 0x2f, 0x1a),
    danger:       Color::Rgb(0xa4, 0x37, 0x3c),
    success:      Color::Rgb(0xa4, 0x6d, 0x2d),
    warning:      Color::Rgb(0xa8, 0x61, 0x1f),
    secondary:    Color::Rgb(0x9c, 0x35, 0x21),
    info:         Color::Rgb(0x75, 0x58, 0x33),
    surface:      Color::Rgb(0xc8, 0xb4, 0x94),
    muted:        Color::Rgb(0x9f, 0x82, 0x53),
    spark_cpu:    Color::Rgb(0xa3, 0x2f, 0x1a),
    spark_mem:    Color::Rgb(0xb8, 0x52, 0x20),
    spark_disk:   Color::Rgb(0xa8, 0x61, 0x1f),
    spark_rx:     Color::Rgb(0xa4, 0x6d, 0x2d),
    spark_tx:     Color::Rgb(0x75, 0x58, 0x33),
};

const DARK: Theme = Theme {
    name:         "Dark",
    bg:           Color::Rgb(0x09, 0x09, 0x09),
    overlay:      Color::Rgb(0x18, 0x18, 0x18),
    fg:           Color::Rgb(0xe8, 0xe8, 0xe8),
    accent:       Color::Rgb(0xff, 0xd2, 0x30),
    danger:       Color::Rgb(0xff, 0x5b, 0x5b),
    success:      Color::Rgb(0x34, 0xd3, 0x99),
    warning:      Color::Rgb(0xfb, 0x92, 0x3c),
    secondary:    Color::Rgb(0xc0, 0x84, 0xfc),
    info:         Color::Rgb(0x22, 0xd3, 0xee),
    surface:      Color::Rgb(0x38, 0x38, 0x38),
    muted:        Color::Rgb(0x78, 0x78, 0x78),
    spark_cpu:    Color::Rgb(0xfd, 0xe0, 0x47),
    spark_mem:    Color::Rgb(0xfb, 0x92, 0x3c),
    spark_disk:   Color::Rgb(0xf4, 0x72, 0xb6),
    spark_rx:     Color::Rgb(0xc0, 0x84, 0xfc),
    spark_tx:     Color::Rgb(0x81, 0x8c, 0xf8),
};

const LIGHT: Theme = Theme {
    name:         "Light",
    bg:           Color::Rgb(0xfa, 0xfa, 0xfa),
    overlay:      Color::Rgb(0xee, 0xee, 0xee),
    fg:           Color::Rgb(0x17, 0x17, 0x17),
    accent:       Color::Rgb(0xa1, 0x83, 0x00),
    danger:       Color::Rgb(0xdc, 0x26, 0x26),
    success:      Color::Rgb(0x16, 0xa3, 0x4a),
    warning:      Color::Rgb(0xc2, 0x57, 0x0a),
    secondary:    Color::Rgb(0x7c, 0x3a, 0xed),
    info:         Color::Rgb(0x08, 0x91, 0xb2),
    surface:      Color::Rgb(0xd0, 0xd0, 0xd0),
    muted:        Color::Rgb(0x6b, 0x6b, 0x6b),
    spark_cpu:    Color::Rgb(0xa1, 0x62, 0x07),
    spark_mem:    Color::Rgb(0xc2, 0x41, 0x0c),
    spark_disk:   Color::Rgb(0xb9, 0x1c, 0x1c),
    spark_rx:     Color::Rgb(0x9f, 0x12, 0x39),
    spark_tx:     Color::Rgb(0x6d, 0x28, 0xd9),
};

const THEMES: [&Theme; 4] = [&DARK, &LIGHT, &TOKYO_NIGHT, &AKAITO];
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> &'static Theme {
    let idx = CURRENT.load(Ordering::Relaxed) as usize;
    THEMES[idx.min(THEMES.len() - 1)]
}

pub fn set_theme(id: usize) {
    let idx = id.min(THEMES.len() - 1);
    CURRENT.store(idx as u8, Ordering::Relaxed);
    save(idx);
}

fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("holo")
        .join("theme")
}

fn save(idx: usize) {
    let path = cache_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(path, idx.to_string());
}

pub fn load_saved() {
    if let Ok(s) = std::fs::read_to_string(cache_path())
        && let Ok(idx) = s.trim().parse::<usize>()
        && idx < THEMES.len()
    {
        CURRENT.store(idx as u8, Ordering::Relaxed);
    }
}

pub fn current_index() -> usize {
    CURRENT.load(Ordering::Relaxed) as usize
}

pub fn theme_count() -> usize {
    THEMES.len()
}


pub fn status_color(code: u16) -> Color {
    let t = current();
    match code {
        200..=299 => t.success,
        300..=399 => t.info,
        400..=499 => t.warning,
        _ => t.danger,
    }
}

pub fn level_color(level: char) -> Color {
    let t = current();
    match level {
        'V' => t.muted,
        'D' => t.secondary,
        'I' => t.info,
        'W' => t.warning,
        'E' | 'F' => t.danger,
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
