use tracing::debug;

pub async fn detect_source_app() -> Option<String> {
    try_kwin_active_window().await
}

async fn try_kwin_active_window() -> Option<String> {
    let conn = zbus::Connection::session().await.ok()?;

    let msg = conn
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "activeWindow",
            &(),
        )
        .await
        .ok()?;

    let window_id: u32 = msg.body().deserialize().ok()?;

    let info_msg = conn
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "getWindowInfo",
            &(window_id,),
        )
        .await
        .ok()?;

    let body = info_msg.body();
    let val: zbus::zvariant::Value = body.deserialize().ok()?;
    let fields = match &val {
        zbus::zvariant::Value::Structure(s) => s.fields(),
        _ => return None,
    };

    for &idx in &[4, 12, 5] {
        if let Some(zbus::zvariant::Value::Str(s)) = fields.get(idx) {
            let app_id = s.as_str();
            if !app_id.is_empty() && app_id != "0" {
                debug!("detected source app: {app_id}");
                return Some(app_id.to_string());
            }
        }
    }

    None
}
