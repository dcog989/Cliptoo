// about.rs — About window: logo, tagline, and info table with clickable paths.
use slint::{ComponentHandle, SharedString};
use std::time::Duration;

pub fn setup_about_window(
    ui: &crate::AppWindow,
    dirs: &crate::app_dirs::AppDirs,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
) -> crate::AboutWindow {
    let about_win = crate::AboutWindow::new().expect("AboutWindow creation");
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

    // Show the About window from the hamburger menu.
    {
        let aw = about_win.as_weak();
        let main_ui = ui.as_weak();
        ui.on_menu_about(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_about_open(true);
            }
            if let Some(win) = aw.upgrade() {
                // preferred-* alone doesn't size the window reliably on
                // Wayland/Qt (same reason settings.rs uses set_size), so force
                // the size measured from the content, paths included.
                let w = win.get_content_width().max(340.0);
                let h = win.get_content_height().max(280.0);
                win.window().set_size(slint::LogicalSize {
                    width: w,
                    height: h,
                });
                let _ = win.show();
                // Qt's show() doesn't raise; after the hamburger popup closes
                // the About window can land under the main window.  Defer the
                // raise so the popup has time to recede.
                let aw = aw.clone();
                slint::Timer::single_shot(Duration::from_millis(100), move || {
                    if let Some(win) = aw.upgrade() {
                        crate::drag::activate_window(&win);
                    }
                });
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
