use anyhow::{Context, Result};
use std::path::PathBuf;

const AUTOSTART_DIR: &str = "autostart";
/// Must match the app id registered with the portal in `hotkeys.rs`.
pub(crate) const DESKTOP_FILE: &str = "cliptoo.desktop";

fn autostart_path() -> Result<PathBuf> {
    let config_home = dirs::config_dir().context("no XDG config directory — is HOME set?")?;
    Ok(config_home.join(AUTOSTART_DIR).join(DESKTOP_FILE))
}

/// Quote `value` for a desktop-entry `Exec` value: enclosed in double quotes
/// with `"`, `` ` ``, `$` and `\` escaped by a backslash, per the freedesktop
/// Desktop Entry specification. Without this, an executable path containing
/// spaces (or those characters) would be misparsed by the session autostart.
pub(crate) fn exec_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            // A literal percent starts a field code in `Exec`; `%%` escapes it.
            '%' => {
                out.push('%');
                out.push('%');
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The `[Desktop Entry]` body shared by the autostart file and the AppImage
/// menu entry, so their name and icon cannot drift apart.
pub(crate) fn desktop_entry(exec: &str, no_display: bool) -> String {
    format!(
        "[Desktop Entry]\n\
         Name=Cliptoo\n\
         GenericName=Clipboard Manager\n\
         Comment=Clipboard manager for KDE Plasma 6\n\
         Exec={exec}\n\
         Icon=cliptoo\n\
         Type=Application\n\
         Categories=Utility;Qt;KDE;\n\
         Keywords=clipboard;paste;history;\n\
         StartupNotify=false\n\
         X-KDE-StartupNotify=false\n{}",
        if no_display { "NoDisplay=true\n" } else { "" }
    )
}

pub fn ensure_autostart() -> Result<()> {
    let path = autostart_path()?;
    // Inside an AppImage `current_exe()` is a path in the temporary mount, which
    // disappears at logout; `$APPIMAGE` is the file the user launches and
    // survives. Writing the mount path would silently disable autostart.
    let exe = match std::env::var_os("APPIMAGE") {
        Some(appimage) => PathBuf::from(appimage),
        None => std::env::current_exe()?,
    };

    let content = desktop_entry(&exec_quote(&exe.to_string_lossy()), true);

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

#[cfg(test)]
mod tests {
    use super::exec_quote;

    #[test]
    fn exec_quote_matches_the_desktop_entry_spec() {
        assert_eq!(
            exec_quote("/opt/My App/cliptoo"),
            r#""/opt/My App/cliptoo""#
        );
        assert_eq!(exec_quote("a\"b"), r#""a\"b""#);
        assert_eq!(exec_quote("/opt/a$b"), r#""/opt/a\$b""#);
        assert_eq!(exec_quote("/opt/100%/cliptoo"), r#""/opt/100%%/cliptoo""#);
    }
}
