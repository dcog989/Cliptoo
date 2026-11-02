//! Live theme re-application across every window that holds a `Theme` global,
//! invalidation of cached hover-preview images when the preview size setting
//! changes, and the portal listener that follows OS appearance changes.

use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;
use slint::ComponentHandle;

/// Bumped for every theme re-resolve. A slower, older resolve must not repaint
/// the windows after a newer one has already been requested.
static THEME_REAPPLY_REQUEST: AtomicU64 = AtomicU64::new(0);

/// Suffixes of the cached hover-preview images written by `cliptoo_core::image`
/// (see `store_both_thumbnails_for_file`). Used to invalidate them when the
/// preview size setting changes.
const PREVIEW_WEBP_SUFFIX: &str = "_preview.webp";
const PREVIEW_SVG_SUFFIX: &str = "_preview.svg";

/// Update a single `Theme` token on every window that shows it: the main
/// window, the settings window and the hover-preview window. Slint globals are
/// per-window-instance, so live previews need each window's `Theme` global
/// updated.
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
    crate::preview::apply_theme(&apply);
}

/// Fan a theme change out to the main window, the settings window and every
/// registered filler (tray, About, preview). Slint globals are per-window
/// instance, so each one has to be filled separately.
#[allow(clippy::too_many_arguments)]
pub(super) fn fill_every_window(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    settings: &cliptoo_core::Settings,
    is_dark: bool,
    system_accent: Option<(u8, u8, u8)>,
    fillers: &crate::theme::ThemeFillers,
    fill: impl Fn(&crate::Theme),
) {
    if let Some(ui) = main_ui.upgrade() {
        fill(&ui.global::<crate::Theme>());
    }
    if let Some(win) = settings_win_ui.upgrade() {
        fill(&win.global::<crate::Theme>());
    }
    crate::theme::apply_theme_fillers(fillers, settings, is_dark, system_accent);
}

/// Delete the cached hover-preview images under `thumbnails_dir`, leaving the
/// 36px list-cell thumbnails and unrelated files alone.
fn remove_preview_files(thumbnails_dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(thumbnails_dir) else {
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
}

/// Delete cached hover-preview images so the next hover regenerates them at the
/// new `hover_image_preview_size`. Best-effort and off the UI thread; a failure
/// only leaves a stale (possibly upscaled) preview until the next cache clear.
pub(super) fn invalidate_image_previews(thumbnails_dir: std::path::PathBuf) {
    std::mem::drop(tokio::task::spawn_blocking(move || {
        remove_preview_files(&thumbnails_dir);
    }));
}

/// Re-resolve the theme from `settings` and fill every window's `Theme` global,
/// including the registered About/preview fillers. Runs off the UI thread for
/// the portal/settings lookup, then hops back to apply the tokens.
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
    let request = THEME_REAPPLY_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
    tokio::spawn(async move {
        let prev_dark = crate::theme::cached_resolved_theme().0;
        let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
        let _ = main_weak.upgrade_in_event_loop(move |ui| {
            // A newer change replaced this resolve while it was in flight; its
            // result is the one to keep.
            if THEME_REAPPLY_REQUEST.load(Ordering::Relaxed) != request {
                return;
            }
            // Only the newest resolve writes the cache, so a slow older lookup
            // cannot leave a stale theme behind for the editor to read.
            crate::theme::cache_resolved_theme(is_dark, system_accent);
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
            // The About and preview windows hold their own `Theme` globals; they
            // register here and are re-filled with the same resolved values.
            crate::theme::apply_theme_fillers(&fillers, &s_snap, is_dark, system_accent);
        });
    });
}

/// Re-resolve the theme when the desktop portal reports an appearance change.
/// The portal emits `SettingChanged` for both the colour scheme and the accent;
/// a re-resolve reads the current values, so the signal body is not needed.
pub(crate) fn watch_system_theme(
    main_ui: slint::Weak<crate::AppWindow>,
    settings_win: slint::Weak<crate::SettingsWindow>,
    settings: std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    favicons_dir: std::path::PathBuf,
    fillers: crate::theme::ThemeFillers,
) {
    let result = slint::spawn_local(async move {
        let Ok(conn) = crate::dbus::session().await else {
            return;
        };
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.portal.Desktop")
            .and_then(|b| b.interface("org.freedesktop.portal.Settings"))
            .and_then(|b| b.member("SettingChanged"))
            .and_then(|b| b.path("/org/freedesktop/portal/desktop"))
            .and_then(|b| b.add_arg("org.freedesktop.appearance"))
            .map(|b| b.build());
        let Ok(rule) = rule else {
            return;
        };
        let Ok(mut stream) = zbus::MessageStream::for_match_rule(rule, &conn, Some(1)).await else {
            return;
        };
        while stream.next().await.is_some() {
            let snapshot = settings.borrow().clone();
            // A custom accent with an explicit light/dark mode ignores OS
            // appearance changes; re-resolving would only repeat the same work.
            if snapshot.theme == "System" || snapshot.accent_color.trim().is_empty() {
                reapply_theme(
                    &main_ui,
                    &settings_win,
                    &snapshot,
                    favicons_dir.clone(),
                    fillers.clone(),
                );
            }
        }
    });
    if let Err(e) = result {
        tracing::warn!("system theme watcher: failed to schedule on the event loop: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::remove_preview_files;

    #[test]
    fn preview_invalidation_keeps_list_thumbnails_and_other_files() {
        let dir =
            std::env::temp_dir().join(format!("cliptoo_preview_invalidate_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "abc.webp",
            "abc_preview.webp",
            "abc_preview.svg",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }

        remove_preview_files(&dir);

        assert!(dir.join("abc.webp").is_file(), "list thumbnail must stay");
        assert!(dir.join("notes.txt").is_file(), "unrelated file must stay");
        assert!(!dir.join("abc_preview.webp").exists());
        assert!(!dir.join("abc_preview.svg").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
