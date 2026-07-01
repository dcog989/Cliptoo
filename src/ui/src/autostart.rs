use anyhow::Result;
use std::path::PathBuf;

const AUTOSTART_DIR: &str = "autostart";
const DESKTOP_FILE: &str = "cliptoo.desktop";

fn autostart_path() -> PathBuf {
    let config_home = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    config_home.join(AUTOSTART_DIR).join(DESKTOP_FILE)
}

pub fn ensure_autostart() -> Result<()> {
    let path = autostart_path();
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
        exe.display()
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, content.as_bytes())?;
    Ok(())
}

pub fn remove_autostart() -> Result<()> {
    let path = autostart_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
