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

/// Position the editor window centered over the main window with a
/// +30px downward offset for visual depth.
const EDITOR_OFFSET_Y: i32 = 30;
pub fn position_editor_relative_to_main(
    editor: &impl slint::ComponentHandle,
    main: &impl slint::ComponentHandle,
) {
    let main_pos = main.window().position();
    let main_size = main.window().size();
    let editor_size = editor.window().size();
    let x = main_pos.x + (main_size.width as i32 - editor_size.width as i32) / 2;
    let y =
        main_pos.y + (main_size.height as i32 - editor_size.height as i32) / 2 + EDITOR_OFFSET_Y;
    editor
        .window()
        .set_position(slint::PhysicalPosition { x, y });
}
