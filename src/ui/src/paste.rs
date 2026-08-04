use anyhow::{Context, Result};
use cliptoo_core::content::hash::{normalize_line_endings, sha256_u64};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wl_clipboard_rs::copy::{ClipboardType, MimeType, Options, Seat, Source};

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
    // When paste-as-plain-text is requested and the clip is RTF, strip RTF markup.
    let effective_content: std::borrow::Cow<str> = if paste_as_plain_text && clip_type == "rtf" {
        std::borrow::Cow::Owned(cliptoo_core::content::strip_rtf(content))
    } else {
        std::borrow::Cow::Borrowed(content)
    };

    let normalized = normalize_line_endings(&effective_content);
    let sup_hash = sha256_u64(&normalized);
    suppression.insert(sup_hash);

    let mime = if paste_as_plain_text {
        MimeType::Text
    } else {
        match clip_type {
            "rtf" => MimeType::Specific("text/rtf".into()),
            _ => MimeType::Text,
        }
    };

    let data = normalized.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut opts = Options::new();
        opts.clipboard(ClipboardType::Regular).seat(Seat::All);
        opts.copy(Source::Bytes(data.into_bytes().into_boxed_slice()), mime)
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

fn simulate_ctrl_v() -> Result<()> {
    let mut device = VirtualDevice::builder()
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
    // first keystroke is not dropped during device setup.
    std::thread::sleep(VIRTUAL_DEVICE_SETTLE);

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
