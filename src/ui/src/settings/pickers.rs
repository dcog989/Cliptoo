//! Native picker dialogs and the accent "Clear" handler. Each picker runs a
//! child process (python3/PyQt6 for fonts, kdialog for colors) and applies the
//! result to settings and the live theme.

use slint::ComponentHandle;

use super::theme_apply::apply_theme_to_windows;

/// Show a toast on the settings window (its bottom overlay). Severity is one
/// of "info" | "warn" | "error".
fn show_settings_toast(win: &crate::SettingsWindow, message: &str, severity: &str) {
    win.set_toast_visible(false);
    win.set_toast_message(message.into());
    win.set_toast_severity(severity.into());
    win.set_toast_visible(true);
}

/// Clear accent: empty accent_color means "use the OS default accent".
pub(super) fn setup_clear_accent(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
    fillers: crate::theme::ThemeFillers,
) {
    let sw = settings_win.as_weak();
    let settings_ui = main_ui.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_clear_accent_color(move || {
        // Clear synchronously: Rc<RefCell<Settings>> is !Send, so it must
        // not cross into the spawned task. Snapshot the cleared settings.
        let s_snap;
        {
            let mut s = s.borrow_mut();
            s.accent_color = String::new();
            s_snap = s.clone();
        }
        let sw = sw.clone();
        let settings_ui = settings_ui.clone();
        let p = p.clone();
        let fillers = fillers.clone();
        tokio::spawn(async move {
            // Resolve first so the shared cache holds the freshly detected
            // OS accent; the settings swatch reads that cache. Without this,
            // a previously custom accent leaves the cache stale and "Clear"
            // shows the fallback color until a second click.
            let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
            let swatch = crate::theme::default_accent_color();
            let main_weak = settings_ui.clone();
            let settings_weak = sw.clone();
            let s_main = s_snap.clone();
            let s_settings = s_snap.clone();
            let _ = main_weak.upgrade_in_event_loop(move |ui| {
                crate::theme::fill_theme(
                    &ui.global::<crate::Theme>(),
                    &s_main,
                    is_dark,
                    system_accent,
                );
            });
            let _ = settings_weak.upgrade_in_event_loop(move |win| {
                crate::theme::fill_theme(
                    &win.global::<crate::Theme>(),
                    &s_settings,
                    is_dark,
                    system_accent,
                );
                // Re-fill the tray and About window globals too.
                crate::theme::apply_theme_fillers(&fillers, &s_settings, is_dark, system_accent);
                win.set_s_accent_color(swatch);
            });
            let _ = s_snap.save(&p);
        });
    });
}

/// Font picker — native KDE font dialog via Qt (PyQt6).
///
/// The dialog runs in a child `python3` process; the blocking `.output()` call
/// runs on a background thread so the app stays responsive while the dialog is
/// open. `slint::spawn_local` polls the continuation on the UI thread, which
/// is what lets it capture the non-Send `Rc<RefCell<Settings>>`. A missing or
/// failing python3/PyQt6 is logged and surfaced via the settings toast instead
/// of silently doing nothing; cancelling the dialog is not an error.
pub(super) fn setup_font_picker(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
) {
    let sw = settings_win.as_weak();
    let settings_ui = main_ui.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_font_picker(move || {
        let script = r#"
from PyQt6.QtWidgets import QApplication, QFontDialog
app = QApplication([])
font, ok = QFontDialog.getFont()
if ok:
    print(font.family())
"#;
        let sw = sw.clone();
        let settings_ui = settings_ui.clone();
        let s = s.clone();
        let p = p.clone();
        // The callback fires from the event loop, so spawning can only fail
        // if it is shutting down; ignore the JoinHandle (dropping it does not
        // cancel the future) but surface the rare spawn error.
        if let Err(e) = slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || {
                std::process::Command::new("python3")
                    .arg("-c")
                    .arg(script)
                    .output()
            })
            .await;

            // A successful run prints the family on stdout; anything else
            // (missing python3/PyQt6, a non-zero exit, a cancelled dialog)
            // yields "". The failure cases are distinguished from a plain
            // cancel so they can be surfaced.
            let (family, error): (String, Option<String>) = match result {
                Ok(Ok(output)) if output.status.success() => (
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                    None,
                ),
                Ok(Ok(output)) => (
                    String::new(),
                    Some(format!(
                        "python3 exited {}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                ),
                Ok(Err(e)) => (String::new(), Some(format!("failed to run python3: {e}"))),
                Err(e) => (
                    String::new(),
                    Some(format!("font picker task panicked: {e}")),
                ),
            };
            if let Some(err) = error.as_ref() {
                tracing::warn!("font-picker: {err}");
            }

            if family.is_empty() {
                if error.is_some()
                    && let Some(win) = sw.upgrade()
                {
                    show_settings_toast(
                        &win,
                        "Font picker failed (python3 + PyQt6 required)",
                        "error",
                    );
                }
                return;
            }

            {
                let mut settings = s.borrow_mut();
                settings.font_family.clone_from(&family);
                let _ = settings.save(&p);
            }
            apply_theme_to_windows(&settings_ui, &sw, |t| {
                t.set_font_family(family.as_str().into());
            });
            if let Some(win) = sw.upgrade() {
                win.set_s_font_family(family.as_str().into());
            }
        }) {
            tracing::warn!("font-picker: failed to schedule on the event loop: {e}");
        }
    });
}

/// Normalize a color-dialog result to an uppercase `#RRGGBB` string, returning
/// `None` when the output is not a 6-digit hex color (e.g. the user cancelled).
fn normalize_picker_hex(raw: &str) -> Option<String> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_ascii_uppercase()))
    } else {
        None
    }
}

/// Accent color picker — native KDE color dialog via `kdialog --getcolor`.
///
/// The dialog runs in a child `kdialog` process; the blocking `.output()` call
/// runs on a background thread so the app stays responsive while the dialog is
/// open. `slint::spawn_local` polls the continuation on the UI thread, which
/// is what lets it capture the non-Send `Rc<RefCell<Settings>>`. Cancelling the
/// dialog is not an error; a launch failure is logged and surfaced via the
/// settings toast instead of silently doing nothing.
pub(super) fn setup_accent_picker(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
    fillers: crate::theme::ThemeFillers,
) {
    let sw = settings_win.as_weak();
    let settings_ui = main_ui.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_accent_picker(move || {
        let sw = sw.clone();
        let settings_ui = settings_ui.clone();
        let s = s.clone();
        let p = p.clone();
        let fillers = fillers.clone();
        // Preselect the colour currently shown on the swatch. When "Clear" is
        // active this is the resolved OS/default accent, so the dialog still
        // opens on the colour the user actually sees.
        let current = sw.upgrade().map(|win| {
            let c = win.get_s_accent_color();
            format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue())
        });
        if let Err(e) = slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut cmd = std::process::Command::new("kdialog");
                cmd.arg("--title").arg("Cliptoo accent color");
                if let Some(current) = current.as_deref() {
                    cmd.arg("--default").arg(current);
                }
                cmd.arg("--getcolor").output()
            })
            .await;

            // A successful run prints `#rrggbb`; a cancelled dialog exits
            // non-zero with nothing on stderr. A missing hex is treated as a
            // cancel; only genuine launch/output errors are surfaced.
            let (hex, error): (Option<String>, Option<String>) = match result {
                Ok(Ok(output)) if output.status.success() => (
                    normalize_picker_hex(&String::from_utf8_lossy(&output.stdout)),
                    None,
                ),
                Ok(Ok(output)) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    (
                        None,
                        if stderr.is_empty() {
                            None
                        } else {
                            Some(format!("kdialog exited {}: {stderr}", output.status))
                        },
                    )
                }
                Ok(Err(e)) => (None, Some(format!("failed to run kdialog: {e}"))),
                Err(e) => (None, Some(format!("accent picker task panicked: {e}"))),
            };
            if let Some(err) = error.as_ref() {
                tracing::warn!("accent-picker: {err}");
            }

            let Some(hex) = hex else {
                if error.is_some()
                    && let Some(win) = sw.upgrade()
                {
                    show_settings_toast(&win, "Color picker failed (kdialog required)", "error");
                }
                return;
            };

            {
                let mut settings = s.borrow_mut();
                let (r, g, b) = crate::theme::parse_accent_hex(&hex);
                let (h, sat, val) = crate::theme::rgb_to_hsv(r, g, b);
                // The hex is authoritative; the HSV fields are kept in sync so
                // older settings files and any remaining readers stay valid.
                settings.accent_color.clone_from(&hex);
                settings.accent_hue = h;
                settings.accent_saturation = sat;
                settings.accent_value = val;
                let _ = settings.save(&p);
            }
            let (is_dark, _) = crate::theme::cached_resolved_theme();
            if let Some(ui) = settings_ui.upgrade() {
                crate::theme::fill_accent(&ui.global::<crate::Theme>(), &s.borrow(), is_dark, None);
            }
            if let Some(win) = sw.upgrade() {
                win.set_s_accent_color(crate::theme::accent_hex_to_color(&hex));
                let s = s.borrow();
                crate::theme::fill_accent(&win.global::<crate::Theme>(), &s, is_dark, None);
                crate::theme::apply_theme_fillers(&fillers, &s, is_dark, None);
            }
        }) {
            tracing::warn!("accent-picker: failed to schedule on the event loop: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::normalize_picker_hex;

    #[test]
    fn normalize_picker_hex_accepts_hex_and_rejects_cancel() {
        assert_eq!(
            normalize_picker_hex("#7c6ee6\n").as_deref(),
            Some("#7C6EE6")
        );
        assert_eq!(normalize_picker_hex("7C6EE6").as_deref(), Some("#7C6EE6"));
        assert_eq!(normalize_picker_hex(""), None);
        assert_eq!(normalize_picker_hex("#12345"), None);
        assert_eq!(normalize_picker_hex("#gggggg"), None);
    }
}
