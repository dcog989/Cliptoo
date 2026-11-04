// qr.rs — Decode a QR code from an image clip and show the payloads.
//
// Decoding is on demand from the context menu: an image clip costs nothing
// until the user asks for its code. The result window is read-only; Copy uses
// the normal clipboard write path and Open only ever hands http(s) URLs to
// xdg-open.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, SharedString};

use crate::paste::PasteSuppressionSet;

const WINDOW_WIDTH: f32 = 460.0;
const WINDOW_HEIGHT: f32 = 360.0;
/// Delay before raising the result window; the context-menu popup closes right
/// after the action and would otherwise land on top of it.
const RAISE_DELAY: Duration = Duration::from_millis(100);

pub fn setup_qr(
    ui: &crate::AppWindow,
    qr_slot: &crate::lazy_window::LazyWindow<crate::QrWindow>,
    db: &Arc<cliptoo_core::db::DbPool>,
    suppression: &Arc<PasteSuppressionSet>,
    settings: std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    theme_fillers: crate::theme::ThemeFillers,
) {
    let main_ui = ui.as_weak();
    let qr_slot = qr_slot.clone();
    let db = db.clone();
    let suppression = suppression.clone();
    ui.on_decode_qr(move |id: i32| {
        let Some(ui_now) = main_ui.upgrade() else {
            return;
        };
        // Build the result window on the first QR request: most sessions never
        // decode one, so it does not exist at startup.
        let qr_win = qr_slot
            .get_or_create(|| create_qr_window(&ui_now, &settings, &suppression, &theme_fillers));
        let main_ui = ui_now.as_weak();
        let qr_win = qr_win.as_weak();
        let db = db.clone();
        tokio::spawn(async move {
            let row = db
                .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64))
                .await;
            let Ok((content, clip_type, _hash)) = row else {
                return;
            };
            if clip_type != "file_image" {
                return;
            }
            let decoded = tokio::task::spawn_blocking(move || {
                // A multi-selection image clip stores one path per line; decode
                // the first file.
                let first = content.lines().next().unwrap_or(&content);
                cliptoo_core::qr::decode_path(std::path::Path::new(first))
            })
            .await;
            let (codes, error) = match decoded {
                Ok(Ok(codes)) => (codes, None),
                Ok(Err(e)) => (Vec::new(), Some(format!("{e:#}"))),
                Err(e) => (Vec::new(), Some(format!("QR decode task failed: {e}"))),
            };
            let _ = main_ui.upgrade_in_event_loop(move |ui| {
                if let Some(error) = error {
                    tracing::warn!("QR decode failed: {error}");
                    let message = ui.global::<crate::Translations>().get_err_qr_read();
                    crate::actions::show_main_toast(&ui, &message, "error");
                    return;
                }
                if codes.is_empty() {
                    let message = ui.global::<crate::Translations>().get_no_qr_code();
                    crate::actions::show_main_toast(&ui, &message, "info");
                    return;
                }
                show_results(&ui, &qr_win, codes);
            });
        });
    });
}

/// Wire the window's own callbacks and theme. Runs once, on the first decode.
fn create_qr_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    suppression: &Arc<PasteSuppressionSet>,
    theme_fillers: &crate::theme::ThemeFillers,
) -> crate::QrWindow {
    let qr_win = crate::QrWindow::new().expect("QrWindow creation");

    // A lazily created window missed the startup theme fill.
    let (is_dark, system_accent) = crate::theme::cached_resolved_theme();
    crate::theme::fill_theme(
        &qr_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );
    crate::theme::register_qr_theme_filler(theme_fillers, &qr_win.as_weak());

    // Escape and the Close button both route through `closing`, so this is the
    // one place that clears the blur-to-tray guard.
    {
        let main_ui = ui.as_weak();
        qr_win.on_closing(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_qr_open(false);
            }
        });
    }
    {
        let main_ui = ui.as_weak();
        qr_win.window().on_close_requested(move || {
            if let Some(ui) = main_ui.upgrade() {
                ui.set_qr_open(false);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    {
        let suppression = suppression.clone();
        qr_win.on_copy(move |payload: SharedString| {
            let payload = payload.to_string();
            let suppression = suppression.clone();
            tokio::spawn(async move {
                crate::paste::register_suppression(&suppression, &payload, "text");
                if let Err(e) =
                    crate::paste::write_content_to_clipboard(&payload, "text", false).await
                {
                    tracing::error!("QR copy failed: {e:#}");
                }
            });
        });
    }

    qr_win.on_open(|payload: SharedString| {
        // The UI only offers Open for http(s) payloads; re-check here so an
        // arbitrary scheme can never reach xdg-open through this path.
        if payload.starts_with("http://") || payload.starts_with("https://") {
            crate::helpers::open_with_default(payload.as_str());
        }
    });

    qr_win
}

fn show_results(
    ui: &crate::AppWindow,
    qr_win: &slint::Weak<crate::QrWindow>,
    codes: Vec<cliptoo_core::qr::QrCode>,
) {
    let Some(qr_win) = qr_win.upgrade() else {
        return;
    };
    let rows: Vec<crate::QrResult> = codes
        .into_iter()
        .map(|code| crate::QrResult {
            kind: code.kind.into(),
            payload: code.payload.into(),
        })
        .collect();
    qr_win.set_codes(slint::ModelRc::new(slint::VecModel::from(rows)));
    qr_win.window().set_size(slint::LogicalSize {
        width: WINDOW_WIDTH,
        height: WINDOW_HEIGHT,
    });
    ui.set_qr_open(true);
    if qr_win.show().is_err() {
        // A failed show must not leave the blur-to-tray guard stuck on.
        ui.set_qr_open(false);
        return;
    }
    // Qt's show() does not raise; the context-menu popup is still receding, so
    // defer the raise past its close.
    let weak = qr_win.as_weak();
    slint::Timer::single_shot(RAISE_DELAY, move || {
        if let Some(qr_win) = weak.upgrade() {
            crate::drag::activate_window(&qr_win);
        }
    });
}
