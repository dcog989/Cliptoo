// about.rs — About window: logo, tagline, and info table with clickable paths.
use slint::{ComponentHandle, SharedString};
use std::time::Duration;

pub fn setup_about_window(
    ui: &crate::AppWindow,
    dirs: &crate::app_dirs::AppDirs,
) -> crate::AboutWindow {
    let about_win = crate::AboutWindow::new().expect("AboutWindow creation");
    about_win.set_version(env!("CARGO_PKG_VERSION").into());
    about_win.set_install_path(install_dir().into());
    about_win.set_data_path(dirs.config_dir.display().to_string().into());
    about_win.set_cache_path(dirs.cache_dir.display().to_string().into());
    about_win.set_logs_folder_path(dirs.logs_dir.display().to_string().into());

    // Open the project repository in the default browser.
    {
        about_win.on_open_repo(move || {
            const REPO_URL: &str = "https://github.com/dcog989/Cliptoo";
            if let Err(e) = std::process::Command::new("xdg-open").arg(REPO_URL).spawn() {
                tracing::warn!("about: failed to launch xdg-open for {REPO_URL}: {e}");
            }
        });
    }

    // Open a path (install/data/cache/logs dir) in the default file manager.
    {
        about_win.on_open_folder(move |path: SharedString| {
            if let Err(e) = std::process::Command::new("xdg-open")
                .arg(path.as_str())
                .spawn()
            {
                tracing::warn!("about: failed to launch xdg-open for {path}: {e}");
            }
        });
    }

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
