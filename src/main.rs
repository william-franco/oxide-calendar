mod bridge;
mod domain;
mod ics;
mod settings;
mod storage;
mod viewmodel;

slint::include_modules!();

use std::path::PathBuf;

/// `$XDG_DATA_HOME/oxide-calendar/calendar.db`, falling back to `~/.local/share`.
fn database_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("oxide-calendar").join("calendar.db")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = storage::Repository::open(&database_path())?;
    let vm = viewmodel::CalendarViewModel::new(repo);
    vm.import_legacy_settings(&settings::legacy_settings_path())?;
    let ui = AppWindow::new()?;
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    let _reminders = bridge::bind(&ui, vm);
    ui.run()?;
    Ok(())
}
