use slint::ComponentHandle;
use std::path::Path;
use std::time::Duration;

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
    let _ = slint::ComponentHandle::hide(ui);
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

            let weak2 = weak.clone();
            let s2 = s.clone();
            let p2 = p.clone();
            let guard2 = guard.clone();

            // Defer hide to next event-loop tick so that any intra-window
            // focus transfers (e.g. clicking the search LineEdit) have
            // time to settle and update search-focused before we decide.
            slint::Timer::single_shot(Duration::ZERO, move || {
                let ui = match weak2.upgrade() {
                    Some(u) => u,
                    None => {
                        guard2.set(false);
                        return;
                    }
                };

                // If the search field now has focus, this was an
                // intra-window focus transfer, not a true blur.
                if !ui.get_search_focused() {
                    save_size_and_hide(&ui, &s2, &p2);
                }

                guard2.set(false);
            });
        });
    }
}

pub fn setup_close_to_tray(ui: &crate::AppWindow) {
    let weak = ui.as_weak();
    ui.on_menu_close_to_tray(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = slint::ComponentHandle::hide(&ui);
        }
    });
}
