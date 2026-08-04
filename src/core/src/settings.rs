use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendToApp {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,

    // Hotkeys
    pub hotkey: String,

    // Window
    pub window_width: f64,
    pub window_height: f64,
    pub editor_window_width: f64,
    pub editor_window_height: f64,
    // Settings window size, persisted so user resizes survive restarts.
    #[serde(default = "default_settings_window_width")]
    pub settings_window_width: f64,
    #[serde(default = "default_settings_window_height")]
    pub settings_window_height: f64,

    // Theming
    // accent_color is a "#RRGGBB" string; empty means "use the OS accent".
    // Older settings files stored accent_hue + accent_chroma_level, which are
    // no longer read.
    #[serde(default = "default_accent_color")]
    pub accent_color: String,
    // Accent tuning: the accent color is fully described by a hue (degrees,
    // 0–360) plus a saturation and brightness (HSV value, both 0.0–1.0); the
    // stored accent_color hex is derived from these by the settings UI. Hue
    // is kept separately so the settings hue slider keeps its position while
    // "Clear" (OS accent) is active and there is no custom hex to read it back
    // from. The defaults match the fallback accent (#7C6EE6, hue 247°).
    #[serde(default = "default_accent_hue")]
    pub accent_hue: f64,
    #[serde(default = "default_accent_saturation")]
    pub accent_saturation: f64,
    #[serde(default = "default_accent_value")]
    pub accent_value: f64,
    pub theme: String,

    // Typography / layout
    pub font_family: String,
    pub font_size: f64,
    pub preview_font_size: f64,
    pub clip_item_padding: String,

    // Behaviour
    pub hover_preview_delay: u32,
    pub hover_image_preview_size: u32,
    pub paste_as_plain_text: bool,
    // Modifier key that activates the quick-paste overlay:
    // "Right Alt" | "Left Alt" | "Control".
    #[serde(default = "default_quick_paste_modifier")]
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
    #[serde(default = "default_always_close_to_tray")]
    pub always_close_to_tray: bool,
    pub logging_level: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            hotkey: "Ctrl+Alt+Q".to_string(),
            window_width: 460.0,
            window_height: 600.0,
            editor_window_width: 520.0,
            editor_window_height: 420.0,
            settings_window_width: 560.0,
            settings_window_height: 540.0,
            accent_color: String::new(),
            accent_hue: 247.0,
            accent_saturation: 0.65,
            accent_value: 0.95,
            theme: "System".to_string(),
            font_family: "Inter".to_string(),
            font_size: 13.0,
            preview_font_size: 12.0,
            clip_item_padding: "Standard".to_string(),
            hover_preview_delay: 400,
            hover_image_preview_size: 300,
            paste_as_plain_text: false,
            quick_paste_modifier: "Right Alt".to_string(),
            max_clips: 1000,
            max_age_days: 30,
            tag_prefix: "##".to_string(),
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
    /// Corrupted files are renamed to a timestamped .bak before returning defaults.
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
                // Return defaults but do NOT overwrite the file — it may be intact.
                tracing::error!("settings: failed to read {:?}: {e}", path);
                return Self::default();
            }
        };

        match serde_json::from_str::<Self>(&raw) {
            Ok(s) => s,
            Err(e) => {
                // Rename corrupt file to .json.bak.{timestamp}
                tracing::warn!("settings: parse error in {:?}: {e}; renaming to .bak", path);
                let bak_name = format!(
                    "{}.bak.{}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    chrono_now_compact()
                );
                let bak = path.with_file_name(bak_name);
                let _ = std::fs::rename(path, &bak);
                Self::default()
            }
        }
    }

    /// Atomically write settings: write to .tmp then rename over live file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

// Serde `default =` hooks. Each delegates to `Settings::default()` so the
// `Default` impl is the single source of truth for every default value.

fn default_accent_color() -> String {
    Settings::default().accent_color
}

fn default_accent_hue() -> f64 {
    Settings::default().accent_hue
}

fn default_accent_saturation() -> f64 {
    Settings::default().accent_saturation
}

fn default_accent_value() -> f64 {
    Settings::default().accent_value
}

fn default_always_close_to_tray() -> bool {
    Settings::default().always_close_to_tray
}

fn default_quick_paste_modifier() -> String {
    Settings::default().quick_paste_modifier
}

fn default_settings_window_width() -> f64 {
    Settings::default().settings_window_width
}

fn default_settings_window_height() -> f64 {
    Settings::default().settings_window_height
}

fn chrono_now_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings files written before the accent tuning fields existed must
    /// load with the tuning defaults, not 0.0 (which would make the accent
    /// black / grayscale).
    #[test]
    fn accent_tuning_defaults_for_legacy_files() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("accent_hue");
        value.as_object_mut().unwrap().remove("accent_saturation");
        value.as_object_mut().unwrap().remove("accent_value");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!((s.accent_hue - 247.0).abs() < f64::EPSILON);
        assert!((s.accent_saturation - 0.65).abs() < f64::EPSILON);
        assert!((s.accent_value - 0.95).abs() < f64::EPSILON);
    }
}
