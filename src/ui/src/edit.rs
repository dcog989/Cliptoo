use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use slint::ComponentHandle;

use crate::helpers;

thread_local! {
    /// File mtime/size captured when a clip was opened, with the clip id. Kept
    /// out of Slint properties because `float` is f32 there and cannot
    /// represent an epoch mtime precisely enough to detect an external edit.
    /// One editor window exists, so one entry is enough.
    static FILE_SNAPSHOT: Cell<Option<(i32, f64, u64)>> = const { Cell::new(None) };
}

/// Record the file snapshot for an open clip (UI thread only).
pub fn remember_file_snapshot(id: i32, mtime: f64, size: u64) {
    FILE_SNAPSHOT.with(|s| s.set(Some((id, mtime, size))));
}

/// Drop a snapshot (clip is not a readable file, or the editor was reopened).
pub fn forget_file_snapshot() {
    FILE_SNAPSHOT.with(|s| s.set(None));
}

fn file_snapshot(id: i32) -> Option<(f64, u64)> {
    FILE_SNAPSHOT.with(|s| match s.get() {
        Some((open_id, mtime, size)) if open_id == id => Some((mtime, size)),
        _ => None,
    })
}

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
    theme_fillers: &crate::theme::ThemeFillers,
) -> crate::EditWindow {
    let edit_win = crate::EditWindow::new().expect("EditWindow creation");

    // A lazily created window missed the startup theme fill.
    let (is_dark, system_accent) = crate::theme::cached_resolved_theme();
    crate::theme::fill_theme(
        &edit_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );
    crate::theme::register_edit_theme_filler(theme_fillers, &edit_win.as_weak());

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
                // Snapshot taken when the clip was opened; Save only rewrites
                // the file when it was actually loaded and has not changed.
                let file_snapshot = file_snapshot(id);
                let db = edit_db.clone();
                let win = ew.clone();
                let ui = edit_ui.clone();
                let td = edit_td.clone();
                let fd = edit_fd.clone();
                let main_ui = edit_main_ui.clone();
                let content = content.to_string();
                let tags = tags.to_string();
                let view = helpers::current_view_state(&ui);
                let pfx = tag_prefix.clone();
                tokio::spawn(async move {
                    // Save the edited content. Any failure keeps the editor open
                    // with a toast — a silent close would convince the user their
                    // edits persisted. `file_saved` separates "the file on disk
                    // is already rewritten" from "nothing was saved".
                    let mut file_saved = false;
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
                        // `file_document` covers text-based formats (markdown,
                        // rst, …) and binary ones (pdf, docx, …). Only write the
                        // edited text back when the editor actually loaded this
                        // regular file; a deleted, binary or oversized file keeps
                        // the path view, and Save must not recreate it with the
                        // path text as its contents.
                        let write_back = file_snapshot.is_some()
                            && cliptoo_core::db::models::ClipType::parse(&clip_type).is_text_file();
                        if write_back {
                            let (file_mtime, file_size) =
                                file_snapshot.expect("write_back requires a snapshot");
                            let check_path = stored_content.clone();
                            let unchanged = tokio::task::spawn_blocking(move || {
                                file_unchanged(&check_path, file_mtime, file_size)
                            })
                            .await
                            .context("file-check task panicked")?;
                            if !unchanged {
                                anyhow::bail!(
                                    "the file changed on disk since it was opened; reopen the clip"
                                );
                            }
                            let write_path = stored_content.clone();
                            let write_content = content.clone();
                            let wrote = tokio::task::spawn_blocking(move || {
                                write_text_file(&write_path, &write_content)
                            })
                            .await
                            .context("file-write task panicked")?;
                            let (new_mtime, new_size) = wrote.context("writing edited file")?;
                            file_saved = true;
                            // Replace the snapshot with the file we just wrote,
                            // so a retry after a later step fails (or a second
                            // Save) compares against the current file, not the
                            // one from before the write.
                            let _ = win.upgrade_in_event_loop(move |_| {
                                remember_file_snapshot(id, new_mtime, new_size);
                            });
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
                            // Keep file and folder clips file clips: their payload
                            // is still paths, and classifying it as text alone
                            // would demote them to file_path and drop the
                            // text/uri-list paste rendition.
                            let was_file_clip =
                                cliptoo_core::db::models::ClipType::parse(&clip_type)
                                    .is_file_clip();
                            let classified = cliptoo_core::content::ContentProcessor::process(
                                &normalized,
                                was_file_clip,
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
                        // user can retry or cancel. When the file write already
                        // succeeded, say so: the remaining failure is only the
                        // index/tag update.
                        let _ = win.upgrade_in_event_loop(move |win| {
                            let global = win.global::<crate::Translations>();
                            let (message, severity) = if file_saved {
                                (global.get_err_save_file_index(), "warn")
                            } else {
                                (global.get_err_save_clip(), "error")
                            };
                            win.set_toast_message(message);
                            win.set_toast_severity(severity.into());
                            win.set_toast_visible(true);
                        });
                        return;
                    }

                    helpers::refresh_clips(&db, &ui, &td, &fd, &view, Some(&pfx)).await;
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

/// Write edited text back to a text-document file via a temp file in the same
/// directory, so a crash mid-write cannot truncate the user's document. The
/// original permissions are carried over (a 0600 note must not become 0644).
/// Returns the written file's mtime/size so the save path can refresh its
/// snapshot. Separated so the save path can be unit-tested without the
/// window/tokio plumbing.
fn write_text_file(path: &str, contents: &str) -> std::io::Result<(f64, u64)> {
    use std::os::unix::fs::OpenOptionsExt;

    // Write through a symlink (dotfile setups) instead of replacing the link
    // with a regular file. Falls back to the given path when it is gone.
    let original = Path::new(path);
    let target = std::fs::canonicalize(original).unwrap_or_else(|_| original.to_path_buf());
    let path = target.as_path();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("cliptoo-edit");
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(".{name}.cliptoo-{}.tmp", std::process::id()));

    // `create_new` refuses to follow an existing symlink or reuse someone
    // else's file at this predictable name (the temp lives in the clip's
    // directory, which can be shared like /tmp). The 0600 mode keeps the text
    // private until the original permissions are copied over.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let tmp_file = options.open(&tmp).or_else(|e| {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(e);
        }
        // A temp left by a crashed save, or a planted link. Unlink never
        // follows the link, so removing it cannot touch its target.
        std::fs::remove_file(&tmp)?;
        options.open(&tmp)
    });
    let written = tmp_file.and_then(|mut file| {
        std::io::Write::write_all(&mut file, contents.as_bytes())?;
        // Flush the data before the rename: otherwise a power cut can leave
        // the new name pointing at a zero-length file.
        file.sync_all()?;
        let meta = file.metadata()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0.0, |d| d.as_secs_f64());
        Ok((mtime, meta.len()))
    });
    let (mtime, size) = match written {
        Ok(written) => written,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok((mtime, size))
}

/// Whether the file still has the size and mtime it had when the editor loaded
/// it, so Save does not silently overwrite an edit made in another program.
fn file_unchanged(path: &str, mtime: f64, size: u64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() != size {
        return false;
    }
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        == Some(mtime)
}

#[cfg(test)]
mod tests {
    use super::{file_unchanged, write_text_file};

    #[test]
    fn writes_text_file_contents() {
        let dir = std::env::temp_dir().join(format!("cliptoo_edit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.txt");
        write_text_file(path.to_str().unwrap(), "edited text").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited text");
        // No temp file is left behind.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_parent_dir_errors() {
        let path = "/definitely/not/a/real/dir/doc.txt";
        assert!(write_text_file(path, "x").is_err());
    }

    #[test]
    fn keeps_the_original_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("cliptoo_edit_perms_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.txt");
        write_text_file(path.to_str().unwrap(), "one").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_text_file(path.to_str().unwrap(), "two").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640, "the original permissions must be carried over");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_a_file_changed_after_the_snapshot() {
        let dir = std::env::temp_dir().join(format!("cliptoo_edit_mtime_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.txt");
        let (mtime, size) = write_text_file(path.to_str().unwrap(), "one").unwrap();
        assert!(file_unchanged(path.to_str().unwrap(), mtime, size));
        write_text_file(path.to_str().unwrap(), "a longer edit").unwrap();
        assert!(!file_unchanged(path.to_str().unwrap(), mtime, size));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
