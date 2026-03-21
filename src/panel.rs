use ratatui::style::Color;

use crate::theme;

pub struct PanelDef {
    pub number: u8,
    pub name: &'static str,
    pub dim_color: Color,
    pub bright_color: Color,
    pub focus_key: Option<char>,
}

impl PanelDef {
    pub fn is_focusable(&self) -> bool {
        self.focus_key.is_some()
    }

    pub fn border_color(&self, focused: bool) -> Color {
        if focused { self.bright_color } else { self.dim_color }
    }
}

pub const COMMANDS: u8 = 1;
pub const LOGCAT: u8 = 2;
pub const NETWORK: u8 = 3;
pub const SYSTEM: u8 = 4;
pub const DATABASE: u8 = 5;

pub const PANELS: [PanelDef; 5] = [
    PanelDef { number: 1, name: "commands",  dim_color: theme::DIM_TEAL,    bright_color: theme::CYAN,    focus_key: None },
    PanelDef { number: 2, name: "logcat",    dim_color: theme::DIM_GREEN,   bright_color: theme::GREEN,   focus_key: Some('l') },
    PanelDef { number: 3, name: "network",   dim_color: theme::DIM_YELLOW,  bright_color: theme::YELLOW,  focus_key: None },
    PanelDef { number: 4, name: "system",    dim_color: theme::DIM_CYAN,    bright_color: theme::CYAN,    focus_key: None },
    PanelDef { number: 5, name: "database",  dim_color: theme::DIM_YELLOW,  bright_color: theme::YELLOW,  focus_key: Some('d') },
];

pub fn by_number(n: u8) -> &'static PanelDef {
    PANELS.iter().find(|p| p.number == n).unwrap()
}

pub fn by_focus_key(key: char) -> Option<&'static PanelDef> {
    PANELS.iter().find(|p| p.focus_key == Some(key))
}
