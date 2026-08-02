use slint::ComponentHandle;
use std::path::Path;
use std::time::Duration;

/// Hide the main window. Also clears `window-visible`, which stops the
/// blur-detection poll (AppWindow.slint `focus-poll` is gated on it).
pub fn hide_window(ui: &crate::AppWindow) {
    ui.set_window_visible(false);
    // Closing to tray clears any active search so the next show() reopens the
    // full list rather than a stale filtered set with an empty search box.
    if !ui.get_search_text().is_empty() {
        ui.set_search_text("".into());
        ui.invoke_search_changed("".into());
    }
    let _ = ComponentHandle::hide(ui);
}

/// Show the main window and re-arm the blur-detection poll via
/// `window-visible`. Returns the `show` result so callers can propagate
/// platform errors.
pub fn show_window(ui: &crate::AppWindow) -> Result<(), slint::PlatformError> {
    let result = ComponentHandle::show(ui);
    if result.is_ok() {
        ui.set_window_visible(true);
        // forward-focus only seeds focus-scope on the window's first-ever
        // activation. Re-anchor it explicitly on every show so a prior
        // session's click into search-input can't leave it permanently
        // focused, which would silence blur-to-tray (see AppWindow.slint
        // reset-focus doc comment).
        ui.invoke_reset_focus();
        // Always start the clip list at the top on show, rather than
        // wherever it was last scrolled to.
        ui.invoke_reset_scroll();
    }
    result
}

/// Window drag via Qt FFI (xdg-shell _move protocol).
pub fn setup_drag(ui: &crate::AppWindow) {
    let drag_started = std::rc::Rc::new(std::cell::Cell::new(false));
    {
        let started = drag_started.clone();
        let weak = ui.as_weak();
        ui.on_drag_started(move || {
            if started.replace(true) {
                return;
            }
            if let Some(ui) = weak.upgrade() {
                crate::drag::start_window_move(&ui);
            }
            started.set(false);
        });
    }
    {
        let started = drag_started;
        ui.on_drag_ended(move || {
            started.set(false);
        });
    }
}

/// Window resize via stored width/height.
pub fn setup_resize(ui: &crate::AppWindow) {
    let resize_origin = std::rc::Rc::new(std::cell::RefCell::new(None::<(f32, f32)>));
    {
        let origin = resize_origin.clone();
        let weak = ui.as_weak();
        ui.on_resize_start(move || {
            let ui = match weak.upgrade() {
                Some(u) => u,
                None => return,
            };
            *origin.borrow_mut() = Some((ui.get_stored_width(), ui.get_stored_height()));
        });
    }
    {
        let origin = resize_origin.clone();
        let weak = ui.as_weak();
        ui.on_resize_delta(move |dx: f32, dy: f32| {
            let ui = match weak.upgrade() {
                Some(u) => u,
                None => return,
            };
            let (base_w, base_h) = match *origin.borrow() {
                Some(s) => s,
                None => return,
            };
            let new_w = (base_w + dx).max(ui.get_min_window_width());
            let new_h = (base_h + dy).max(ui.get_min_window_height());
            ui.set_stored_width(new_w);
            ui.set_stored_height(new_h);
        });
    }
    {
        let origin = resize_origin;
        ui.on_resize_ended(move || {
            *origin.borrow_mut() = None;
        });
    }
}

fn save_size_and_hide(
    ui: &crate::AppWindow,
    settings: &std::cell::RefCell<cliptoo_core::Settings>,
    path: &Path,
) {
    let size = slint::ComponentHandle::window(ui).size();
    {
        let mut s = settings.borrow_mut();
        s.window_width = size.width as f64;
        s.window_height = size.height as f64;
    }
    let _ = settings.borrow().save(path);
    hide_window(ui);
}

pub fn setup_close_handlers(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
) {
    let hide_guard = std::rc::Rc::new(std::cell::Cell::new(false));
    let path = dirs.settings_path.clone();

    {
        let guard = hide_guard.clone();
        let s = settings.clone();
        let p = path.clone();
        let weak = ui.as_weak();
        ui.on_close_window(move || {
            if guard.get() {
                return;
            }
            guard.set(true);
            let ui = match weak.upgrade() {
                Some(u) => u,
                None => {
                    guard.set(false);
                    return;
                }
            };
            save_size_and_hide(&ui, &s, &p);
            guard.set(false);
        });
    }
    {
        let guard = hide_guard;
        let s = settings.clone();
        let p = path;
        let weak = ui.as_weak();
        ui.on_blur_closed(move || {
            if guard.get() {
                return;
            }
            guard.set(true);

            // "Always close to tray" off: losing focus leaves the window open.
            if !s.borrow().always_close_to_tray {
                guard.set(false);
                return;
            }

            let weak2 = weak.clone();
            let s2 = s.clone();
            let p2 = p.clone();
            let guard2 = guard.clone();

            slint::Timer::single_shot(Duration::ZERO, move || {
                let ui = match weak2.upgrade() {
                    Some(u) => u,
                    None => {
                        guard2.set(false);
                        return;
                    }
                };
                save_size_and_hide(&ui, &s2, &p2);
                guard2.set(false);
            });
        });
    }
}

pub fn setup_close_to_tray(ui: &crate::AppWindow) {
    let weak = ui.as_weak();
    ui.on_menu_close_to_tray(move || {
        if let Some(ui) = weak.upgrade() {
            hide_window(&ui);
        }
    });
}
