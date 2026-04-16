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
pub const MONITOR: u8 = 2;
pub const NETWORK: u8 = 3;
pub const TRACE: u8 = 4;
pub const ISSUES: u8 = 5;
pub const PERMISSIONS: u8 = 6;
pub const FILES: u8 = 7;
pub const DATABASE: u8 = 8;

pub const PANELS: [PanelDef; 9] = [
    PanelDef { number: 0, name: "commands",    focus_key: Some('c') },
    PanelDef { number: 1, name: "logcat",      focus_key: Some('l') },
    PanelDef { number: 2, name: "monitor",     focus_key: Some('m') },
    PanelDef { number: 3, name: "network",     focus_key: Some('n') },
    PanelDef { number: 4, name: "trace",       focus_key: Some('t') },
    PanelDef { number: 5, name: "issues",      focus_key: Some('i') },
    PanelDef { number: 6, name: "permissions", focus_key: Some('p') },
    PanelDef { number: 7, name: "files",       focus_key: Some('f') },
    PanelDef { number: 8, name: "database",    focus_key: Some('d') },
];

pub fn by_number(n: u8) -> &'static PanelDef {
    PANELS.iter().find(|p| p.number == n).unwrap()
}

pub fn by_focus_key(key: char) -> Option<&'static PanelDef> {
    PANELS.iter().find(|p| p.focus_key == Some(key))
}
