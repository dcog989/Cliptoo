// The only `unsafe` in the codebase — a Qt FFI shim (drag_qt.cpp) for
// operations Slint's public API can't provide on the Qt backend
// (QWindow::startSystemMove, QCursor::pos, QScreen geometry).
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
    fn cliptoo_cursor_pos(x: *mut i32, y: *mut i32);
    fn cliptoo_screen_size(widget: *mut std::ffi::c_void, w: *mut i32, h: *mut i32);
}

pub fn start_window_move(ui: &crate::AppWindow) {
    let win = slint::ComponentHandle::window(ui);
    if let Some(ptr) = win.qt_widget_ptr() {
        unsafe {
            cliptoo_start_window_move(ptr.as_ptr() as *mut std::ffi::c_void);
        }
    }
}

pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut x: i32 = 0;
    let mut y: i32 = 0;
    unsafe {
        cliptoo_cursor_pos(&mut x, &mut y);
    }
    if x == 0 && y == 0 {
        return None;
    }
    Some((x, y))
}

pub fn screen_size(ui: &crate::AppWindow) -> (i32, i32) {
    let win = slint::ComponentHandle::window(ui);
    let mut w: i32 = 1920;
    let mut h: i32 = 1080;
    if let Some(ptr) = win.qt_widget_ptr() {
        unsafe {
            cliptoo_screen_size(ptr.as_ptr() as *mut std::ffi::c_void, &mut w, &mut h);
        }
    }
    (w, h)
}
