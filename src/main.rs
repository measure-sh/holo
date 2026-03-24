mod adb;
mod app;
mod apps;
mod battery;
mod clipboard;
mod commands;
mod data;
mod database;
mod database_ui;
mod dispatch;
mod files;
mod files_ui;
mod logcat;
mod logcat_state;
mod monitor;
mod monitor_ui;
mod logcat_ui;
mod panel;
mod permissions;
mod permissions_ui;
mod processes;
mod selector;
mod theme;
mod toolbar;
mod trace;
mod trace_ui;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};

use adb::{Adb, RealAdb};
use app::App;
use dispatch::DispatchContext;

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb: Arc<dyn Adb> = Arc::new(RealAdb);
    let initial_device = adb.list_devices().ok()
        .and_then(|mut d| if !d.is_empty() { Some(d.swap_remove(0)) } else { None });

    let terminal = ratatui::init();
    let result = run_app(terminal, adb, initial_device);
    ratatui::restore();
    result
}

fn run_app(
    mut terminal: ratatui::DefaultTerminal,
    adb: Arc<dyn Adb>,
    initial_device: Option<adb::Device>,
) -> Result<()> {
    let mut app = App::new(initial_device.clone(), None);
    let mut ctx = DispatchContext {
        adb: adb.clone(),
        data: None,
        title: String::new(),
        devices_rx: None,
        packages_rx: None,
        pending_emulator_rx: None,
    };

    if let Some(device) = &initial_device {
        let (packages, auto) = dispatch::try_auto_select_package(&adb, device, app.toolbar().last_package.as_deref());
        app.toolbar_mut().receive_packages(packages);
        if let Some(pkg) = auto {
            app.toolbar_mut().package = Some(pkg.clone());
            ctx.data = Some(dispatch::build_data(&adb, device, &pkg, &mut app));
            ctx.title = dispatch::build_title(ctx.data.as_ref().unwrap());
        } else {
            app.toolbar_mut().open = Some(toolbar::DropdownKind::App);
        }
    }

    loop {
        ctx.poll_receivers(&mut app);

        let (serial, package) = {
            let tb = app.toolbar();
            (
                tb.device.as_ref().map(|d| d.serial.clone()),
                tb.package.clone(),
            )
        };

        if let Some(d) = &mut ctx.data {
            if let (Some(s), Some(p)) = (&serial, &package) {
                d.poll(&mut app, s, p);
                app.toolbar_mut().device_connected = d.device_connected;
            }
        }

        let battery_level = ctx.data.as_ref().and_then(|d| d.battery_level);
        let logcat_lines: &[String] = ctx.data.as_ref().map_or(&[], |d| &d.logcat_lines);

        terminal.draw(|frame| {
            ui::render_app(frame, &ctx.title, battery_level, &mut app, logcat_lines)
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let action = app.handle_key(key);
                if ctx.dispatch(action, &mut app) {
                    return Ok(());
                }
            }
        }
    }
}
