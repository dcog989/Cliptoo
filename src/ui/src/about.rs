// about.rs — About window: logo, tagline, and info table with clickable paths.
use slint::{ComponentHandle, SharedString};
use std::time::Duration;

/// Register the About open handler. The window itself is built on the first
/// open, not at startup.
pub fn register_about(
    ui: &crate::AppWindow,
    slot: &crate::lazy_window::LazyWindow<crate::AboutWindow>,
    dirs: crate::app_dirs::AppDirs,
    settings: std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    theme_fillers: crate::theme::ThemeFillers,
) {
    let weak_ui = ui.as_weak();
    let slot = slot.clone();
    ui.on_menu_about(move || {
        let Some(ui) = weak_ui.upgrade() else {
            return;
        };
        let win = slot.get_or_create(|| create_about_window(&ui, &dirs, &settings, &theme_fillers));
        present_about(&win, &ui);
    });
}

/// Show the About window from the hamburger menu.
fn present_about(about_win: &crate::AboutWindow, main_ui: &crate::AppWindow) {
    main_ui.set_about_open(true);
    // preferred-* alone doesn't size the window reliably on Wayland/Qt (same
    // reason settings.rs uses set_size), so force the size measured from the
    // content, paths included.
    let w = about_win.get_content_width().max(340.0);
    let h = about_win.get_content_height().max(280.0);
    about_win.window().set_size(slint::LogicalSize {
        width: w,
        height: h,
    });
    let _ = about_win.show();
    // Qt's show() doesn't raise; after the hamburger popup closes the About
    // window can land under the main window. Defer the raise so the popup has
    // time to recede.
    let aw = about_win.as_weak();
    slint::Timer::single_shot(Duration::from_millis(100), move || {
        if let Some(win) = aw.upgrade() {
            crate::drag::activate_window(&win);
        }
    });
}

fn create_about_window(
    ui: &crate::AppWindow,
    dirs: &crate::app_dirs::AppDirs,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    theme_fillers: &crate::theme::ThemeFillers,
) -> crate::AboutWindow {
    let about_win = crate::AboutWindow::new().expect("AboutWindow creation");

    // A lazily created window missed the startup theme fill.
    let (is_dark, system_accent) = crate::theme::cached_resolved_theme();
    crate::theme::fill_theme(
        &about_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );
    crate::theme::register_about_theme_filler(theme_fillers, &about_win.as_weak());

    about_win.set_version(env!("CARGO_PKG_VERSION").into());
    about_win.set_install_path(install_dir().into());
    about_win.set_config_path(dirs.config_dir.display().to_string().into());
    about_win.set_data_path(dirs.data_dir.display().to_string().into());
    about_win.set_cache_path(dirs.cache_dir.display().to_string().into());
    about_win.set_logs_folder_path(dirs.logs_dir.display().to_string().into());
    about_win.set_log_level(
        crate::settings::LOG_LEVELS[crate::settings::idx_of(
            &settings.borrow().logging_level,
            &crate::settings::LOG_LEVELS,
        ) as usize]
            .into(),
    );

    // Log level lives here rather than in Settings. A step wraps around, is
    // applied live (the file logger reads an atomic), and is saved at once.
    {
        let aw = about_win.as_weak();
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        about_win.on_log_level_step(move |delta: i32| {
            let mut s = s.borrow_mut();
            let count = crate::settings::LOG_LEVELS.len() as i32;
            let current = crate::settings::idx_of(&s.logging_level, &crate::settings::LOG_LEVELS);
            let next = (current + delta).rem_euclid(count);
            let level = crate::settings::LOG_LEVELS[next as usize];
            s.logging_level = level.to_string();
            cliptoo_core::logger::set_level(s.log_level_filter());
            let _ = s.save(&p);
            if let Some(win) = aw.upgrade() {
                win.set_log_level(level.into());
            }
        });
    }

    // Open the newest log file in the default viewer.
    {
        let logs_dir = dirs.logs_dir.clone();
        about_win.on_open_latest_log(move || {
            match cliptoo_core::logger::latest_log_path(&logs_dir) {
                Some(latest) => crate::helpers::open_with_default(&latest.to_string_lossy()),
                None => tracing::warn!("open-log: no log file yet in {}", logs_dir.display()),
            }
        });
    }

    // Open the project repository in the default browser.
    about_win.on_open_repo(|| {
        crate::helpers::open_with_default(env!("CARGO_PKG_REPOSITORY"));
    });

    // Check GitHub for a newer release. Once one is found the same button
    // opens its release page, so a second click never re-queries.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        let aw = about_win.as_weak();
        let checking = Arc::new(AtomicBool::new(false));
        let release_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        about_win.on_update_action(move || {
            if let Some(url) = release_url.lock().ok().and_then(|url| url.clone()) {
                crate::helpers::open_with_default(&url);
                return;
            }
            if checking.swap(true, Ordering::Relaxed) {
                return;
            }
            let Some(win) = aw.upgrade() else { return };
            win.set_update_label(win.global::<crate::Translations>().get_update_checking());
            let weak = aw.clone();
            let checking = checking.clone();
            let release_url = release_url.clone();
            tokio::spawn(async move {
                let latest = fetch_latest_tag().await;
                checking.store(false, Ordering::Relaxed);
                let _ = weak.upgrade_in_event_loop(move |win| {
                    let translations = win.global::<crate::Translations>();
                    match latest {
                        Some(tag) if is_newer_tag(&tag, env!("CARGO_PKG_VERSION")) => {
                            let url = release_page_url(&tag);
                            win.set_update_label(
                                translations
                                    .invoke_update_to_version(tag.trim_start_matches('v').into()),
                            );
                            if let Ok(mut slot) = release_url.lock() {
                                *slot = Some(url);
                            }
                        }
                        Some(_) => {
                            win.set_update_label(translations.get_up_to_date());
                        }
                        None => {
                            win.set_update_label(translations.get_update_failed());
                        }
                    }
                });
            });
        });
    }

    // Open a path (install/data/cache/logs dir) in the default file manager.
    about_win.on_open_folder(|path: SharedString| {
        crate::helpers::open_with_default(path.as_str());
    });

    // Opening the About window must not trigger blur-to-tray on the main
    // window, so guard it with the same mechanism as the settings window.
    {
        let main_ui = ui.as_weak();
        about_win.window().on_close_requested(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_about_open(false);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    {
        let main_ui = ui.as_weak();
        about_win.on_about_closing(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_about_open(false);
            }
        });
    }

    about_win
}

/// Directory containing the running executable (the "Install" path).
fn install_dir() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.display().to_string()))
        .unwrap_or_else(|| String::from("?"))
}

/// GitHub's latest-release endpoint. The User-Agent is mandatory there.
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/dcog989/Cliptoo/releases/latest";
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// The latest release's tag, or None when the network or the API failed.
async fn fetch_latest_tag() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(UPDATE_CHECK_TIMEOUT)
        .user_agent(crate::helpers::USER_AGENT)
        .build()
        .ok()?;
    let body = client
        .get(LATEST_RELEASE_API)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    tag_name(&body)
}

/// Pull `"tag_name":"v2.24.0"` out of the release JSON. serde_json is not a
/// dependency of this crate and one field does not justify adding it.
fn tag_name(body: &str) -> Option<String> {
    const KEY: &str = "\"tag_name\":\"";
    let start = body.find(KEY)? + KEY.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn version_parts(v: &str) -> Vec<u64> {
    // Drop a leading `v` and any pre-release/build suffix ("2.24.0-rc1").
    v.trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Numeric compare, so `v2.24` does not count as newer than `2.24.0`.
fn is_newer_tag(latest: &str, current: &str) -> bool {
    let mut latest = version_parts(latest);
    let mut current = version_parts(current);
    let len = latest.len().max(current.len());
    latest.resize(len, 0);
    current.resize(len, 0);
    latest > current
}

fn release_page_url(tag: &str) -> String {
    format!("{}/releases/tag/{tag}", env!("CARGO_PKG_REPOSITORY"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_name_and_newer_tag() {
        let body = r#"{"url":"x","tag_name":"v2.24.0","html_url":"y"}"#;
        assert_eq!(tag_name(body).as_deref(), Some("v2.24.0"));
        assert_eq!(tag_name("{}"), None);

        assert!(is_newer_tag("v2.24.0", "2.23.0"));
        assert!(is_newer_tag("v2.23.1", "2.23.0"));
        assert!(!is_newer_tag("v2.23.0", "2.23.0"));
        assert!(!is_newer_tag("v2.23", "2.23.0"));
        assert!(!is_newer_tag("v2.22.9", "2.23.0"));
        assert!(is_newer_tag("v3.0.0-rc1", "2.23.0"));
    }
}
