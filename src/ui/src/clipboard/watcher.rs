//! Monitor clipboard selection changes over the Wayland data-control protocol.
//!
//! The clipboard listener polls, which means it learns about a copy up to one
//! poll interval late — long enough for the user to switch windows and for a
//! fresh active-window query to report the *new* app as the source. The
//! wlr/ext-data-control protocols make the compositor push a `selection` event
//! the instant the clipboard changes, so the copy moment is known within
//! milliseconds. This module runs that event loop on a dedicated thread and,
//! for every selection change, snapshots the active-window cache (the app the
//! user copied *from*, before any focus switch lands) and hands it to the
//! listener, which then polls the clipboard content immediately.
//!
//! KWin exposes `zwlr_data_control_manager_v1` (and `ext_data_control_v1` on
//! newer Plasma); both are supported here, preferring the `ext` variant.

use std::time::Duration;

use tokio::sync::mpsc;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_device_v1::{
    self as ext_device, ExtDataControlDeviceV1,
};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_manager_v1::ExtDataControlManagerV1;
use wayland_protocols::ext::data_control::v1::client::ext_data_control_offer_v1::ExtDataControlOfferV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
    self as zwlr_device, ZwlrDataControlDeviceV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;

use crate::source_app::ActiveWindowCache;

/// The listener's channel for selection-change notifications. Every message
/// carries the active-window app id snapshotted at the moment the selection
/// changed (`None` when no window was focused then).
pub type SelectionNotifier = mpsc::Sender<Option<String>>;

/// Cadence for re-attempting the data-control connection after it fails to
/// come up or the device is invalidated (e.g. the compositor restarted under
/// us). While the watcher is down, the listener falls back to its poll loop.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Start the data-control watcher on a dedicated thread. The thread retries
/// forever: a missing protocol or a lost connection only downgrades
/// attribution to the listener's poll+cache fallback for one
/// `RECONNECT_DELAY` window at a time, it never permanently kills the watcher.
pub fn spawn_clipboard_watcher(source_app: ActiveWindowCache, notifier: SelectionNotifier) {
    let spawned = std::thread::Builder::new()
        .name(String::from("cliptoo-clipboard-watcher"))
        .spawn(move || {
            loop {
                match run_clipboard_watcher(&source_app, &notifier) {
                    Ok(()) => tracing::debug!("clipboard watcher device invalidated; reconnecting"),
                    Err(e) => tracing::debug!("clipboard watcher unavailable: {e}; retrying"),
                }
                std::thread::sleep(RECONNECT_DELAY);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("failed to spawn clipboard watcher thread: {e}");
    }
}

fn run_clipboard_watcher(
    source_app: &ActiveWindowCache,
    notifier: &SelectionNotifier,
) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<WatcherState>(&conn)?;
    let qh = queue.handle();

    let manager = globals
        .bind::<ExtDataControlManagerV1, WatcherState, ()>(&qh, 1..=1, ())
        .ok()
        .map(Manager::Ext)
        .or_else(|| {
            globals
                .bind::<ZwlrDataControlManagerV1, WatcherState, ()>(&qh, 1..=1, ())
                .ok()
                .map(Manager::Zwlr)
        })
        .ok_or_else(|| anyhow::anyhow!("compositor exposes no ext/wlr-data-control"))?;

    let seat = globals
        .contents()
        .with_list(|list| {
            list.iter()
                .find(|g| g.interface == WlSeat::interface().name)
                .map(|g| {
                    globals
                        .registry()
                        .bind::<WlSeat, (), WatcherState>(g.name, 1, &qh, ())
                })
        })
        .ok_or_else(|| anyhow::anyhow!("no Wayland seat available"))?;

    let device = manager.get_data_device(&seat, &qh);
    let mut state = WatcherState {
        source_app: source_app.clone(),
        notifier: notifier.clone(),
        current_offer: None,
        device,
        dead: false,
        swallow_initial_selection: true,
    };

    while !state.dead {
        queue.blocking_dispatch(&mut state)?;
    }
    Ok(())
}

enum Manager {
    Zwlr(ZwlrDataControlManagerV1),
    Ext(ExtDataControlManagerV1),
}

impl Manager {
    fn get_data_device(&self, seat: &WlSeat, qh: &QueueHandle<WatcherState>) -> Device {
        match self {
            Manager::Zwlr(manager) => Device::Zwlr(manager.get_data_device(seat, qh, ())),
            Manager::Ext(manager) => Device::Ext(manager.get_data_device(seat, qh, ())),
        }
    }
}

enum Device {
    Zwlr(ZwlrDataControlDeviceV1),
    Ext(ExtDataControlDeviceV1),
}

impl Device {
    fn destroy(&self) {
        match self {
            Device::Zwlr(device) => device.destroy(),
            Device::Ext(device) => device.destroy(),
        }
    }
}

enum Offer {
    Zwlr(ZwlrDataControlOfferV1),
    Ext(ExtDataControlOfferV1),
}

impl Offer {
    fn destroy(&self) {
        match self {
            Offer::Zwlr(offer) => offer.destroy(),
            Offer::Ext(offer) => offer.destroy(),
        }
    }
}

struct WatcherState {
    source_app: ActiveWindowCache,
    notifier: SelectionNotifier,
    /// The offer of the current selection, destroyed when replaced or cleared.
    current_offer: Option<Offer>,
    /// Kept alive for the thread's lifetime.
    device: Device,
    /// Set when the compositor invalidates the data-control device; ends the
    /// dispatch loop so the outer run returns and the connection is
    /// re-attempted after a delay.
    dead: bool,
    /// True until the bind-time selection push has been seen; see the
    /// Selection handler for why that one must not be notified.
    swallow_initial_selection: bool,
}

macro_rules! impl_device_dispatch {
    ($device:ty, $offer:ty, $data_offer_opcode:path, $offer_variant:ident) => {
        impl Dispatch<$device, ()> for WatcherState {
            fn event(
                state: &mut Self,
                _proxy: &$device,
                event: <$device as Proxy>::Event,
                _data: &(),
                _conn: &Connection,
                _qhandle: &QueueHandle<Self>,
            ) {
                type Event = <$device as Proxy>::Event;
                match event {
                    Event::DataOffer { id } => {
                        // A selection always opens with a fresh data_offer; the
                        // previous offer is dead from here on.
                        if let Some(offer) = state.current_offer.take() {
                            offer.destroy();
                        }
                        state.current_offer = Some(Offer::$offer_variant(id));
                    }
                    Event::Selection { id } => {
                        // The first selection event on a fresh connection is
                        // the *current* selection pushed at bind (null when
                        // the clipboard is empty) — state, not a copy.
                        // Notifying would make the listener force a changed
                        // generation and re-ingest the pre-existing
                        // clipboard, and the reconnect loop would repeat that
                        // after every compositor restart. Swallow exactly one
                        // selection per connection.
                        if state.swallow_initial_selection {
                            state.swallow_initial_selection = false;
                        } else if id.is_some() {
                            // The referenced offer was introduced by the
                            // preceding data_offer. Snapshot the source app
                            // *now*: this is the instant of the copy, before
                            // any focus switch.
                            let app = state.source_app.read().ok().and_then(|g| g.clone());
                            if let Err(e) = state.notifier.try_send(app) {
                                tracing::debug!("selection notification dropped: {e}");
                            }
                        }
                        if id.is_none() {
                            // Clipboard cleared; the offer it referenced is dead.
                            if let Some(offer) = state.current_offer.take() {
                                offer.destroy();
                            }
                        }
                    }
                    Event::PrimarySelection { id: _ } => {
                        // The primary (middle-click) selection is not tracked;
                        // drop the offer it references so it doesn't leak.
                        if let Some(offer) = state.current_offer.take() {
                            offer.destroy();
                        }
                    }
                    Event::Finished => {
                        if let Some(offer) = state.current_offer.take() {
                            offer.destroy();
                        }
                        state.device.destroy();
                        state.dead = true;
                    }
                    // The event enums are #[non_exhaustive]. Unknown events
                    // can't arrive: this client binds version 1, and protocol
                    // additions always ship behind a version bump.
                    _ => {}
                }
            }

            event_created_child!(WatcherState, $device, [
                $data_offer_opcode => ($offer, ()),
            ]);
        }
    };
}

impl_device_dispatch!(
    ZwlrDataControlDeviceV1,
    ZwlrDataControlOfferV1,
    zwlr_device::EVT_DATA_OFFER_OPCODE,
    Zwlr
);

impl_device_dispatch!(
    ExtDataControlDeviceV1,
    ExtDataControlOfferV1,
    ext_device::EVT_DATA_OFFER_OPCODE,
    Ext
);

impl Dispatch<WlRegistry, GlobalListContents> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlManagerV1,
        _event: <ExtDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlOfferV1,
        _event: <ZwlrDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlOfferV1,
        _event: <ExtDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}
