use zbus::Connection;

/// Asynchronously queries Feral GameMode ClientCount via native D-Bus without spawning child processes.
pub async fn is_gamemode_active(conn: Option<&Connection>) -> bool {
    let Some(conn) = conn else {
        return false;
    };

    if let Ok(proxy) = zbus::Proxy::new(
        conn,
        "com.feralinteractive.GameMode",
        "/com/feralinteractive/GameMode",
        "com.feralinteractive.GameMode",
    )
    .await
    {
        return proxy.get_property::<i32>("ClientCount").await.unwrap_or(0) > 0;
    }

    false
}
