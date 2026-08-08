use anyhow::{Context, Result};
use cliptoo_core::content::hash::{normalize_line_endings, sha256_u64};
use cliptoo_core::db::models::ClipType;
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wl_clipboard_rs::copy::{ClipboardType, MimeSource, MimeType, Options, Seat, Source};

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
            tokio::time::sleep(Duration::from_millis(500)).await;
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
    let is_file_clip = ClipType::parse(clip_type).is_file_clip();
    let is_rtf = clip_type == "rtf";

    // The text/plain offer is the stripped RTF text for RTF clips, the content
    // itself otherwise.
    let plain_text: std::borrow::Cow<str> = if is_rtf {
        std::borrow::Cow::Owned(cliptoo_core::content::strip_rtf(content))
    } else {
        std::borrow::Cow::Borrowed(content)
    };

    let normalized = normalize_line_endings(&plain_text);

    // The listener polls text/rtf before text/plain, so after a rich paste it
    // re-reads the raw RTF markup, while after a plain-text paste it re-reads
    // the stripped text. Register both hashes so either payload is recognised
    // as our own paste and not re-ingested. (For non-RTF content the raw hash
    // covers the single text/plain payload.)
    suppression.insert(sha256_u64(&normalize_line_endings(content)));
    if is_rtf {
        suppression.insert(sha256_u64(&normalized));
    }

    let data = normalized.clone();
    // `content` is a borrowed &str and cannot move into the 'static blocking
    // closure, so pre-normalize the raw RTF payload here.
    let rich_rtf = if is_rtf && !paste_as_plain_text {
        Some(normalize_line_endings(content))
    } else {
        None
    };

    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut opts = Options::new();
        opts.clipboard(ClipboardType::Regular).seat(Seat::All);
        if is_file_clip {
            // File paste: uri-list plus the decoded paths as text/plain.
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
        } else if let Some(raw) = rich_rtf {
            // Rich-text paste: offer the raw RTF (formatted) plus the stripped
            // text/plain fallback so targets without RTF support still insert
            // the text instead of nothing.
            opts.copy_multi(vec![
                MimeSource {
                    source: Source::Bytes(raw.into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Specific("text/rtf".into()),
                },
                MimeSource {
                    source: Source::Bytes(data.into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Text,
                },
            ])
        } else {
            opts.copy(Source::Bytes(data.into_bytes().into_boxed_slice()), MimeType::Text)
        }
        .map_err(|e| anyhow::anyhow!("clipboard write: {e}"))
    })
    .await
    .context("spawn clipboard write")??;

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

/// Build a `text/uri-list` payload from decoded newline-joined paths (the
/// storage format of file clips). Each path becomes a percent-encoded `file://`
/// URI on its own line; the list ends with a trailing CRLF as the freedesktop
/// clipboard spec requires.
fn build_uri_list(decoded_paths: &str) -> String {
    decoded_paths
        .lines()
        .map(|p| format!("file://{}", cliptoo_core::content::percent_encode_path(p)))
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
