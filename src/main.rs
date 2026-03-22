mod adb;
mod app;
mod apps;
mod battery;
mod boot;
mod boot_ui;
mod data;
mod database;
mod database_ui;
mod logcat;
mod logcat_state;
mod logcat_ui;
mod panel;
mod processes;
mod selector;
mod theme;
mod ui;

use std::sync::{mpsc, Arc};
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use adb::{Adb, Device, RealAdb};
use app::{Action, App};
use boot::{BootAction, BootPhase};
use data::DataSources;

fn spawn_device_poller(adb: Arc<dyn Adb>) -> mpsc::Receiver<Vec<Device>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        if let Ok(devices) = adb.list_devices() {
            if tx.send(devices).is_err() {
                return;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    });
    rx
}

fn spawn_app_action(
    adb: &Arc<dyn Adb>,
    serial: &str,
    package: &str,
    f: impl FnOnce(Arc<dyn Adb>, &str, &str) -> Result<()> + Send + 'static,
) {
    let adb = adb.clone();
    let serial = serial.to_string();
    let package = package.to_string();
    std::thread::spawn(move || {
        let _ = f(adb, &serial, &package);
    });
}

fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let adb: Arc<dyn Adb> = Arc::new(RealAdb);
    let terminal = ratatui::init();
    let result = run_boot(terminal, adb);
    ratatui::restore();
    result
}

fn run_boot(mut terminal: ratatui::DefaultTerminal, adb: Arc<dyn Adb>) -> Result<()> {
    let mut phase = BootPhase::WaitingForDevice;
    let device_rx = spawn_device_poller(adb.clone());
    let mut apps_rx: Option<mpsc::Receiver<Vec<String>>> = None;
    let mut tick: u8 = 0;
    let mut confirming_quit = false;

    loop {
        while let Ok(devices) = device_rx.try_recv() {
            phase.receive_devices(devices);
        }

        if let BootPhase::SelectApp { device, .. } = &phase {
            if apps_rx.is_none() {
                apps_rx = Some(apps::spawn_poller(adb.clone(), device.serial.clone()));
            }
        }

        if let Some(rx) = &apps_rx {
            while let Ok(pkgs) = rx.try_recv() {
                phase.receive_packages(pkgs);
            }
        }

        if phase.is_done() {
            let (device, package) = phase.into_result().unwrap();
            return run_app(terminal, adb, &device, &package);
        }

        terminal.draw(|frame| boot_ui::render_boot(frame, &phase, tick, confirming_quit))?;

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if confirming_quit {
                    confirming_quit = false;
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                    continue;
                }
                match phase.handle_key(key.code) {
                    BootAction::Quit => confirming_quit = true,
                    BootAction::Continue => {}
                }
            }
        }

        tick = tick.wrapping_add(1);
    }
}

fn run_app(
    mut terminal: ratatui::DefaultTerminal,
    adb: Arc<dyn Adb>,
    device: &Device,
    package: &str,
) -> Result<()> {
    let title = format!(" {} — {} ", selector::selector_label(device), package);
    let mut app = App::new();
    let mut data = DataSources::new(adb.clone(), &device.serial, package);
    app.set_layout_bounds(data.initial_layout_bounds);
    app.set_airplane_mode(data.initial_airplane_mode);

    loop {
        data.poll(&mut app, &device.serial, package);

        let now = chrono::Local::now();
        let time_str = format!(" {} ", now.format("%H:%M:%S"));
        terminal.draw(|frame| {
            ui::render_app(frame, &title, &time_str, data.battery_level, &mut app, &data.logcat_lines, data.monitored_pid)
        })?;

        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.handle_key(key) {
                    Action::Quit => return Ok(()),
                    Action::OpenApp => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, p| {
                            adb.launch_app(s, p)
                        });
                    }
                    Action::KillApp => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, p| {
                            adb.kill_app(s, p)
                        });
                    }
                    Action::ClearData => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, p| {
                            adb.clear_app_data(s, p)
                        });
                    }
                    Action::WakeScreen => {
                        spawn_app_action(&adb, &device.serial, package, |adb, s, _| {
                            adb.wake_screen(s)
                        });
                    }
                    Action::ToggleLayoutBounds => {
                        let enabled = app.layout_bounds();
                        spawn_app_action(&adb, &device.serial, package, move |adb, s, _| {
                            adb.set_layout_bounds(s, enabled)
                        });
                    }
                    Action::ToggleAirplaneMode => {
                        let enabled = app.airplane_mode();
                        spawn_app_action(&adb, &device.serial, package, move |adb, s, _| {
                            adb.set_airplane_mode(s, enabled)
                        });
                    }
                    Action::SetNetworkSpeed => {
                        let bps = app.network_speed().bytes_per_second();
                        spawn_app_action(&adb, &device.serial, package, move |adb, s, _| {
                            adb.set_network_speed(s, bps)
                        });
                    }
                    Action::ResetLogcat => {
                        data.logcat_lines.clear();
                    }
                    Action::ResetDb => {
                        data.restart_db_detection(adb.clone(), device.serial.clone(), package.to_string());
                    }
                    Action::CopyDbResult(text) => {
                        copy_to_clipboard(&text);
                    }
                    Action::RunQuery(db, sql) => {
                        data.start_query(adb.clone(), device.serial.clone(), package.to_string(), db, sql);
                    }
                    Action::PullDb(db) => {
                        data.start_pull(adb.clone(), device.serial.clone(), package.to_string(), db);
                    }
                    Action::None => {}
                }
            }
        }
    }
}
