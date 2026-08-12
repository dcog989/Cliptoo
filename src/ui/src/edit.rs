use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle;

use crate::helpers;

pub fn setup_edit_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    db: &Arc<cliptoo_core::db::DbPool>,
) -> crate::EditWindow {
    let edit_win = crate::EditWindow::new().expect("EditWindow creation");
    let main_ui = ui.as_weak();

    // Save editor size on close.
    let ew = edit_win.as_weak();
    {
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        let main_ui = main_ui.clone();
        edit_win.window().on_close_requested(move || {
            if let Some(win) = ew.upgrade() {
                let size = win.window().size();
                let mut s = s.borrow_mut();
                s.editor_window_width = size.width as f64;
                s.editor_window_height = size.height as f64;
                let _ = s.save(&p);
            }
            if let Some(ui) = main_ui.upgrade() {
                ui.set_edit_open(false);
                // Re-focus the main window once the editor is gone; it lost
                // focus to the editor and otherwise blur-to-tray would hide it.
                let ui = ui.as_weak();
                slint::Timer::single_shot(Duration::ZERO, move || {
                    if let Some(ui) = ui.upgrade() {
                        crate::drag::activate_window(&ui);
                    }
                });
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    // Cancel closes the editor.
    {
        let ew = edit_win.as_weak();
        let main_ui = main_ui.clone();
        edit_win.on_cancel_clicked(move || {
            if let Some(win) = ew.upgrade() {
                let _ = win.hide();
            }
            if let Some(ui) = main_ui.upgrade() {
                ui.set_edit_open(false);
                crate::drag::activate_window(&ui);
            }
        });
    }

    // Save updated clip content: write text-document edits back to the file,
    // otherwise reclassify and update the stored clip.
    {
        let ew = edit_win.as_weak();
        let edit_ui = ui.as_weak();
        let edit_db = db.clone();
        let edit_td = dirs.thumbnails_dir.clone();
        let edit_fd = dirs.favicons_dir.clone();
        let edit_main_ui = main_ui.clone();
        edit_win.on_save_clicked(
            move |id: i32, content: slint::SharedString, tags: slint::SharedString| {
                let db = edit_db.clone();
                let win = ew.clone();
                let ui = edit_ui.clone();
                let td = edit_td.clone();
                let fd = edit_fd.clone();
                let main_ui = edit_main_ui.clone();
                let content = content.to_string();
                let tags = tags.to_string();
                tokio::spawn(async move {
                    // A text-document clip stores its file path in `Content`;
                    // saving means writing the edited text back to that file.
                    let (stored_content, clip_type) = db
                        .with(|conn| {
                            cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64)
                        })
                        .await
                        .map(|(c, t, _)| (c, t))
                        .unwrap_or_default();
                    if clip_type == "file_text" {
                        let write_path = stored_content.clone();
                        let write_content = content.clone();
                        let wrote = tokio::task::spawn_blocking(move || {
                            write_text_file(&write_path, &write_content)
                        })
                        .await;
                        match wrote {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                tracing::error!(
                                    "edit: failed to write edited file {stored_content:?}: {e}"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "edit: file write task for {stored_content:?} panicked: {e}"
                                );
                            }
                        }
                    } else {
                        let normalized = cliptoo_core::content::normalize_line_endings(&content);
                        if let Some(classified) =
                            cliptoo_core::content::ContentProcessor::process(&normalized, false)
                            && let Err(e) = db
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
                                .await
                        {
                            tracing::error!("edit: failed to update clip {id} content: {e:#}");
                        }
                    }
                    if let Err(e) = db
                        .with(|conn| cliptoo_core::db::queries::update_tags(conn, id as i64, &tags))
                        .await
                    {
                        tracing::error!("edit: failed to update clip {id} tags: {e:#}");
                    }
                    helpers::refresh_clips(&db, &ui, &td, &fd, "", "", None).await;
                    let _ = win.upgrade_in_event_loop(move |win| {
                        let _ = win.hide();
                        if let Some(ui) = main_ui.upgrade() {
                            ui.set_edit_open(false);
                            crate::drag::activate_window(&ui);
                        }
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

/// Write edited text back to a text-document file. Separated so the save path
/// can be unit-tested without the window/tokio plumbing.
fn write_text_file(path: &str, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::write_text_file;

    #[test]
    fn writes_text_file_contents() {
        let dir = std::env::temp_dir().join(format!("cliptoo_edit_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.txt");
        write_text_file(path.to_str().unwrap(), "edited text").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited text");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_parent_dir_errors() {
        let path = "/definitely/not/a/real/dir/doc.txt";
        assert!(write_text_file(path, "x").is_err());
    }
}
