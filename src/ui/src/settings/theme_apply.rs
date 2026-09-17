//! Live theme re-application across every window that holds a `Theme` global,
//! and invalidation of cached hover-preview images when the preview size
//! setting changes.

use slint::ComponentHandle;

/// Suffixes of the cached hover-preview images written by `cliptoo_core::image`
/// (see `store_both_thumbnails_for_file`). Used to invalidate them when the
/// preview size setting changes.
const PREVIEW_WEBP_SUFFIX: &str = "_preview.webp";
const PREVIEW_SVG_SUFFIX: &str = "_preview.svg";

/// Update a single `Theme` token on both the main window and the settings
/// window globals. Slint globals are per-window-instance, so live previews
/// need each window's `Theme` global updated.
pub(super) fn apply_theme_to_windows(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    apply: impl Fn(&crate::Theme),
) {
    if let Some(ui) = main_ui.upgrade() {
        apply(&ui.global::<crate::Theme>());
    }
    if let Some(win) = settings_win_ui.upgrade() {
        apply(&win.global::<crate::Theme>());
    }
}

/// Delete cached hover-preview images so the next hover regenerates them at the
/// new `hover_image_preview_size`. The 36px list-cell thumbnails are left
/// alone. Best-effort and off the UI thread; a failure only leaves a stale
/// (possibly upscaled) preview in place until the next cache clear.
pub(super) fn invalidate_image_previews(thumbnails_dir: std::path::PathBuf) {
    std::mem::drop(tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&thumbnails_dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(PREVIEW_WEBP_SUFFIX) && !name.ends_with(PREVIEW_SVG_SUFFIX) {
                continue;
            }
            if let Err(e) = std::fs::remove_file(entry.path()) {
                tracing::warn!("invalidate_image_previews: {:?}: {e}", entry.path());
            }
        }
    }));
}

/// Re-resolve the theme from `settings` and fill every window's `Theme` global,
/// including the registered tray/About/preview fillers. Runs off the UI thread
/// for the portal/settings lookup, then hops back to apply the tokens.
pub(super) fn reapply_theme(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    settings: &cliptoo_core::Settings,
    favicons_dir: std::path::PathBuf,
    fillers: crate::theme::ThemeFillers,
) {
    let main_weak = main_ui.clone();
    let settings_weak = settings_win_ui.clone();
    let s_snap = settings.clone();
    tokio::spawn(async move {
        let prev_dark = crate::theme::cached_resolved_theme().0;
        let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
        let _ = main_weak.upgrade_in_event_loop(move |ui| {
            crate::theme::fill_theme(
                &ui.global::<crate::Theme>(),
                &s_snap,
                is_dark,
                system_accent,
            );
            if is_dark != prev_dark {
                // The visible list rows cache decoded favicon images keyed by
                // theme variant, so a light→dark (or vice-versa) switch must
                // reload them to avoid showing an invisible icon.
                crate::thumbnail_cache::reload_favicons(&ui, &favicons_dir);
            }
            if let Some(win) = settings_weak.upgrade() {
                crate::theme::fill_theme(
                    &win.global::<crate::Theme>(),
                    &s_snap,
                    is_dark,
                    system_accent,
                );
            }
            // The tray and About window hold their own `Theme` globals; they
            // register here and are re-filled with the same resolved values.
            crate::theme::apply_theme_fillers(&fillers, &s_snap, is_dark, system_accent);
        });
    });
}
