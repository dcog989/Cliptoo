//! Row-height lookup for the configurable clip-item padding density.
//!
//! Launch-position window placement was removed: `slint::Window::set_position`
//! is a no-op for xdg_toplevel windows under Wayland/KWin — the protocol has
//! no client-side "set absolute position" request (only compositor-driven
//! placement, or the interactive `xdg_toplevel::move` used by `drag.rs`).
//! None of the seven alignment modes ever actually moved the window.

/// Row height for a padding mode at the given clip-list font size. The row
/// grows with the text so a larger font never clips; the values are chosen so
/// the default 13 px font keeps the historical 24 / 32 / 40 px rows.
pub fn row_height(padding: &str, font_size: f64) -> f64 {
    let padding_height = match padding {
        "Compact" => 11.0,
        "Luxury" => 27.0,
        _ => 19.0,
    };
    font_size + padding_height
}

#[cfg(test)]
mod tests {
    use super::row_height;

    #[test]
    fn row_height_keeps_defaults_and_scales_with_font() {
        assert_eq!(row_height("Compact", 13.0), 24.0);
        assert_eq!(row_height("Standard", 13.0), 32.0);
        assert_eq!(row_height("Luxury", 13.0), 40.0);
        assert!(row_height("Standard", 18.0) > row_height("Standard", 13.0));
    }
}
