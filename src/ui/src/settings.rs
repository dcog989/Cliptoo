use slint::ComponentHandle;
use tracing::info;

fn idx_of(needle: &str, haystack: &[&str]) -> i32 {
    haystack
        .iter()
        .position(|&s| s.eq_ignore_ascii_case(needle))
        .unwrap_or(0) as i32
}

fn reapply_theme(settings_ui: &slint::Weak<crate::AppWindow>, settings: &cliptoo_core::Settings) {
    let weak = settings_ui.clone();
    let s_snap = settings.clone();
    tokio::spawn(async move {
        let is_dark = match s_snap.theme.as_str() {
            "Light" => false,
            "Dark" => true,
            _ => crate::theme::detect_system_dark().await.unwrap_or(true),
        };
        let system_accent = if s_snap.theme.as_str() != "Light" && s_snap.theme.as_str() != "Dark" {
            crate::theme::detect_system_accent().await
        } else {
            None
        };
        let _ = weak.upgrade_in_event_loop(move |ui| {
            crate::theme::apply_theme_inner(&ui, &s_snap, is_dark, system_accent);
        });
    });
}

pub fn setup_settings_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
) -> crate::SettingsWindow {
    let settings_win = crate::SettingsWindow::new().expect("SettingsWindow creation");

    // Initialise all settings properties.
    {
        let s = settings.borrow();
        settings_win.set_s_hotkey(s.hotkey.as_str().into());
        settings_win.set_s_preview_hotkey(s.preview_hotkey.as_str().into());
        settings_win.set_s_quick_paste_hotkey(s.quick_paste_hotkey.as_str().into());
        settings_win.set_s_launch_position_idx(idx_of(
            &s.launch_position,
            &[
                "Cursor",
                "Center",
                "TopLeft",
                "TopRight",
                "BottomLeft",
                "BottomRight",
                "Fixed",
            ],
        ));
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

    // Handle setting changes.
    {
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        let settings_ui = ui.as_weak();
        settings_win.on_setting_changed(
            move |key: slint::SharedString, value: slint::SharedString| {
                let key = key.to_string();
                let value = value.to_string();
                let mut s = s.borrow_mut();

                match key.as_str() {
                    "hotkey" => s.hotkey = value.clone(),
                    "preview_hotkey" => s.preview_hotkey = value.clone(),
                    "quick_paste_hotkey" => s.quick_paste_hotkey = value.clone(),
                    "launch_position" => s.launch_position = value.clone(),
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
                        reapply_theme(&settings_ui, &s);
                    }
                    "accent_hue" => {
                        if let Ok(v) = value.parse::<f64>() {
                            s.accent_hue = v;
                            reapply_theme(&settings_ui, &s);
                        }
                    }
                    "accent_chroma_level" => {
                        s.accent_chroma_level = value.clone();
                        reapply_theme(&settings_ui, &s);
                    }
                    "font_family" => {
                        s.font_family = value.clone();
                        if let Some(ui) = settings_ui.upgrade() {
                            ui.global::<crate::Theme>()
                                .set_font_family(value.as_str().into());
                        }
                    }
                    "font_size" => {
                        if let Ok(v) = value.parse::<f64>() {
                            s.font_size = v;
                            if let Some(ui) = settings_ui.upgrade() {
                                ui.global::<crate::Theme>().set_font_size(v as f32);
                            }
                        }
                    }
                    "preview_font_size" => {
                        if let Ok(v) = value.parse::<f64>() {
                            s.preview_font_size = v;
                            if let Some(ui) = settings_ui.upgrade() {
                                ui.global::<crate::Theme>().set_preview_font_size(v as f32);
                            }
                        }
                    }
                    "clip_item_padding" => {
                        s.clip_item_padding = value.clone();
                        if let Some(ui) = settings_ui.upgrade() {
                            ui.global::<crate::Theme>().set_row_height(
                                crate::positioning::row_height(value.as_str()) as f32,
                            );
                        }
                    }
                    "hover_preview_delay" => {
                        if let Ok(ms) = value.parse::<u32>() {
                            s.hover_preview_delay = ms;
                            if let Some(ui) = settings_ui.upgrade() {
                                ui.global::<crate::Theme>()
                                    .set_hover_preview_delay(ms as i64);
                            }
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
        ui.on_menu_settings(move || {
            if let Some(win) = sw.upgrade() {
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
