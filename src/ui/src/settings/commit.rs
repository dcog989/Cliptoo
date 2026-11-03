//! Persist setting changes from the settings window and apply their live
//! effects (theme, fonts, hotkey, retention, blacklist).

use std::time::Duration;

use slint::ComponentHandle;

use super::parse::{clean_hotkey_text, parse_blacklist, parse_send_to_apps};
use super::theme_apply::{apply_theme_to_windows, invalidate_image_previews, reapply_theme};

/// Delay between the last edit and the settings write. Text fields commit on
/// every keystroke; writing the whole file in the callback stalled the UI.
const SETTINGS_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Quiet period before the preview cache is dropped, so holding the size
/// stepper does not walk the thumbnail directory once per step.
const PREVIEW_INVALIDATE_DEBOUNCE: Duration = Duration::from_millis(500);

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
    db: &std::sync::Arc<cliptoo_core::db::DbPool>,
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
    let reapply_db = db.clone();
    let save_timer = std::rc::Rc::new(slint::Timer::default());
    let invalidate_timer = std::rc::Rc::new(slint::Timer::default());
    settings_win.on_setting_changed(
        move |key: slint::SharedString, value: slint::SharedString| {
            let key = key.to_string();
            let value = value.to_string();
            let s_saved = s.clone();
            let p_saved = p.clone();
            let save_timer = save_timer.clone();
            let mut s = s.borrow_mut();

            match key.as_str() {
                "hotkey" => {
                    // No trim: clean_hotkey_text keeps a trailing `+` as the
                    // plus key (Ctrl++ = Ctrl + the plus key) and drops a
                    // dangling separator itself.
                    let cleaned = clean_hotkey_text(&value);
                    s.hotkey = cleaned.clone();
                    if let Some(win) = sw.upgrade() {
                        win.global::<crate::SettingsValues>()
                            .set_s_hotkey(cleaned.into());
                    }
                    let _ = hotkey_tx.send(s.hotkey.clone());
                }
                "start_with_system" => {
                    let enabled = value == "true";
                    s.start_with_system = enabled;
                    let result = if enabled {
                        crate::autostart::ensure_autostart()
                    } else {
                        crate::autostart::remove_autostart()
                    };
                    if let Err(e) = result {
                        // Do not keep a setting that does not match the disk:
                        // the next launch would still not autostart.
                        s.start_with_system = !enabled;
                        if let Some(win) = sw.upgrade() {
                            win.global::<crate::SettingsValues>()
                                .set_s_start_with_system(!enabled);
                        }
                        tracing::warn!("autostart: failed to update the desktop file: {e}");
                        super::show_toast(&sw, "Could not update autostart", "error");
                    }
                }
                "always_close_to_tray" => s.always_close_to_tray = value == "true",
                "quick_paste_modifier" => {
                    s.quick_paste_modifier = value.clone();
                    if let Some(ui) = settings_ui.upgrade() {
                        ui.set_quick_paste_mod(value.clone().into());
                    }
                }
                "theme" => {
                    s.theme = value.clone();
                    reapply_theme(
                        &settings_ui,
                        &sw,
                        &s,
                        favicons_dir.clone(),
                        &reapply_db,
                        fillers.clone(),
                    );
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
                        // thumbnail generation, and the preview window's Theme
                        // token is updated by apply_theme_to_windows.
                        image_preview_size.store(v, std::sync::atomic::Ordering::Relaxed);
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_preview_size(v as f32);
                        });
                        // Drop cached hover previews once the stepper settles,
                        // so holding it does not rescan the cache per step.
                        let td = thumbnails_dir.clone();
                        let timer = invalidate_timer.clone();
                        timer.start(
                            slint::TimerMode::SingleShot,
                            PREVIEW_INVALIDATE_DEBOUNCE,
                            move || invalidate_image_previews(td.clone()),
                        );
                    }
                }
                "color_code_clip_types" => {
                    let enabled = value == "true";
                    s.color_code_clip_types = enabled;
                    // The mapping itself lives in Slint (ClipTypeMap.color-for);
                    // only the on/off token needs re-applying here.
                    apply_theme_to_windows(&settings_ui, &sw, |t| {
                        t.set_color_code_clip_types(enabled);
                    });
                }
                "paste_as_plain_text" => s.paste_as_plain_text = value == "true",
                "paste_moves_to_top" => s.paste_moves_clip_to_top = value == "true",
                "compare_tool_path" => s.compare_tool_path = value.clone(),
                "send_to_apps" => s.send_to_apps = parse_send_to_apps(&value),
                "blacklisted_apps" => s.blacklisted_apps = parse_blacklist(&value),
                "max_clips" => {
                    if let Ok(v) = value.parse::<u32>() {
                        // Retention ceiling the settings page offers; the clip
                        // list pages through whatever is kept.
                        s.max_clips = v.min(cliptoo_core::db::queries::SEARCH_RESULT_LIMIT as u32);
                    }
                }
                "max_age_days" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.max_age_days = v;
                    }
                }
                _ => {}
            }

            // Publish retention changes so the scheduled maintenance task picks
            // them up on its next pass without a restart.
            if matches!(key.as_str(), "max_clips" | "max_age_days") {
                let _ = retention_tx.send(cliptoo_core::maintenance::RetentionConfig {
                    max_clips: s.max_clips,
                    max_age_days: s.max_age_days,
                });
            }

            // Coalesce text-field edits: a write per keystroke serialized the
            // whole file and blocked the UI thread. Closing the window saves
            // synchronously through the close handlers.
            drop(s);
            let live_ui = settings_ui.clone();
            let live_blacklist = blacklist_state.clone();
            save_timer.start(
                slint::TimerMode::SingleShot,
                SETTINGS_SAVE_DEBOUNCE,
                move || {
                    let snapshot = s_saved.borrow();
                    let _ = snapshot.save(&p_saved);
                    // Apply the text-field side effects only after the edits
                    // settle: per keystroke, a half-typed list was pushed to
                    // the context menu and the live app blacklist.
                    if let Some(ui) = live_ui.upgrade() {
                        let names: Vec<slint::SharedString> = snapshot
                            .send_to_apps
                            .iter()
                            .map(|a| slint::SharedString::from(a.name.as_str()))
                            .collect();
                        ui.set_ctx_send_to_apps(
                            std::rc::Rc::new(slint::VecModel::from(names)).into(),
                        );
                    }
                    *live_blacklist
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        snapshot.blacklisted_apps.clone();
                },
            );
        },
    );
}
