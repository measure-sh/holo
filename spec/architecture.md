# Architecture

## ADB Abstraction
- Define an `Adb` trait with methods for each adb operation (`list_devices`, `list_packages`, etc.)
- `RealAdb` struct implements the trait via `std::process::Command`
- `MockAdb` struct implements the trait with canned data for tests
- All app logic depends on the trait, never calls adb directly

## Module Structure
- `adb/` — trait definition + real implementation + mock
- `app/` — application state, event handling, business logic
- `ui/` — rendering, widgets, layout
- `main.rs` — wiring: creates `RealAdb`, builds app, runs event loop

## State Management
- Single `App` struct owns all application state
- `App` receives `Box<dyn Adb>` at construction
- UI reads from `App` state, never fetches data itself

## Event Loop
- Standard ratatui synchronous loop: draw → read event → update state → repeat
- All event handling goes through `App` methods
