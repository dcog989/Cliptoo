use anyhow::{Context, Result};
use std::path::PathBuf;

/// Resolved application directory paths.
///
///   config/state  (~/.config/Cliptoo/)   — settings, DB, logs
///   cache/transient (~/.local/share/Cliptoo/) — images, thumbnails, icons
pub struct AppDirs {
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub db_path: PathBuf,
    pub settings_path: PathBuf,
    pub images_dir: PathBuf,
    pub thumbnails_dir: PathBuf,
    pub favicons_dir: PathBuf,
}

impl AppDirs {
    /// Resolve XDG base directories via the `dirs` crate.
    pub fn resolve() -> Result<Self> {
        let config_home = dirs::config_dir().context("no config dir (unset HOME?)")?;
        let data_home = dirs::data_dir().context("no data dir (unset HOME?)")?;

        // Config/state — settings, DB, logs
        let config_dir = config_home.join("Cliptoo");
        let logs_dir = config_dir.join("Logs");

        // Cache/transient — images, thumbnails, icons
        let cache_dir = data_home.join("Cliptoo");
        let tmp_dir = std::env::temp_dir().join("Cliptoo");

        let images_dir = cache_dir.join("images");
        let thumbnails_dir = cache_dir.join("thumbnails");
        let favicons_dir = cache_dir.join("favicons");
        let icons_cache_dir = cache_dir.join("icons");

        // Create all directories on first run
        for dir in &[
            &config_dir,
            &cache_dir,
            &logs_dir,
            &tmp_dir,
            &images_dir,
            &thumbnails_dir,
            &favicons_dir,
            &icons_cache_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }

        Ok(Self {
            db_path: config_dir.join("clips.db"),
            settings_path: config_dir.join("settings.json"),
            data_dir: config_dir,
            logs_dir,
            images_dir,
            thumbnails_dir,
            favicons_dir,
        })
    }
}
