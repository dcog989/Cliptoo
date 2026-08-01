use slint::ComponentHandle;
use tracing::info;

fn idx_of(needle: &str, haystack: &[&str]) -> i32 {
    haystack
        .iter()
        .position(|&s| s.eq_ignore_ascii_case(needle))
        .unwrap_or(0) as i32
}

fn clean_hotkey_text(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            let code = c as u32;
            if (1..=26).contains(&code) {
                ((b'a' + (code - 1) as u8) as char).to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Update a single `Theme` token on both the main window and the settings
/// window globals. Slint globals are per-window-instance, so live previews
/// need each window's `Theme` global updated.
fn apply_theme_to_windows(
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

fn reapply_theme(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    settings: &cliptoo_core::Settings,
) {
    let main_weak = main_ui.clone();
    let settings_weak = settings_win_ui.clone();
    let s_snap = settings.clone();
    tokio::spawn(async move {
        let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
        let _ = main_weak.upgrade_in_event_loop(move |ui| {
            crate::theme::fill_theme(
                &ui.global::<crate::Theme>(),
                &s_snap,
                is_dark,
                system_accent,
            );
            if let Some(win) = settings_weak.upgrade() {
                crate::theme::fill_theme(
                    &win.global::<crate::Theme>(),
                    &s_snap,
                    is_dark,
                    system_accent,
                );
            }
        });
    });
}

pub fn setup_settings_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    hotkey_tx: tokio::sync::watch::Sender<String>,
) -> crate::SettingsWindow {
    let settings_win = crate::SettingsWindow::new().expect("SettingsWindow creation");

    // Reset settings-open when the settings window is closed via the window
    // manager (ESC is handled by the settings-closing callback).
    {
        let main_ui = ui.as_weak();
        settings_win.window().on_close_requested(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_settings_open(false);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    // Initialise all settings properties.
    {
        let s = settings.borrow();
        settings_win.set_s_hotkey(s.hotkey.as_str().into());
        settings_win.set_s_start_with_system(s.start_with_system);
        settings_win.set_s_log_level_idx(idx_of(
            &s.logging_level,
            &["Debug", "Info", "Warn", "Error"],
        ));
        settings_win.set_s_log_retention_days(s.log_retention_days as i32);
        settings_win.set_s_theme_idx(idx_of(&s.theme, &["System", "Light", "Dark"]));
        settings_win.set_s_accent_hue(s.accent_hue as i32);
        settings_win.set_s_chroma_idx(idx_of(
            &s.accent_chroma_level,
            &["Neon", "Vibrant", "Mellow", "Muted", "Ditchwater"],
        ));
        settings_win.set_s_font_family(s.font_family.as_str().into());
        settings_win.set_s_font_size_hundredths((s.font_size * 100.0) as i32);
        settings_win.set_s_preview_font_size_hundredths((s.preview_font_size * 100.0) as i32);
        settings_win.set_s_row_padding_idx(idx_of(
            &s.clip_item_padding,
            &["Compact", "Standard", "Luxury"],
        ));
        settings_win.set_hover_delay(s.hover_preview_delay as i32);
        settings_win.set_s_image_preview_size(s.hover_image_preview_size as i32);
        settings_win.set_s_paste_as_plain_text(s.paste_as_plain_text);
        settings_win.set_s_diff_tool_path(s.compare_tool_path.as_str().into());
        settings_win.set_s_max_clips(s.max_clips as i32);
        settings_win.set_s_max_age_days(s.max_age_days as i32);
    }

    // Forward maintenance actions from SettingsWindow to AppWindow.
    {
        let main_ui = ui.as_weak();
        settings_win.on_maintenance_action(move |key: slint::SharedString| {
            if let Some(ui) = main_ui.upgrade() {
                ui.invoke_maintenance_action(key);
            }
        });
    }

    // When the settings window closes, reset settings-open so ESC and
    // blur-to-tray work again on the main window.
    {
        let main_ui = ui.as_weak();
        settings_win.on_settings_closing(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_settings_open(false);
            }
        });
    }

    // Accent hue: click-and-hold repeat via 100ms timer.
    {
        let sw = settings_win.as_weak();
        let timer = std::cell::RefCell::new(slint::Timer::default());
        settings_win.on_hue_step(move |direction: slint::SharedString| {
            let dir = direction.as_str();
            let t = timer.borrow_mut();
            t.stop();
            if dir == "stop" {
                return;
            }
            let step: i32 = if dir == "minus" { -1 } else { 1 };
            let weak = sw.clone();
            t.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(100),
                move || {
                    if let Some(win) = weak.upgrade() {
                        let cur = win.get_s_accent_hue();
                        let v = (cur + step).clamp(0, 360);
                        win.set_s_accent_hue(v);
                        win.invoke_setting_changed("accent_hue".into(), format!("{v}").into());
                    }
                },
            );
        });
    }

    // Font picker — native KDE font dialog via Qt (PyQt6).
    {
        let sw = settings_win.as_weak();
        let settings_ui = ui.as_weak();
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        settings_win.on_font_picker(move || {
            let script = r#"
from PyQt6.QtWidgets import QApplication, QFontDialog
app = QApplication([])
font, ok = QFontDialog.getFont()
if ok:
    print(font.family())
"#;
            let Ok(output) = std::process::Command::new("python3")
                .arg("-c")
                .arg(script)
                .output()
            else {
                return;
            };
            let family = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if family.is_empty() {
                return;
            }
            let mut s = s.borrow_mut();
            s.font_family.clone_from(&family);
            if let Some(win) = sw.upgrade() {
                win.set_s_font_family(family.as_str().into());
            }
            apply_theme_to_windows(&settings_ui, &sw, |t| {
                t.set_font_family(family.as_str().into());
            });
            let _ = s.save(&p);
        });
    }

    // Handle setting changes.
    {
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        let settings_ui = ui.as_weak();
        let sw = settings_win.as_weak();
        settings_win.on_setting_changed(
            move |key: slint::SharedString, value: slint::SharedString| {
                let key = key.to_string();
                let value = value.to_string();
                let mut s = s.borrow_mut();

                match key.as_str() {
                    "hotkey" => {
                        let cleaned = clean_hotkey_text(value.trim_end_matches('+'));
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
                    "logging_level" => s.logging_level = value.clone(),
                    "log_retention_days" => {
                        if let Ok(v) = value.parse::<u32>() {
                            s.log_retention_days = v;
                        }
                    }
                    "theme" => {
                        s.theme = value.clone();
                        reapply_theme(&settings_ui, &sw, &s);
                    }
                    "accent_hue" => {
                        if let Ok(v) = value.parse::<f64>() {
                            s.accent_hue = v;
                            reapply_theme(&settings_ui, &sw, &s);
                        }
                    }
                    "accent_chroma_level" => {
                        s.accent_chroma_level = value.clone();
                        reapply_theme(&settings_ui, &sw, &s);
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
                                t.set_font_size(v as f32)
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
                        }
                    }
                    "paste_as_plain_text" => s.paste_as_plain_text = value == "true",
                    "compare_tool_path" => s.compare_tool_path = value.clone(),
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
            },
        );
    }

    // Show settings window from hamburger menu.
    {
        let sw = settings_win.as_weak();
        let main_ui = ui.as_weak();
        ui.on_menu_settings(move || {
            if let Some(win) = sw.upgrade() {
                if let Some(ui) = main_ui.upgrade() {
                    // Guard blur-to-tray while the settings window is visible;
                    // cleared when the settings window closes.
                    ui.set_settings_open(true);
                }
                win.show().ok();
            }
        });
    }

    // About.
    ui.on_menu_about(move || {
        info!("About Cliptoo — clipboard manager for Wayland/KDE Plasma 6");
    });

    settings_win
}
