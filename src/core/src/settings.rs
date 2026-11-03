use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Prefix that marks a tag search (`##work`). Not user-configurable; background
/// refreshes with no settings handle must use the same one as the search box.
pub const DEFAULT_TAG_PREFIX: &str = "##";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendToApp {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Hotkeys
    pub hotkey: String,

    // Window
    pub window_width: f64,
    pub window_height: f64,
    pub editor_window_width: f64,
    pub editor_window_height: f64,
    // Settings window size, persisted so user resizes survive restarts. A
    // height the user never adjusted follows the main window instead; the flag
    // distinguishes a real resize from heights older versions saved on close.
    pub settings_window_width: f64,
    pub settings_window_height: Option<f64>,
    pub settings_window_height_custom: bool,

    // Theming
    // accent_color is a "#RRGGBB" string; empty means "use the OS accent".
    pub accent_color: String,
    pub theme: String,

    // Typography / layout
    pub font_family: String,
    pub font_size: f64,
    pub preview_font_size: f64,
    pub clip_item_padding: String,
    // When true, clip-list body text is tinted by clip type. Text clips keep
    // the base list colour. See `ClipTypeMap.color-for` for the mapping.
    pub color_code_clip_types: bool,

    // Behaviour
    pub hover_preview_delay: u32,
    pub hover_image_preview_size: u32,
    // When true, rich clips paste as stripped text and file clips paste as
    // their path text instead of offering `text/rtf`/`text/html`/`text/uri-list`.
    pub paste_as_plain_text: bool,
    // When true, pasting a clip bumps it to the top of the list.
    pub paste_moves_clip_to_top: bool,
    // Modifier key that activates the quick-paste overlay:
    // "Right Alt" | "Left Alt" | "Control".
    pub quick_paste_modifier: String,

    // Data / retention
    pub max_clips: u32,
    pub max_age_days: u32,
    pub tag_prefix: String,

    // External tools
    pub compare_tool_path: String,
    pub send_to_apps: Vec<SendToApp>,
    pub blacklisted_apps: Vec<String>,

    // System
    pub start_with_system: bool,
    pub always_close_to_tray: bool,
    pub logging_level: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Alt+Q".to_string(),
            window_width: 460.0,
            window_height: 600.0,
            editor_window_width: 520.0,
            editor_window_height: 420.0,
            settings_window_width: 560.0,
            settings_window_height: None,
            settings_window_height_custom: false,
            accent_color: String::new(),
            theme: "System".to_string(),
            font_family: "Inter".to_string(),
            font_size: 13.0,
            preview_font_size: 12.0,
            clip_item_padding: "Standard".to_string(),
            color_code_clip_types: false,
            hover_preview_delay: 400,
            hover_image_preview_size: 300,
            paste_as_plain_text: false,
            paste_moves_clip_to_top: true,
            quick_paste_modifier: "Right Alt".to_string(),
            max_clips: 1000,
            max_age_days: 30,
            tag_prefix: DEFAULT_TAG_PREFIX.to_string(),
            compare_tool_path: String::new(),
            send_to_apps: vec![],
            blacklisted_apps: vec![],
            start_with_system: true,
            always_close_to_tray: true,
            logging_level: "Info".to_string(),
        }
    }
}

impl Settings {
    /// Map the configured `logging_level` string to a tracing `LevelFilter`.
    pub fn log_level_filter(&self) -> tracing_subscriber::filter::LevelFilter {
        match self.logging_level.to_lowercase().as_str() {
            "debug" | "trace" => tracing_subscriber::filter::LevelFilter::DEBUG,
            "info" => tracing_subscriber::filter::LevelFilter::INFO,
            "warn" | "warning" => tracing_subscriber::filter::LevelFilter::WARN,
            "error" => tracing_subscriber::filter::LevelFilter::ERROR,
            _ => tracing_subscriber::filter::LevelFilter::INFO,
        }
    }

    /// Load settings from the given path, falling back to defaults on error.
    /// Corrupted or unreadable files are renamed to a `.bak` before returning
    /// defaults, and the defaults are persisted in the file's place.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            let defaults = Self::default();
            let _ = defaults.save(path);
            return defaults;
        }

        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Race between the exists() check and the read; treat as first launch.
                let defaults = Self::default();
                let _ = defaults.save(path);
                return defaults;
            }
            Err(e) => {
                // Transient I/O error (permissions, filesystem full, etc.).
                // Move the file aside: the defaults returned here are saved on
                // the next settings change, and that save would otherwise
                // overwrite a file that may still be intact.
                tracing::error!("settings: failed to read {:?}: {e}", path);
                let _ = std::fs::rename(path, backup_path(path));
                return Self::default();
            }
        };

        match serde_json::from_str::<Self>(&raw) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("settings: parse error in {:?}: {e}; renaming to .bak", path);
                let _ = std::fs::rename(path, backup_path(path));
                // Persist defaults in the file's place so a crash now still
                // leaves a valid settings file instead of none.
                let defaults = Self::default();
                let _ = defaults.save(path);
                defaults
            }
        }
    }

    /// Atomically write settings: write to .tmp then rename over live file. A
    /// failed write or rename removes the .tmp and is logged: every caller
    /// discards the error, and a silently unsaved setting looks like the app
    /// forgot it.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("tmp");
        let written = std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, path));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            tracing::warn!("settings: cannot save {:?}: {e}", path);
            return Err(e.into());
        }
        Ok(())
    }
}

/// Fixed-name backup for a settings file that cannot be read or parsed. One
/// backup, not history: a `.bak` per corruption would accumulate in the config
/// dir forever.
fn backup_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.bak",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings files written by an older version, or missing a key, must load
    /// with the default for that field instead of failing the whole file (a
    /// parse error renames the file and resets every setting).
    #[test]
    fn missing_fields_get_defaults() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("theme");
        value.as_object_mut().unwrap().remove("hotkey");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.theme, "System");
        assert_eq!(s.hotkey, "Ctrl+Alt+Q");
    }

    /// A corrupt settings file is moved to `settings.json.bak` and defaults are
    /// persisted in its place. A second corruption overwrites that one backup,
    /// so the config dir cannot fill with stale copies.
    #[test]
    fn corrupt_settings_are_backed_up_and_defaults_persisted() {
        let dir =
            std::env::temp_dir().join(format!("cliptoo_settings_corrupt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cliptoo.json");
        std::fs::write(&path, "{ not valid json ").unwrap();

        let s = Settings::load(&path);
        assert_eq!(s.hotkey, "Ctrl+Alt+Q");

        // The corrupt file was moved aside, and defaults written back.
        assert!(dir.join("cliptoo.json.bak").is_file());

        // Corrupting the (now valid) file again replaces the backup rather than
        // adding another one.
        std::fs::write(&path, "{ still not json ").unwrap();
        let _ = Settings::load(&path);

        let backups: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".bak"))
            .collect();
        assert_eq!(backups, ["cliptoo.json.bak"]);

        let persisted = Settings::load(&path);
        assert_eq!(persisted.hotkey, "Ctrl+Alt+Q");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("cliptoo_settings_rt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cliptoo.json");

        let s = Settings {
            hotkey: "Ctrl+Shift+V".to_string(),
            max_clips: 500,
            theme: "Dark".to_string(),
            send_to_apps: vec![SendToApp {
                name: "code".to_string(),
                path: "/usr/bin/code".to_string(),
            }],
            ..Settings::default()
        };
        s.save(&path).unwrap();

        let loaded = Settings::load(&path);
        assert_eq!(loaded.hotkey, "Ctrl+Shift+V");
        assert_eq!(loaded.max_clips, 500);
        assert_eq!(loaded.theme, "Dark");
        assert_eq!(loaded.send_to_apps.len(), 1);
        assert_eq!(loaded.send_to_apps[0].path, "/usr/bin/code");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Settings files written before the newer fields existed must load with
    /// their defaults rather than 0 / empty / false.
    #[test]
    fn legacy_files_get_non_accent_defaults() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("paste_moves_clip_to_top");
        value
            .as_object_mut()
            .unwrap()
            .remove("quick_paste_modifier");
        value
            .as_object_mut()
            .unwrap()
            .remove("always_close_to_tray");
        value
            .as_object_mut()
            .unwrap()
            .remove("settings_window_width");
        value
            .as_object_mut()
            .unwrap()
            .remove("settings_window_height");
        value
            .as_object_mut()
            .unwrap()
            .remove("color_code_clip_types");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!(s.paste_moves_clip_to_top);
        assert_eq!(s.quick_paste_modifier, "Right Alt");
        assert!(s.always_close_to_tray);
        assert!((s.settings_window_width - 560.0).abs() < f64::EPSILON);
        assert_eq!(s.settings_window_height, None);
        assert!(!s.settings_window_height_custom);
        assert!(!s.color_code_clip_types);
    }
}
