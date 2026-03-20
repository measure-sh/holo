mod adb;
mod app;
mod apps;
mod battery;
mod boot;
mod selector;
mod theme;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};

use adb::{Adb, Device, RealAdb};
use app::{Action, App};
use boot::BootResult;

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb = Arc::new(RealAdb);
    let devices = adb.list_devices()?;

    let device = match boot::resolve_device(devices) {
        BootResult::NoDevices => {
            eprintln!("No devices connected. Connect a device and try again.");
            std::process::exit(1);
        }
        BootResult::Selected(device) => device,
        BootResult::NeedsSelection(devices) => selector::run(devices)?,
    };

    let terminal = ratatui::init();
    let result = run_app(terminal, adb, &device);
    ratatui::restore();
    result
}

fn run_app(mut terminal: ratatui::DefaultTerminal, adb: Arc<dyn Adb>, device: &Device) -> Result<()> {
    let title = format!(" {} ", selector::selector_label(device));
    let mut app = App::new();

    let battery_rx = battery::spawn_poller(adb.clone(), device.serial.clone());
    let mut battery_level: Option<u8> = None;

    let apps_rx = apps::spawn_poller(adb, device.serial.clone());
    let mut packages: Vec<String> = Vec::new();

    loop {
        while let Ok(level) = battery_rx.try_recv() {
            battery_level = Some(level);
        }
        while let Ok(pkgs) = apps_rx.try_recv() {
            packages = pkgs;
        }

        let now = chrono::Local::now();
        let time_str = format!(" {} ", now.format("%H:%M:%S"));
        terminal.draw(|frame| {
            ui::render_app(frame, &title, &time_str, battery_level, app.panel_visibility(), &packages)
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if matches!(app.handle_key(key.code), Action::Quit) {
                    return Ok(());
                }
            }
        }
    }
}
