//! App preferences, stored in the `settings` table. The file used by earlier versions
//! (`$XDG_CONFIG_HOME/oxide-calendar/settings`, `key=value` lines) is imported once and removed.

use std::path::{Path, PathBuf};

use crate::storage::{Repository, StorageError};

const DARK_THEME: &str = "dark_theme";

/// `None` follows the system colour scheme until the user chooses.
pub fn dark_theme(repo: &Repository) -> Result<Option<bool>, StorageError> {
    Ok(match repo.get_setting(DARK_THEME)?.as_deref() {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    })
}

pub fn set_dark_theme(repo: &Repository, dark: bool) -> Result<(), StorageError> {
    repo.set_setting(DARK_THEME, if dark { "true" } else { "false" })
}

/// Reads a legacy `key=value` file. Invalid lines are ignored.
pub fn parse_legacy(text: &str) -> Option<bool> {
    let mut dark = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == DARK_THEME
        {
            match value.trim() {
                "true" => dark = Some(true),
                "false" => dark = Some(false),
                _ => {}
            }
        }
    }
    dark
}

/// Imports the legacy file once. Keeps a value already stored in the database and removes the file.
pub fn import_legacy_file(repo: &Repository, path: &Path) -> Result<(), StorageError> {
    if !path.is_file() {
        return Ok(());
    }
    if let Some(dark) = std::fs::read_to_string(path).ok().and_then(|text| parse_legacy(&text))
        && dark_theme(repo)?.is_none()
    {
        set_dark_theme(repo, dark)?;
    }
    std::fs::remove_file(path)?;
    Ok(())
}

/// `$XDG_CONFIG_HOME/oxide-calendar/settings`, falling back to `~/.config`.
pub fn legacy_settings_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("oxide-calendar").join("settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("oxide-cal-settings-{name}-{nanos}"))
    }

    #[test]
    fn stored_theme_round_trip() {
        let repo = Repository::in_memory().unwrap();
        assert_eq!(dark_theme(&repo).unwrap(), None);
        set_dark_theme(&repo, true).unwrap();
        assert_eq!(dark_theme(&repo).unwrap(), Some(true));
        set_dark_theme(&repo, false).unwrap();
        assert_eq!(dark_theme(&repo).unwrap(), Some(false));
    }

    #[test]
    fn legacy_parse_ignores_invalid_lines() {
        assert_eq!(parse_legacy("# c\n\nsem igual\ndark_theme=talvez\n"), None);
        assert_eq!(parse_legacy("dark_theme = true"), Some(true));
    }

    #[test]
    fn legacy_file_is_imported_once_and_removed() {
        let dir = temp_dir("import");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings");
        std::fs::write(&file, "dark_theme=true\n").unwrap();
        let repo = Repository::in_memory().unwrap();
        import_legacy_file(&repo, &file).unwrap();
        assert_eq!(dark_theme(&repo).unwrap(), Some(true));
        assert!(!file.exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn database_value_wins_over_legacy_file() {
        let dir = temp_dir("keep");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings");
        std::fs::write(&file, "dark_theme=true\n").unwrap();
        let repo = Repository::in_memory().unwrap();
        set_dark_theme(&repo, false).unwrap();
        import_legacy_file(&repo, &file).unwrap();
        assert_eq!(dark_theme(&repo).unwrap(), Some(false));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_legacy_file_is_a_no_op() {
        let repo = Repository::in_memory().unwrap();
        import_legacy_file(&repo, &temp_dir("none").join("settings")).unwrap();
        assert_eq!(dark_theme(&repo).unwrap(), None);
    }
}
