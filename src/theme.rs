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
    info:         Color::Rgb(0x0d, 0xb9, 0xd7),
    surface:      Color::Rgb(0x32, 0x34, 0x4a),
    muted:        Color::Rgb(0x56, 0x5f, 0x89),
    spark_cpu:    Color::Rgb(0x93, 0xc5, 0xfd),
    spark_mem:    Color::Rgb(0x60, 0xa5, 0xfa),
    spark_disk:   Color::Rgb(0x3b, 0x82, 0xf6),
    spark_rx:     Color::Rgb(0x25, 0x63, 0xeb),
    spark_tx:     Color::Rgb(0x1d, 0x4e, 0xd8),
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
    info:         Color::Rgb(0x75, 0x58, 0x33),
    surface:      Color::Rgb(0xc8, 0xb4, 0x94),
    muted:        Color::Rgb(0x9f, 0x82, 0x53),
    spark_cpu:    Color::Rgb(0xef, 0x44, 0x44),
    spark_mem:    Color::Rgb(0xdc, 0x26, 0x26),
    spark_disk:   Color::Rgb(0xb9, 0x1c, 0x1c),
    spark_rx:     Color::Rgb(0x99, 0x1b, 0x1b),
    spark_tx:     Color::Rgb(0x7f, 0x1d, 0x1d),
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
    info:         Color::Rgb(0x22, 0xd3, 0xee),
    surface:      Color::Rgb(0x38, 0x38, 0x38),
    muted:        Color::Rgb(0x78, 0x78, 0x78),
    spark_cpu:    Color::Rgb(0xfc, 0xd3, 0x4d),
    spark_mem:    Color::Rgb(0xfb, 0xbf, 0x24),
    spark_disk:   Color::Rgb(0xf5, 0x9e, 0x0b),
    spark_rx:     Color::Rgb(0xd9, 0x77, 0x06),
    spark_tx:     Color::Rgb(0xb4, 0x53, 0x09),
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
    info:         Color::Rgb(0x08, 0x91, 0xb2),
    surface:      Color::Rgb(0xd0, 0xd0, 0xd0),
    muted:        Color::Rgb(0x6b, 0x6b, 0x6b),
    spark_cpu:    Color::Rgb(0xf5, 0x9e, 0x0b),
    spark_mem:    Color::Rgb(0xd9, 0x77, 0x06),
    spark_disk:   Color::Rgb(0xb4, 0x53, 0x09),
    spark_rx:     Color::Rgb(0x92, 0x40, 0x0e),
    spark_tx:     Color::Rgb(0x78, 0x35, 0x0f),
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
        100..=199 => t.spark_cpu,
        200..=299 => t.spark_mem,
        300..=399 => t.spark_disk,
        400..=499 => t.spark_rx,
        _ => t.spark_tx,
    }
}

pub fn level_color(level: char) -> Color {
    let t = current();
    match level {
        'V' => t.spark_cpu,
        'D' => t.spark_mem,
        'I' => t.spark_disk,
        'W' => t.spark_rx,
        'E' | 'F' => t.spark_tx,
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
