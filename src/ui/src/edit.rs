use std::sync::Arc;

use slint::ComponentHandle;

pub fn setup_edit_window(
    _ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    db: &Arc<cliptoo_core::db::DbPool>,
) -> crate::EditWindow {
    let edit_win = crate::EditWindow::new().expect("EditWindow creation");

    // Save editor size on close.
    let ew = edit_win.as_weak();
    {
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        edit_win.window().on_close_requested(move || {
            if let Some(win) = ew.upgrade() {
                let size = win.window().size();
                let mut s = s.borrow_mut();
                s.editor_window_width = size.width as f64;
                s.editor_window_height = size.height as f64;
                let _ = s.save(&p);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    // Cancel closes the editor.
    {
        let ew = edit_win.as_weak();
        edit_win.on_cancel_clicked(move || {
            if let Some(win) = ew.upgrade() {
                let _ = win.hide();
            }
        });
    }

    // Save updated clip content to DB.
    {
        let ew = edit_win.as_weak();
        let edit_db = db.clone();
        edit_win.on_save_clicked(
            move |id: i32, content: slint::SharedString, tags: slint::SharedString| {
                let db = edit_db.clone();
                let win = ew.clone();
                let content = content.to_string();
                let tags = tags.to_string();
                tokio::spawn(async move {
                    let normalized = cliptoo_core::content::normalize_line_endings(&content);
                    if let Some(classified) =
                        cliptoo_core::content::ContentProcessor::process(&normalized)
                    {
                        let _ = db
                            .with(|conn| {
                                cliptoo_core::db::queries::update_clip_content(
                                    conn,
                                    id as i64,
                                    &classified.content,
                                    &classified.preview_content,
                                    &classified.content_hash,
                                    classified.clip_type.as_str(),
                                    classified.was_trimmed,
                                    classified.has_leading_whitespace,
                                    classified.is_multiline,
                                    classified.size_in_bytes,
                                )
                            })
                            .await;
                    }
                    let _ = db
                        .with(|conn| cliptoo_core::db::queries::update_tags(conn, id as i64, &tags))
                        .await;
                    let _ = win.upgrade_in_event_loop(move |win| {
                        let _ = win.hide();
                    });
                });
            },
        );
    }

    // Toggle edit/view mode.
    {
        let ew = edit_win.as_weak();
        edit_win.on_toggle_mode(move || {
            if let Some(win) = ew.upgrade() {
                win.set_editing(!win.get_editing());
            }
        });
    }

    edit_win
}
