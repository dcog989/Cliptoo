// about.rs — About window: logo, tagline, and links to the repo and log folder.
use slint::ComponentHandle;

pub fn setup_about_window(ui: &crate::AppWindow, logs_dir: &std::path::Path) -> crate::AboutWindow {
    let about_win = crate::AboutWindow::new().expect("AboutWindow creation");
    about_win.set_logs_folder_path(logs_dir.display().to_string().into());
    about_win.set_version(env!("CARGO_PKG_VERSION").into());

    // Open the project repository in the default browser.
    {
        about_win.on_open_repo(move || {
            const REPO_URL: &str = "https://github.com/dcog989/Cliptoo";
            if let Err(e) = std::process::Command::new("xdg-open").arg(REPO_URL).spawn() {
                tracing::warn!("about: failed to launch xdg-open for {REPO_URL}: {e}");
            }
        });
    }

    // Open the logs folder in the default file manager.
    {
        let logs_dir = logs_dir.to_path_buf();
        about_win.on_open_logs_folder(move || {
            if let Err(e) = std::process::Command::new("xdg-open")
                .arg(&logs_dir)
                .spawn()
            {
                tracing::warn!(
                    "about: failed to launch xdg-open for {}: {e}",
                    logs_dir.display()
                );
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
                let _ = win.show();
            }
        });
    }

    about_win
}
