// System tray via StatusNotifierItem protocol using ksni.
//
// TODO: Replace with Slint's native system tray support once
// https://github.com/slint-ui/slint/pull/11574 lands in a release.
// When that's available the whole ksni dependency and this module can go.

use anyhow::Result;
use ksni::TrayMethods;
use std::sync::OnceLock;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub enum TrayAction {
    ToggleWindow,
    Quit,
}

pub(crate) struct CliptooTray {
    action_tx: UnboundedSender<TrayAction>,
}

fn tray_icon() -> Vec<ksni::Icon> {
    static ICON: OnceLock<Vec<ksni::Icon>> = OnceLock::new();
    ICON.get_or_init(|| {
        let svg = include_bytes!("../assets/cliptoo.svg");
        if let Ok((mut rgba, w, h)) = cliptoo_core::icon::rasterize_svg(svg, 48) {
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.rotate_right(1);
            }
            vec![ksni::Icon {
                width: w as i32,
                height: h as i32,
                data: rgba,
            }]
        } else {
            vec![]
        }
    })
    .clone()
}

impl ksni::Tray for CliptooTray {
    fn id(&self) -> String {
        "cliptoo".into()
    }

    fn title(&self) -> String {
        "Cliptoo".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        tray_icon()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Cliptoo — Clipboard Manager".into(),
            description: String::new(),
            icon_name: String::new(),
            icon_pixmap: vec![],
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let toggle_tx = self.action_tx.clone();
        let quit_tx = self.action_tx.clone();
        vec![
            ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: "Show / Hide".into(),
                activate: Box::new(move |_: &mut Self| {
                    let _ = toggle_tx.send(TrayAction::ToggleWindow);
                }),
                ..Default::default()
            }),
            ksni::MenuItem::Separator,
            ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_: &mut Self| {
                    let _ = quit_tx.send(TrayAction::Quit);
                }),
                ..Default::default()
            }),
        ]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.action_tx.send(TrayAction::ToggleWindow);
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        let _ = self.action_tx.send(TrayAction::ToggleWindow);
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        tracing::info!("tray watcher offline: {reason:?}, keeping alive");
        true
    }
}

pub(crate) async fn create_tray(
    action_tx: UnboundedSender<TrayAction>,
) -> Result<ksni::Handle<CliptooTray>> {
    let handle = CliptooTray { action_tx }.spawn().await?;
    Ok(handle)
}
