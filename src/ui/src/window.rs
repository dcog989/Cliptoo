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
    // Dismiss a showing hover preview: its preview-visible latch survives a
    // hide, so without this the popup would reappear orphaned on the next
    // show() with no cursor under it to ever dismiss it.
    ui.set_preview_visible(false);
    let _ = ComponentHandle::hide(ui);
}

/// Show the main window and re-arm the blur-detection poll via
/// `window-visible`. Returns the `show` result so callers can propagate
/// platform errors.
pub fn show_window(ui: &crate::AppWindow) -> Result<(), slint::PlatformError> {
    let result = ComponentHandle::show(ui);
    if result.is_ok() {
        ui.set_window_visible(true);
        // Activate/focus the window now. Otherwise the first click on a just-
        // shown window is consumed merely focusing it, so controls (e.g. the
        // hamburger menu) would need a second click.
        crate::drag::activate_window(ui);
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

/// Toggle the main window's visibility: hide when visible, show when hidden.
/// Shared by the global-hotkey and system-tray handlers.
pub fn toggle_window(ui: &crate::AppWindow) {
    if ComponentHandle::window(ui).is_visible() {
        hide_window(ui);
    } else {
        let _ = show_window(ui);
    }
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

/// Reentrancy guard shared by the close handlers. Marks the guard cell active
/// while the returned handle is alive and releases it on drop, so every exit
/// path (early returns, dropped weak handle, timer callback) resets it without
/// a manual `set(false)`. Blur-close and window-close can fire back-to-back;
/// the guard keeps the second from re-entering `save_size_and_hide` while the
/// first is still mid-flight.
struct CloseGuard(std::rc::Rc<std::cell::Cell<bool>>);

impl CloseGuard {
    /// Acquire the guard if it is free; returns `None` when a close handler is
    /// already running.
    fn try_acquire(guard: &std::rc::Rc<std::cell::Cell<bool>>) -> Option<Self> {
        if guard.get() {
            return None;
        }
        guard.set(true);
        Some(Self(guard.clone()))
    }
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

pub fn setup_close_handlers(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
) {
    let guard = std::rc::Rc::new(std::cell::Cell::new(false));
    let path = dirs.settings_path.clone();

    {
        let guard = guard.clone();
        let s = settings.clone();
        let p = path.clone();
        let weak = ui.as_weak();
        ui.on_close_window(move || {
            let Some(_guard) = CloseGuard::try_acquire(&guard) else {
                return;
            };
            let Some(ui) = weak.upgrade() else {
                return;
            };
            save_size_and_hide(&ui, &s, &p);
        });
    }
    {
        let guard = guard.clone();
        let s = settings.clone();
        let p = path;
        let weak = ui.as_weak();
        ui.on_blur_closed(move || {
            let Some(guard) = CloseGuard::try_acquire(&guard) else {
                return;
            };

            // "Always close to tray" off: losing focus leaves the window open.
            if !s.borrow().always_close_to_tray {
                return;
            }

            let weak2 = weak.clone();
            let s2 = s.clone();
            let p2 = p.clone();
            slint::Timer::single_shot(Duration::ZERO, move || {
                // Hold the guard for the whole callback, not just the outer
                // blur event, so a close fired while this runs is blocked.
                let _guard = guard;
                let Some(ui) = weak2.upgrade() else {
                    return;
                };
                save_size_and_hide(&ui, &s2, &p2);
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

/// When "always close to tray" is off, the window stays open but unfocused
/// after losing focus (see `on_blur_closed`), so switching back to it (e.g.
/// Alt+Tab, taskbar) never goes through `show_window()`'s `activate_window()`
/// call. The compositor restores keyboard focus but not Qt activation, so
/// the first click would just focus the window instead of registering on
/// the widget beneath it. Re-activate on every regained-focus event to
/// close that gap.
pub fn setup_focus_regained(ui: &crate::AppWindow) {
    let weak = ui.as_weak();
    ui.on_focus_regained(move || {
        if let Some(ui) = weak.upgrade() {
            crate::drag::activate_window(&ui);
        }
    });
}
