use anyhow::{Context, Result};
use cliptoo_core::content::hash::{normalize_line_endings, sha256_u64};
use cliptoo_core::db::models::ClipType;
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wl_clipboard_rs::copy::{ClipboardType, MimeSource, MimeType, Options, Seat, Source};

/// How long a paste-suppression hash stays valid after `insert`. Must
/// comfortably exceed the clipboard listener's poll interval (500ms in
/// `clipboard/listener.rs`): if the entry expires before the listener reads
/// back our own paste, the content is re-ingested as a new clip. The multi-poll
/// margin also covers the listener being mid-ingest of a large clip when the
/// paste lands.
const SUPPRESSION_TTL: Duration = Duration::from_secs(2);

pub struct PasteSuppressionSet {
    inner: Arc<Mutex<HashSet<u64>>>,
    handle: tokio::runtime::Handle,
}

impl PasteSuppressionSet {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashSet::new())),
            handle: tokio::runtime::Handle::current(),
        }
    }

    pub fn insert(&self, hash: u64) {
        let set = self.inner.clone();
        {
            let mut guard = set.lock().expect("PasteSuppressionSet lock");
            guard.insert(hash);
        }
        let set2 = set.clone();
        self.handle.spawn(async move {
            tokio::time::sleep(SUPPRESSION_TTL).await;
            let mut guard = set2.lock().expect("PasteSuppressionSet lock");
            guard.remove(&hash);
        });
    }

    pub fn check_and_remove(&self, hash: u64) -> bool {
        let mut guard = self.inner.lock().expect("PasteSuppressionSet lock");
        guard.remove(&hash)
    }
}

pub async fn paste_content(
    content: &str,
    clip_type: &str,
    suppression: &PasteSuppressionSet,
    window: &slint::Weak<crate::AppWindow>,
    paste_as_plain_text: bool,
) -> Result<()> {
    register_suppression(suppression, content, clip_type);

    write_content_to_clipboard(content, clip_type, !paste_as_plain_text).await?;

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let _ = window.upgrade_in_event_loop(move |ui| {
        crate::window::hide_window(&ui);
        let _ = tx.send(());
    });
    rx.await.ok();

    tokio::time::sleep(Duration::from_millis(80)).await;

    simulate_ctrl_v().context("input simulation")?;

    Ok(())
}

/// Mark a clip's payload as self-originated so the clipboard listener does not
/// re-ingest it as a new clip. The listener polls text/rtf and text/html
/// before text/plain, so after a rich write it re-reads the raw markup, while
/// after a plain-text write it re-reads the stripped text. Register both
/// hashes so either payload is recognised as our own. (For non-rich content
/// the raw hash covers the single text/plain payload.)
pub fn register_suppression(suppression: &PasteSuppressionSet, content: &str, clip_type: &str) {
    suppression.insert(sha256_u64(&normalize_line_endings(content)));
    if clip_type == "rtf" {
        let stripped = cliptoo_core::content::strip_rtf(content);
        suppression.insert(sha256_u64(&normalize_line_endings(&stripped)));
    } else if clip_type == "html" {
        let stripped = cliptoo_core::content::strip_html(content);
        suppression.insert(sha256_u64(&normalize_line_endings(&stripped)));
    }
}

/// Write a clip's payload to the regular Wayland clipboard without pasting it.
/// `offer_rich` controls whether an RTF/HTML clip advertises the raw `text/rtf`
/// or `text/html` MIME type in addition to the stripped `text/plain` fallback.
pub async fn write_content_to_clipboard(
    content: &str,
    clip_type: &str,
    offer_rich: bool,
) -> Result<()> {
    let is_file_clip = ClipType::parse(clip_type).is_file_clip();
    let is_rtf = clip_type == "rtf";
    let is_html = clip_type == "html";
    let is_rich = is_rtf || is_html;

    // The text/plain offer is the stripped rich text for RTF/HTML clips, the
    // content itself otherwise.
    let plain_text: std::borrow::Cow<str> = if is_rtf {
        std::borrow::Cow::Owned(cliptoo_core::content::strip_rtf(content))
    } else if is_html {
        std::borrow::Cow::Owned(cliptoo_core::content::strip_html(content))
    } else {
        std::borrow::Cow::Borrowed(content)
    };

    let normalized = normalize_line_endings(&plain_text);

    let data = normalized.clone();
    // `content` is a borrowed &str and cannot move into the 'static blocking
    // closure, so pre-normalize the raw rich payload here.
    let rich_raw = if is_rich && offer_rich {
        Some(normalize_line_endings(content))
    } else {
        None
    };
    let rich_mime = if is_rtf { "text/rtf" } else { "text/html" };

    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut opts = Options::new();
        opts.clipboard(ClipboardType::Regular).seat(Seat::All);
        if is_file_clip {
            // File write: uri-list plus the decoded paths as text/plain.
            let uri_list = build_uri_list(&data);
            opts.copy_multi(vec![
                MimeSource {
                    source: Source::Bytes(uri_list.into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Specific("text/uri-list".into()),
                },
                MimeSource {
                    source: Source::Bytes(data.into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Text,
                },
            ])
        } else if let Some(raw) = rich_raw {
            // Rich-text write: offer the raw markup (formatted) plus the
            // stripped text/plain fallback so targets without rich support
            // still insert the text instead of nothing.
            opts.copy_multi(vec![
                MimeSource {
                    source: Source::Bytes(raw.into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Specific(rich_mime.into()),
                },
                MimeSource {
                    source: Source::Bytes(data.into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Text,
                },
            ])
        } else {
            opts.copy(
                Source::Bytes(data.into_bytes().into_boxed_slice()),
                MimeType::Text,
            )
        }
        .map_err(|e| anyhow::anyhow!("clipboard write: {e}"))
    })
    .await
    .context("spawn clipboard write")??;

    Ok(())
}

/// Build a `text/uri-list` payload from decoded newline-joined paths (the
/// storage format of file clips). Each path becomes a percent-encoded `file://`
/// URI on its own line; the list ends with a trailing CRLF as the freedesktop
/// clipboard spec requires.
fn build_uri_list(decoded_paths: &str) -> String {
    decoded_paths
        .lines()
        .filter_map(|p| url::Url::from_file_path(std::path::Path::new(p)).ok())
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

/// Injects Ctrl+V through a virtual uinput keyboard device.
///
/// A spawned `wtype` is not viable here: KWin does not advertise the wlr
/// virtual-keyboard protocol, so wtype can never bind. uinput is compositor-
/// agnostic and works through the device node's logind uaccess ACL. This is
/// the same mechanism ydotool uses, without an external binary.
const DEVICE_NAME: &str = "cliptoo virtual keyboard";
const VIRTUAL_DEVICE_SETTLE: Duration = Duration::from_millis(50);
const KEY_STRESS_DELAY: Duration = Duration::from_millis(10);
const PRESSED: i32 = 1;
const RELEASED: i32 = 0;

/// Lazily-created, persistent uinput device — built once on the first paste
/// and reused for every subsequent one. Previously a new `VirtualDevice` was
/// created and destroyed on every paste, paying the 50ms compositor hotplug
/// settle delay each time and causing KWin to see an input device appear and
/// disappear on every single paste.
static VIRTUAL_KEYBOARD: Mutex<Option<VirtualDevice>> = Mutex::new(None);

fn create_virtual_keyboard() -> Result<VirtualDevice> {
    let device = VirtualDevice::builder()
        .context("open /dev/uinput")?
        .name(DEVICE_NAME)
        .with_keys(
            &[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V]
                .into_iter()
                .collect::<AttributeSet<KeyCode>>(),
        )
        .context("configure uinput keyboard capabilities")?
        .build()
        .context("create uinput keyboard device")?;

    // Let the compositor hotplug the new input device before emitting, so the
    // first keystroke is not dropped during device setup. Paid once, at
    // first-paste time, since the device is now created lazily and reused.
    std::thread::sleep(VIRTUAL_DEVICE_SETTLE);

    Ok(device)
}

fn simulate_ctrl_v() -> Result<()> {
    let mut guard = VIRTUAL_KEYBOARD
        .lock()
        .expect("virtual keyboard mutex poisoned");
    if guard.is_none() {
        *guard = Some(create_virtual_keyboard()?);
    }
    let device = guard.as_mut().expect("just initialized above");

    device
        .emit(&[
            key_event(KeyCode::KEY_LEFTCTRL, PRESSED),
            key_event(KeyCode::KEY_V, PRESSED),
        ])
        .context("inject Ctrl+V key press")?;

    // Separate press from release so the target application registers a
    // distinct keydown/keyup rather than a simultaneous event.
    std::thread::sleep(KEY_STRESS_DELAY);

    device
        .emit(&[
            key_event(KeyCode::KEY_V, RELEASED),
            key_event(KeyCode::KEY_LEFTCTRL, RELEASED),
        ])
        .context("inject Ctrl+V key release")?;

    Ok(())
}

fn key_event(key: KeyCode, value: i32) -> InputEvent {
    InputEvent::new(EventType::KEY.0, key.code(), value)
}
