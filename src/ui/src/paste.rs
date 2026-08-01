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
        std::borrow::Cow::Owned(strip_rtf(content))
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

/// Remove RTF markup, returning the plain-text content.
///
/// Algorithm: iterate characters, skip everything inside `{...}` groups whose
/// first token starts with `\\`, pass through characters outside groups that
/// are not RTF control words.  Handles the common subset produced by office
/// applications; does not attempt full RTF spec compliance.
fn strip_rtf(rtf: &str) -> String {
    let mut out = String::with_capacity(rtf.len());
    let mut depth: u32 = 0;
    let mut skip_group = false;
    let mut skip_stack: Vec<bool> = Vec::new();
    let bytes = rtf.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                skip_stack.push(skip_group);
                // Peek: if next non-space char is '\\', this is a control group.
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                skip_group = j < bytes.len() && bytes[j] == b'\\';
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                skip_group = skip_stack.pop().unwrap_or(false);
                i += 1;
            }
            b'\\' => {
                i += 1;
                // Escaped char literal (e.g. \\{ \\} \\\\)
                if i < bytes.len() && (bytes[i] == b'{' || bytes[i] == b'}' || bytes[i] == b'\\') {
                    if !skip_group && depth > 0 {
                        out.push(bytes[i] as char);
                    }
                    i += 1;
                    continue;
                }
                // Control word: letters then optional signed integer
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &rtf[start..i];
                // Optional numeric parameter
                let _neg = i < bytes.len() && bytes[i] == b'-';
                if _neg {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                // Trailing space delimiter is consumed
                if i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if !skip_group {
                    match word {
                        "par" | "line" => out.push('\n'),
                        "tab" => out.push('\t'),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => {
                i += 1;
            }
            ch => {
                if !skip_group && depth > 0 {
                    out.push(ch as char);
                }
                i += 1;
            }
        }
    }
    // Collapse runs of blank lines
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
