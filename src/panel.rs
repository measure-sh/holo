use ratatui::style::Color;

use crate::theme;

pub struct PanelDef {
    pub number: u8,
    pub name: &'static str,
    pub focus_key: Option<char>,
}

impl PanelDef {
    pub fn border_color(&self, focused: bool) -> Color {
        let t = theme::current();
        if focused { t.accent } else { t.surface }
    }
}

pub const COMMANDS: u8 = 0;
pub const LOGCAT: u8 = 1;
pub const DISK: u8 = 2;
pub const SYSTEM: u8 = 3;
pub const PERMISSIONS: u8 = 4;
pub const TRACE: u8 = 5;
pub const CRASHES: u8 = 6;
pub const ANRS: u8 = 7;
pub const FILES: u8 = 8;
pub const DATABASE: u8 = 9;

pub const PANELS: [PanelDef; 10] = [
    PanelDef { number: 0, name: "commands",     focus_key: Some('c') },
    PanelDef { number: 1, name: "logcat",       focus_key: Some('l') },
    PanelDef { number: 2, name: "disk",         focus_key: None },
    PanelDef { number: 3, name: "cpu & memory", focus_key: None },
    PanelDef { number: 4, name: "permissions",  focus_key: Some('p') },
    PanelDef { number: 5, name: "trace",        focus_key: Some('t') },
    PanelDef { number: 6, name: "crashes",      focus_key: Some('e') },
    PanelDef { number: 7, name: "anrs",         focus_key: Some('a') },
    PanelDef { number: 8, name: "files",        focus_key: Some('f') },
    PanelDef { number: 9, name: "database",     focus_key: Some('d') },
];

pub fn by_number(n: u8) -> &'static PanelDef {
    PANELS.iter().find(|p| p.number == n).unwrap()
}

pub fn by_focus_key(key: char) -> Option<&'static PanelDef> {
    PANELS.iter().find(|p| p.focus_key == Some(key))
}
