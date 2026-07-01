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
    pub preview_hotkey: String,
    pub quick_paste_hotkey: String,

    // Window
    pub launch_position: String,
    pub fixed_x: i32,
    pub fixed_y: i32,
    pub window_width: f64,
    pub window_height: f64,
    pub editor_window_width: f64,
    pub editor_window_height: f64,

    // Theming
    pub accent_hue: f64,
    pub accent_chroma_level: String,
    pub accent_color: String,
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
    pub logging_level: String,
    pub log_retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            hotkey: "Ctrl+Alt+Q".to_string(),
            preview_hotkey: "Ctrl+Alt+P".to_string(),
            quick_paste_hotkey: "Alt".to_string(),
            launch_position: "Cursor".to_string(),
            fixed_x: 0,
            fixed_y: 0,
            window_width: 460.0,
            window_height: 600.0,
            editor_window_width: 520.0,
            editor_window_height: 420.0,
            accent_hue: 250.0,
            accent_chroma_level: "Mellow".to_string(),
            accent_color: "#7C6EE6".to_string(),
            theme: "System".to_string(),
            font_family: "Inter".to_string(),
            font_size: 13.0,
            preview_font_size: 12.0,
            clip_item_padding: "Standard".to_string(),
            hover_preview_delay: 400,
            hover_image_preview_size: 300,
            paste_as_plain_text: false,
            max_clips: 10000,
            max_age_days: 90,
            tag_prefix: "##".to_string(),
            compare_tool_path: String::new(),
            send_to_apps: vec![],
            blacklisted_apps: vec![],
            start_with_system: true,
            logging_level: "Info".to_string(),
            log_retention_days: 14,
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

fn chrono_now_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
