//! Desktop integration for the AppImage build.
//!
//! An AppImage has no installer, so the menu entry and icon are written on
//! launch. Release filenames carry the version, so `Exec` is repointed whenever
//! the running file differs. This also fixes the global hotkey: the portal only
//! accepts the app id `cliptoo` when an installed `cliptoo.desktop` matches it.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::autostart::{DESKTOP_FILE, desktop_entry, exec_quote};

const ICON_FILE: &str = "cliptoo.svg";
const ICON_SVG: &[u8] = include_bytes!("../assets/cliptoo.svg");
/// Records the AppImage this entry was generated for, so a stale entry is
/// recognised without parsing the quoted `Exec` value.
const MARKER: &str = "X-Cliptoo-AppImage=";
/// A package install already provides the menu entry; a user copy would shadow
/// it, so none is written in that case.
const SYSTEM_DESKTOP_FILE: &str = "/usr/share/applications/cliptoo.desktop";

/// Registers the running AppImage, or removes an entry left behind by one that
/// no longer exists. Runs before the hotkey loop so the portal sees the entry.
pub fn install() {
    if let Err(e) = sync() {
        tracing::warn!("AppImage desktop integration failed: {e:#}");
    }
}

fn sync() -> Result<()> {
    let running = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    let system_install = Path::new(SYSTEM_DESKTOP_FILE).exists();
    let data = dirs::data_dir().context("no XDG data directory — is HOME set?")?;
    let applications = data.join("applications");

    let Some(appimage) = running.filter(|_| !system_install) else {
        remove_generated(&applications);
        return Ok(());
    };

    let icons = data.join("icons/hicolor/scalable/apps");
    std::fs::create_dir_all(&applications)?;
    std::fs::create_dir_all(&icons)?;
    write_if_changed(&icons.join(ICON_FILE), ICON_SVG)?;

    let path = applications.join(DESKTOP_FILE);
    let content = appimage_entry(&appimage);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("writing {}", path.display()))?;
        refresh_menu(&applications);
    }
    Ok(())
}

/// Deletes the entry only when it carries `MARKER`: a package install could
/// otherwise be removed by a run of a different build.
fn remove_generated(applications: &Path) {
    let path = applications.join(DESKTOP_FILE);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let Some(appimage) = marker_value(&content) else {
        return;
    };
    if Path::new(appimage).is_file() {
        return;
    }
    let _ = std::fs::remove_file(&path);
    refresh_menu(applications);
}

fn appimage_entry(appimage: &Path) -> String {
    let mut entry = desktop_entry(&exec_quote(&appimage.to_string_lossy()), false);
    entry.push_str(&format!("{MARKER}{}\n", appimage.display()));
    entry
}

fn marker_value(content: &str) -> Option<&str> {
    content.lines().find_map(|line| line.strip_prefix(MARKER))
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

/// Rebuilds the desktop-file cache. Absent on minimal systems; KDE also
/// refreshes through KSycoca, so a failure is not fatal.
fn refresh_menu(applications: &Path) {
    let _ = std::process::Command::new("update-desktop-database")
        .arg(applications)
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_registers_the_running_appimage() {
        let entry = appimage_entry(Path::new("/home/user/Apps/Cliptoo 2.AppImage"));
        assert!(
            entry.contains("Exec=\"/home/user/Apps/Cliptoo 2.AppImage\"\n"),
            "{entry}"
        );
        assert!(
            entry.contains(&format!("{MARKER}/home/user/Apps/Cliptoo 2.AppImage\n")),
            "{entry}"
        );
        // A menu entry must stay visible, unlike the autostart file.
        assert!(!entry.contains("NoDisplay=true"), "{entry}");
    }

    #[test]
    fn marker_is_read_back_verbatim() {
        let entry = appimage_entry(Path::new("/opt/cliptoo-2.23.0-x86_64.AppImage"));
        assert_eq!(
            marker_value(&entry),
            Some("/opt/cliptoo-2.23.0-x86_64.AppImage")
        );
        assert_eq!(marker_value("[Desktop Entry]\n"), None);
    }
}
