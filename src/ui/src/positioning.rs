//! Window positioning — HLD §6.2
//! Seven alignment modes implemented via Qt FFI for cursor + screen queries.

use crate::drag;
use cliptoo_core::Settings;
use slint::ComponentHandle;

/// Send-safe copy of the settings fields needed for positioning.
#[derive(Clone, Debug)]
pub struct PositionSettings {
    pub launch_position: String,
    pub fixed_x: i32,
    pub fixed_y: i32,
    pub clip_item_padding: String,
}

impl From<&Settings> for PositionSettings {
    fn from(s: &Settings) -> Self {
        Self {
            launch_position: s.launch_position.clone(),
            fixed_x: s.fixed_x,
            fixed_y: s.fixed_y,
            clip_item_padding: s.clip_item_padding.clone(),
        }
    }
}

pub fn row_height(padding: &str) -> f64 {
    match padding {
        "Compact" => 24.0,
        "Luxury" => 40.0,
        _ => 32.0,
    }
}

/// Vertical offset below the cursor when launching in "Cursor" mode.
/// Offsets the window below the cursor so the toolbar doesn't obscure
/// the clip under the cursor. Not directly tied to the toolbar height
/// (which is 48px in Toolbar.slint) — this is a UX tuning value.
const ACTION_BAR_HEIGHT: f64 = 40.0;

/// Position the window according to `settings.launch_position`.
/// Must be called on the UI thread.
pub fn position_window(ui: &crate::AppWindow, settings: &Settings) {
    position_window_ex(ui, &PositionSettings::from(settings))
}

/// Position the window from a `PositionSettings` (Send-safe).
/// Must be called on the UI thread.
pub fn position_window_ex(ui: &crate::AppWindow, pos: &PositionSettings) {
    let win = slint::ComponentHandle::window(ui);
    let size = win.size();
    let win_w = size.width as i32;
    let win_h = size.height as i32;
    let (screen_w, screen_h) = drag::screen_size(ui);

    let (x, y) = match pos.launch_position.as_str() {
        "Cursor" => {
            let offset = ACTION_BAR_HEIGHT + row_height(&pos.clip_item_padding) / 2.0;
            match drag::cursor_pos() {
                Some((cx, cy)) => {
                    let wx = cx.saturating_sub(win_w / 2);
                    let wy = (cy as f64 + offset) as i32;
                    (wx, wy)
                }
                None => ((screen_w - win_w) / 2, (screen_h - win_h) / 2),
            }
        }
        "Center" => ((screen_w - win_w) / 2, (screen_h - win_h) / 2),
        "Fixed" => (pos.fixed_x, pos.fixed_y),
        "TopLeft" => (0, 0),
        "TopRight" => ((screen_w - win_w).max(0), 0),
        "BottomLeft" => (0, (screen_h - win_h).max(0)),
        "BottomRight" => ((screen_w - win_w).max(0), (screen_h - win_h).max(0)),
        _ => ((screen_w - win_w) / 2, (screen_h - win_h) / 2),
    };

    win.set_position(slint::PhysicalPosition::new(x.max(0), y.max(0)));
}

/// Position the editor window centered over the main window with a
/// +30px downward offset for visual depth.
const EDITOR_OFFSET_Y: i32 = 30;
pub fn position_editor_relative_to_main(
    editor: &impl ComponentHandle,
    main: &impl ComponentHandle,
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
