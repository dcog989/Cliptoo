// The only `unsafe` in the codebase — a Qt FFI shim (drag_qt.cpp) for
// QWindow::startSystemMove, which Slint's public API can't provide on the Qt
// backend (the xdg-shell interactive-move request).
// Crate roots `#![deny(unsafe_code)]`; this module is the sole carve-out.
#![allow(unsafe_code)]

use i_slint_backend_qt::QtWidgetAccessor;

// Soundness invariants:
// - The widget pointer comes from QtWidgetAccessor::qt_widget_ptr(), a live
//   QWidget* valid for the window's lifetime; callers guard `Some(ptr)`.
// - Every caller runs on the Slint event loop / Qt GUI thread (a Qt
//   requirement); the pointer is never moved across threads.
// - The C++ shim trusts the cast (no null/type checks) and never leaks a
//   pointer back into Rust references; out-params are written by C++ and read
//   immediately.
unsafe extern "C" {
    fn cliptoo_start_window_move(widget: *mut std::ffi::c_void);
    fn cliptoo_activate_window(widget: *mut std::ffi::c_void);
}

pub fn start_window_move(ui: &crate::AppWindow) {
    let win = slint::ComponentHandle::window(ui);
    if let Some(ptr) = win.qt_widget_ptr() {
        unsafe {
            cliptoo_start_window_move(ptr.as_ptr() as *mut std::ffi::c_void);
        }
    }
}

/// Raise and activate the window so the first click on a freshly-shown window
/// isn't consumed just focusing it. See cliptoo_activate_window in drag_qt.cpp.
pub fn activate_window(ui: &crate::AppWindow) {
    let win = slint::ComponentHandle::window(ui);
    if let Some(ptr) = win.qt_widget_ptr() {
        unsafe {
            cliptoo_activate_window(ptr.as_ptr() as *mut std::ffi::c_void);
        }
    }
}
