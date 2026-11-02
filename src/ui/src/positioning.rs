//! Row-height lookup for the configurable clip-item padding density.
//!
//! Launch-position window placement was removed: `slint::Window::set_position`
//! is a no-op for xdg_toplevel windows under Wayland/KWin — the protocol has
//! no client-side "set absolute position" request (only compositor-driven
//! placement, or the interactive `xdg_toplevel::move` used by `drag.rs`).
//! None of the seven alignment modes ever actually moved the window.

pub fn row_height(padding: &str) -> f64 {
    match padding {
        "Compact" => 24.0,
        "Luxury" => 40.0,
        _ => 32.0,
    }
}
