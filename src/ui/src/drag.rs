use i_slint_backend_qt::QtWidgetAccessor;

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
