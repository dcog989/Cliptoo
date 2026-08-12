use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use slint::ComponentHandle;

use crate::helpers;

/// Persist the editor window's current size to settings. Called on every close
/// path — WM close, Cancel, and Save — because save/cancel hide the window
/// without firing `close_requested`, which would otherwise drop any resize
/// made while the editor was open.
fn save_editor_size(
    win: &crate::EditWindow,
    settings: &std::cell::RefCell<cliptoo_core::Settings>,
    settings_path: &Path,
) {
    let size = win.window().size();
    let mut s = settings.borrow_mut();
    s.editor_window_width = size.width as f64;
    s.editor_window_height = size.height as f64;
    let _ = s.save(settings_path);
}

pub fn setup_edit_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    db: &Arc<cliptoo_core::db::DbPool>,
    tag_prefix: &str,
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
                // A WM close with unsaved edits arms the two-step Cancel (it
                // becomes "Discard?") instead of silently losing them.
                if win.get_dirty() && !win.get_confirm_cancel() {
                    win.set_confirm_cancel(true);
                    return slint::CloseRequestResponse::KeepWindowShown;
                }
                save_editor_size(&win, &s, &p);
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

    // Cancel closes the editor. The two-step discard guard lives in the Slint
    // button (first click with `dirty` re-arms as "Discard?"); by the time this
    // callback fires the user has confirmed.
    {
        let ew = edit_win.as_weak();
        let s = settings.clone();
        let p = dirs.settings_path.clone();
        let main_ui = main_ui.clone();
        edit_win.on_cancel_clicked(move || {
            if let Some(win) = ew.upgrade() {
                save_editor_size(&win, &s, &p);
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
        let edit_settings = settings.clone();
        let edit_settings_path = dirs.settings_path.clone();
        let tag_prefix = tag_prefix.to_string();
        edit_win.on_save_clicked(
            move |id: i32, content: slint::SharedString, tags: slint::SharedString| {
                // Persist the editor size on this close path too: the hide()
                // below never fires close_requested, so a save would otherwise
                // drop any resize made while the editor was open.
                if let Some(win) = ew.upgrade() {
                    save_editor_size(&win, &edit_settings, &edit_settings_path);
                }
                let db = edit_db.clone();
                let win = ew.clone();
                let ui = edit_ui.clone();
                let td = edit_td.clone();
                let fd = edit_fd.clone();
                let main_ui = edit_main_ui.clone();
                let content = content.to_string();
                let tags = tags.to_string();
                let (query, filter) = helpers::current_view_state(&ui);
                let pfx = tag_prefix.clone();
                tokio::spawn(async move {
                    // Save the edited content. Any failure aborts the save and
                    // keeps the editor open with an error toast — a silent
                    // close would convince the user their edits persisted.
                    let result: anyhow::Result<()> = async {
                        // A text-document clip stores its file path in `Content`;
                        // saving means writing the edited text back to that file.
                        let (stored_content, clip_type) = db
                            .with(|conn| {
                                cliptoo_core::db::queries::get_clip_type_and_content(
                                    conn, id as i64,
                                )
                            })
                            .await
                            .map(|(c, t, _)| (c, t))
                            .context("loading clip")?;
                        if clip_type == "file_text" {
                            let write_path = stored_content.clone();
                            let write_content = content.clone();
                            let wrote = tokio::task::spawn_blocking(move || {
                                write_text_file(&write_path, &write_content)
                            })
                            .await
                            .context("file-write task panicked")?;
                            wrote.context("writing edited file")?;
                            // Refresh the file-derived metadata so the stored
                            // row reflects the rewritten file's size and shape;
                            // Content/PreviewContent stay the path.
                            db.with(|conn| {
                                cliptoo_core::db::queries::update_clip_metadata(
                                    conn,
                                    id as i64,
                                    content.len() as i64,
                                    content.contains('\n'),
                                )
                            })
                            .await
                            .context("saving clip metadata")?;
                        } else {
                            let normalized =
                                cliptoo_core::content::normalize_line_endings(&content);
                            let classified = cliptoo_core::content::ContentProcessor::process(
                                &normalized,
                                false,
                            )
                            .context("content is empty")?;
                            db.with(|conn| {
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
                            .context("saving clip content")?;
                        }
                        db.with(|conn| {
                            cliptoo_core::db::queries::update_tags(conn, id as i64, &tags)
                        })
                        .await
                        .context("saving tags")?;
                        Ok(())
                    }
                    .await;

                    if let Err(e) = result {
                        tracing::error!("edit: failed to save clip {id}: {e:#}");
                        // Keep the window open so nothing is silently lost; the
                        // user can fix the problem and retry, or cancel.
                        let _ = win.upgrade_in_event_loop(move |win| {
                            win.set_toast_message(format!("Save failed: {e:#}").into());
                            win.set_toast_severity("error".into());
                            win.set_toast_visible(true);
                        });
                        return;
                    }

                    helpers::refresh_clips(&db, &ui, &td, &fd, &query, &filter, Some(&pfx)).await;
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
