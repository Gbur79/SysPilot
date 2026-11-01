use std::time::Duration;
use zbus::Connection;

/// Persistent GameMode client maintaining an open D-Bus connection and proxy,
/// supporting lazy reconnection upon daemon restart or broken sockets.
pub struct GameModeWatcher {
    conn: Option<Connection>,
    proxy: Option<zbus::Proxy<'static>>,
}

impl Default for GameModeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl GameModeWatcher {
    pub fn new() -> Self {
        Self {
            conn: None,
            proxy: None,
        }
    }

    /// Attempts lazy reconnection to D-Bus session bus with timeout if disconnected.
    pub async fn ensure_connection(&mut self) -> bool {
        if let Some(conn) = &self.conn {
            if !conn.is_closed() && self.proxy.is_some() {
                return true;
            }
        }

        self.proxy = None;
        self.conn = None;

        let session =
            match tokio::time::timeout(Duration::from_millis(500), Connection::session()).await {
                Ok(Ok(c)) => c,
                _ => return false,
            };

        match zbus::Proxy::new_owned(
            session.clone(),
            "com.feralinteractive.GameMode",
            "/com/feralinteractive/GameMode",
            "com.feralinteractive.GameMode",
        )
        .await
        {
            Ok(proxy) => {
                self.proxy = Some(proxy);
                self.conn = Some(session);
                true
            }
            Err(e) => {
                eprintln!("SysPilot: failed to construct GameMode proxy: {e}");
                self.conn = Some(session);
                false
            }
        }
    }

    /// Queries whether GameMode is currently active (ClientCount > 0).
    /// Differentiates between service absent (returns false) and broken socket (drops proxy/conn for reconnection).
    pub async fn is_active(&mut self) -> bool {
        if !self.ensure_connection().await {
            return false;
        }

        let Some(proxy) = &self.proxy else {
            return false;
        };

        match proxy.get_property::<i32>("ClientCount").await {
            Ok(count) => count > 0,
            Err(err) => {
                // If connection closed or I/O error occurred, invalidate connection to trigger reconnection next tick
                if self.conn.as_ref().is_none_or(|c| c.is_closed())
                    || matches!(err, zbus::Error::InputOutput(_))
                {
                    self.proxy = None;
                    self.conn = None;
                }
                false
            }
        }
    }

    /// Standalone helper for querying an existing connection without retaining proxy state.
    #[allow(dead_code)]
    pub async fn check_connection(conn: Option<&Connection>) -> bool {
        let Some(conn) = conn else {
            return false;
        };
        if conn.is_closed() {
            return false;
        }

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
}

/// Asynchronously queries Feral GameMode ClientCount via native D-Bus without spawning child processes.
#[allow(dead_code)]
pub async fn is_gamemode_active(conn: Option<&Connection>) -> bool {
    GameModeWatcher::check_connection(conn).await
}
