use anyhow::{Context, Result};
use std::path::PathBuf;

const AUTOSTART_DIR: &str = "autostart";
const DESKTOP_FILE: &str = "cliptoo.desktop";

fn autostart_path() -> Result<PathBuf> {
    let config_home = dirs::config_dir().context("no XDG config directory — is HOME set?")?;
    Ok(config_home.join(AUTOSTART_DIR).join(DESKTOP_FILE))
}

/// Quote `value` for a desktop-entry `Exec` value: enclosed in double quotes
/// with `"`, `` ` ``, `$` and `\` escaped by a backslash, per the freedesktop
/// Desktop Entry specification. Without this, an executable path containing
/// spaces (or those characters) would be misparsed by the session autostart.
fn exec_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn ensure_autostart() -> Result<()> {
    let path = autostart_path()?;
    let exe = std::env::current_exe()?;

    let content = format!(
        "[Desktop Entry]\n\
         Name=Cliptoo\n\
         Comment=Clipboard manager for KDE Plasma 6\n\
         Exec={}\n\
         Icon=cliptoo\n\
         Type=Application\n\
         Categories=Utility;\n\
         Keywords=clipboard;paste;history;\n\
         StartupNotify=false\n\
         NoDisplay=true\n\
         X-KDE-StartupNotify=false\n",
        exec_quote(&exe.to_string_lossy())
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, content.as_bytes())?;
    Ok(())
}

pub fn remove_autostart() -> Result<()> {
    let path = autostart_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
