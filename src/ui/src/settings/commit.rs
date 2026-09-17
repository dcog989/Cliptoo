//! Persist setting changes from the settings window and apply their live
//! effects (theme, fonts, hotkey, retention, blacklist).

use slint::ComponentHandle;

use super::parse::{clean_hotkey_text, parse_blacklist, parse_send_to_apps};
use super::theme_apply::{apply_theme_to_windows, invalidate_image_previews, reapply_theme};

/// Handle setting changes: persist each key/value into `Settings` and re-apply
/// live effects (theme, fonts, hotkeys).
#[allow(clippy::too_many_arguments)]
pub(super) fn setup_setting_commit(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
    favicons_dir: std::path::PathBuf,
    thumbnails_dir: std::path::PathBuf,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    image_preview_size: std::sync::Arc<std::sync::atomic::AtomicU32>,
    fillers: crate::theme::ThemeFillers,
) {
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    let settings_ui = main_ui.as_weak();
    let sw = settings_win.as_weak();
    let fillers = fillers.clone();
    let image_preview_size = image_preview_size.clone();
    let thumbnails_dir = thumbnails_dir.clone();
    settings_win.on_setting_changed(
        move |key: slint::SharedString, value: slint::SharedString| {
            let key = key.to_string();
            let value = value.to_string();
            let mut s = s.borrow_mut();

            match key.as_str() {
                "hotkey" => {
                    // No trim: clean_hotkey_text keeps a trailing `+` as the
                    // plus key (Ctrl++ = Ctrl + the plus key) and drops a
                    // dangling separator itself.
                    let cleaned = clean_hotkey_text(&value);
                    s.hotkey = cleaned.clone();
                    if let Some(win) = sw.upgrade() {
                        win.set_s_hotkey(cleaned.into());
                    }
                    let _ = hotkey_tx.send(s.hotkey.clone());
                }
                "start_with_system" => {
                    let enabled = value == "true";
                    s.start_with_system = enabled;
                    if enabled {
                        let _ = crate::autostart::ensure_autostart();
                    } else {
                        let _ = crate::autostart::remove_autostart();
                    }
                }
                "always_close_to_tray" => s.always_close_to_tray = value == "true",
                "quick_paste_modifier" => {
                    s.quick_paste_modifier = value.clone();
                    if let Some(ui) = settings_ui.upgrade() {
                        ui.set_quick_paste_mod(value.clone().into());
                    }
                }
                "logging_level" => {
                    s.logging_level = value.clone();
                    // Apply live: the file logger reads the level from an
                    // atomic, so the new level is in effect immediately.
                    cliptoo_core::logger::set_level(s.log_level_filter());
                }
                "theme" => {
                    s.theme = value.clone();
                    reapply_theme(&settings_ui, &sw, &s, favicons_dir.clone(), fillers.clone());
                }
                "font_family" => {
                    s.font_family = value.clone();
                    apply_theme_to_windows(&settings_ui, &sw, |t| {
                        t.set_font_family(value.as_str().into());
                    });
                }
                "font_size" => {
                    if let Ok(v) = value.parse::<f64>() {
                        s.font_size = v;
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_clip_list_font_size(v as f32)
                        });
                    }
                }
                "preview_font_size" => {
                    if let Ok(v) = value.parse::<f64>() {
                        s.preview_font_size = v;
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_preview_font_size(v as f32);
                        });
                    }
                }
                "clip_item_padding" => {
                    s.clip_item_padding = value.clone();
                    apply_theme_to_windows(&settings_ui, &sw, |t| {
                        t.set_row_height(crate::positioning::row_height(value.as_str()) as f32);
                    });
                }
                "hover_preview_delay" => {
                    if let Ok(ms) = value.parse::<u32>() {
                        s.hover_preview_delay = ms;
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_hover_preview_delay(ms as i64);
                        });
                    }
                }
                "hover_image_preview_size" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.hover_image_preview_size = v;
                        // The clipboard listener reads this shared value per
                        // thumbnail generation, and the preview popup reads the
                        // Theme token, so the change applies live.
                        image_preview_size.store(v, std::sync::atomic::Ordering::Relaxed);
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_preview_size(v as f32);
                        });
                        // The preview window has its own per-window Theme global
                        // that apply_theme_to_windows does not reach.
                        crate::preview::set_preview_size(v as f32);
                        // Drop cached hover previews so the next hover rebuilds
                        // them at the new size instead of upscaling the old one.
                        invalidate_image_previews(thumbnails_dir.clone());
                    }
                }
                "paste_as_plain_text" => s.paste_as_plain_text = value == "true",
                "paste_moves_to_top" => s.paste_moves_clip_to_top = value == "true",
                "compare_tool_path" => s.compare_tool_path = value.clone(),
                "send_to_apps" => {
                    s.send_to_apps = parse_send_to_apps(&value);
                    // Rebuild the context-menu Send To list so the change
                    // applies without a restart.
                    if let Some(ui) = settings_ui.upgrade() {
                        let names: Vec<slint::SharedString> = s
                            .send_to_apps
                            .iter()
                            .map(|a| slint::SharedString::from(a.name.as_str()))
                            .collect();
                        ui.set_ctx_send_to_apps(
                            std::rc::Rc::new(slint::VecModel::from(names)).into(),
                        );
                    }
                }
                "blacklisted_apps" => {
                    s.blacklisted_apps = parse_blacklist(&value);
                    // The clipboard listener reads the blacklist from this
                    // shared state on every poll, so the change applies
                    // without a restart.
                    *blacklist_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        s.blacklisted_apps.clone();
                }
                "max_clips" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.max_clips = v;
                    }
                }
                "max_age_days" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.max_age_days = v;
                    }
                }
                _ => {}
            }

            let _ = s.save(&p);

            // Publish retention changes so the scheduled maintenance task picks
            // them up on its next pass without a restart.
            if matches!(key.as_str(), "max_clips" | "max_age_days") {
                let _ = retention_tx.send(cliptoo_core::maintenance::RetentionConfig {
                    max_clips: s.max_clips,
                    max_age_days: s.max_age_days,
                });
            }
        },
    );
}
