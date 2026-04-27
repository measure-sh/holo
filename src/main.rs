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
mod anrs;
mod crashes;
mod issues;
mod issues_ui;
mod files;
mod files_ui;
mod dropbox;
mod logcat;
mod monitor;
mod monitor_ui;
mod network;
mod network_ui;
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
mod vitals;

use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEventKind,
};
use crossterm::execute;

use adb::{Adb, RealAdb};
use app::App;
use dispatch::DispatchContext;

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb: Arc<dyn Adb> = Arc::new(RealAdb);
    let initial_device = adb.list_devices().ok()
        .and_then(|mut d| if !d.is_empty() { Some(d.swap_remove(0)) } else { None });

    let terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = run_app(terminal, adb, initial_device);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run_app(
    mut terminal: ratatui::DefaultTerminal,
    adb: Arc<dyn Adb>,
    initial_device: Option<adb::Device>,
) -> Result<()> {
    theme::load_saved();
    let mut app = App::new(initial_device.clone(), None);
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let (adb_status_tx, adb_status_rx) = std::sync::mpsc::channel();
    adb::set_status_sender(adb_status_tx);
    let mut ctx = DispatchContext {
        adb: adb.clone(),
        data: None,
        title: String::new(),
        devices_rx: None,
        packages_rx: None,
        pending_emulator_rx: None,
        pending_build_rx: None,
        command_tx,
        command_rx,
        adb_status_rx,
        pending_redraw: false,
    };

    if let Some(device) = &initial_device {
        app.commands_mut().is_emulator = device.serial.starts_with("emulator-");
        ctx.pending_build_rx = Some(dispatch::spawn_select_and_build(
            adb.clone(),
            device.clone(),
            app.toolbar().last_package.clone(),
            *app.panel_visibility(),
        ));
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

        if let Some(d) = &mut ctx.data
            && let (Some(s), Some(_)) = (&serial, &package)
        {
            let was_connected = d.device_connected;
            d.poll(&mut app, s);
            app.toolbar_mut().device_connected = d.device_connected;
            if let Some(path) = d.take_pending_editor_open()
                && dispatch::launch_editor(&path)
            {
                ctx.pending_redraw = true;
            }
            if !was_connected && d.device_connected
                && let Some(device) = app.toolbar().device.clone()
                && let Some(pkg) = package.clone()
            {
                app.reset_for_new_app(&pkg);
                ctx.data = None;
                ctx.title = String::new();
                ctx.pending_build_rx = Some(dispatch::spawn_build(
                    ctx.adb.clone(),
                    device,
                    pkg,
                    *app.panel_visibility(),
                ));
            }
        }

        app.database_state_mut().panel_visible = app.is_panel_visible(panel::DATABASE);

        if let (Some(d), Some(s), Some(p)) = (&mut ctx.data, &serial, &package) {
            if app.database_state().needs_table_refresh()
                && let Some((db_name, table_name)) = app.database_state().selected_table.clone()
            {
                app.database_state_mut().mark_refresh_started();
                d.start_fetch_table_data(ctx.adb.clone(), s.clone(), p.clone(), db_name, table_name, database::FetchKind::Tail);
            } else if let Some(offset) = app.database_state().needs_previous_rows()
                && let Some((db_name, table_name)) = app.database_state().selected_table.clone()
            {
                app.database_state_mut().mark_load_more_started();
                d.start_fetch_table_data(ctx.adb.clone(), s.clone(), p.clone(), db_name, table_name, database::FetchKind::At(offset));
            } else if let Some(offset) = app.database_state().needs_next_rows()
                && let Some((db_name, table_name)) = app.database_state().selected_table.clone()
            {
                app.database_state_mut().mark_load_more_started();
                d.start_fetch_table_data(ctx.adb.clone(), s.clone(), p.clone(), db_name, table_name, database::FetchKind::At(offset));
            }

            if !d.files_stat_in_flight()
                && app.files_state().loading_meta
                && let Some(path) = app.files_state().selected_file.clone()
            {
                d.start_stat_file(ctx.adb.clone(), s.clone(), p.clone(), path);
            }
            if !d.files_cat_in_flight()
                && let Some(path) = app.files_state_mut().take_pending_cat()
            {
                d.start_cat_file(ctx.adb.clone(), s.clone(), p.clone(), path);
            }
        }

        let battery_level = ctx.data.as_ref().and_then(|d| d.battery_level);
        let logcat_lines: &[String] = ctx.data.as_ref().map_or(&[], |d| &d.logcat_lines);

        if ctx.pending_redraw {
            terminal.clear()?;
            ctx.pending_redraw = false;
        }

        terminal.draw(|frame| {
            ui::render_app(frame, &ctx.title, battery_level, &mut app, logcat_lines)
        })?;

        let poll_timeout = if app.has_active_animation() {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(1)
        };
        if !event::poll(poll_timeout)? {
            continue;
        }
        loop {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let action = app.handle_key(key);
                    if ctx.dispatch(action, &mut app) {
                        return Ok(());
                    }
                    if let Some(d) = &ctx.data {
                        d.update_monitor_visibility(app.panel_visibility());
                    }
                }
                Event::Mouse(mouse) => {
                    let code = match mouse.kind {
                        MouseEventKind::ScrollUp => Some(KeyCode::Up),
                        MouseEventKind::ScrollDown => Some(KeyCode::Down),
                        _ => None,
                    };
                    if let Some(code) = code {
                        let key = KeyEvent::new(code, KeyModifiers::NONE);
                        let action = app.handle_key(key);
                        if ctx.dispatch(action, &mut app) {
                            return Ok(());
                        }
                    }
                }
                Event::Resize(_, _) => {
                    terminal.clear()?;
                }
                _ => {}
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}
