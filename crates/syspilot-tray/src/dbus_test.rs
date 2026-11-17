use crate::gamemode::{GameModeWatcher, is_gamemode_active};
use crate::sentinels::SentinelStatus;
use crate::spawn_tray_service;
use crate::state::SystemSnapshot;
use crate::tray::SysPilotTray;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use zbus::Connection;

static DBUS_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

const ISOLATED_DBUS_CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

struct IsolatedTestDir {
    path: PathBuf,
}

impl IsolatedTestDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let seq = DBUS_TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("syspilot-dbus-test-{pid}-{nanos}-{seq}"));
        fs::create_dir_all(path.join("state")).expect("create temp state dir");
        fs::create_dir_all(path.join("config")).expect("create temp config dir");
        fs::write(path.join("session.conf"), ISOLATED_DBUS_CONFIG)
            .expect("write isolated dbus config");
        Self { path }
    }
}

impl Drop for IsolatedTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct MockGameMode {
    client_count: Arc<AtomicI32>,
}

#[zbus::interface(name = "com.feralinteractive.GameMode")]
impl MockGameMode {
    #[zbus(property)]
    async fn client_count(&self) -> i32 {
        self.client_count.load(Ordering::Acquire)
    }
}

struct MockStatusNotifierWatcher {
    registered_tx: mpsc::UnboundedSender<String>,
    items: Mutex<Vec<String>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl MockStatusNotifierWatcher {
    async fn register_status_notifier_item(&self, service: &str) -> zbus::fdo::Result<()> {
        self.items
            .lock()
            .expect("watcher items mutex")
            .push(service.to_string());
        let _ = self.registered_tx.send(service.to_string());
        Ok(())
    }

    async fn register_status_notifier_host(&self, _service: &str) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().expect("watcher items mutex").clone()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }
}

async fn wait_for_name_disappearance(conn: &Connection, bus_name: &'static str) {
    let dbus_proxy = zbus::fdo::DBusProxy::new(conn)
        .await
        .expect("DBusProxy init");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let has_owner = dbus_proxy
            .name_has_owner(bus_name.try_into().expect("valid bus name"))
            .await
            .expect("name_has_owner query");
        if !has_owner {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for D-Bus name '{bus_name}' to disappear"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn run_isolated_dbus_suite() {
    let observer_conn = Connection::session()
        .await
        .expect("connect to isolated D-Bus session");

    // Part 1: GameModeWatcher lifecycle (absent -> 0 -> 1 -> 0 -> vanish -> restart)
    let mut watcher = GameModeWatcher::default();

    assert!(!watcher.is_active().await);
    assert!(!watcher.is_active().await);
    assert!(!GameModeWatcher::check_connection(None).await);
    assert!(!is_gamemode_active(Some(&observer_conn)).await);

    let count_1 = Arc::new(AtomicI32::new(0));
    let gm_conn_1 = zbus::connection::Builder::session()
        .expect("session builder")
        .serve_at(
            "/com/feralinteractive/GameMode",
            MockGameMode {
                client_count: Arc::clone(&count_1),
            },
        )
        .expect("serve_at GameMode")
        .name("com.feralinteractive.GameMode")
        .expect("name GameMode")
        .build()
        .await
        .expect("build MockGameMode #1");

    assert!(!watcher.is_active().await);
    assert!(!is_gamemode_active(Some(&observer_conn)).await);

    count_1.store(1, Ordering::Release);
    assert!(watcher.is_active().await);
    assert!(is_gamemode_active(Some(&observer_conn)).await);

    count_1.store(0, Ordering::Release);
    assert!(!watcher.is_active().await);
    assert!(!is_gamemode_active(Some(&observer_conn)).await);

    gm_conn_1.close().await.expect("close MockGameMode #1");
    wait_for_name_disappearance(&observer_conn, "com.feralinteractive.GameMode").await;
    assert!(!watcher.is_active().await);

    let count_2 = Arc::new(AtomicI32::new(2));
    let gm_conn_2 = zbus::connection::Builder::session()
        .expect("session builder")
        .serve_at(
            "/com/feralinteractive/GameMode",
            MockGameMode {
                client_count: Arc::clone(&count_2),
            },
        )
        .expect("serve_at GameMode #2")
        .name("com.feralinteractive.GameMode")
        .expect("name GameMode #2")
        .build()
        .await
        .expect("build MockGameMode #2");

    assert!(watcher.is_active().await);

    let temp_conn = Connection::session().await.expect("temp session");
    let temp_conn_closed = temp_conn.clone();
    temp_conn.close().await.expect("close temp_conn");
    assert!(temp_conn_closed.is_closed());
    assert!(!GameModeWatcher::check_connection(Some(&temp_conn_closed)).await);

    gm_conn_2.close().await.expect("close MockGameMode #2");
    wait_for_name_disappearance(&observer_conn, "com.feralinteractive.GameMode").await;
    assert!(!watcher.is_active().await);

    // Part 2: ksni::Tray vs org.kde.StatusNotifierWatcher (Hyprland/Sway/GNOME)
    let (exit_tx, _exit_rx) = mpsc::channel(1);
    let tray = SysPilotTray::new(
        SystemSnapshot {
            status: "FLIGHT_READY".into(),
            cache_stale: false,
            ..Default::default()
        },
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx,
    );

    let handle = spawn_tray_service(tray)
        .await
        .expect("spawn_tray_service must succeed without StatusNotifierWatcher");
    assert!(!handle.is_closed());

    let (reg_tx_1, mut reg_rx_1) = mpsc::unbounded_channel();
    let watcher_conn_1 = zbus::connection::Builder::session()
        .expect("watcher #1 builder")
        .serve_at(
            "/StatusNotifierWatcher",
            MockStatusNotifierWatcher {
                registered_tx: reg_tx_1,
                items: Mutex::new(Vec::new()),
            },
        )
        .expect("serve_at StatusNotifierWatcher #1")
        .name("org.kde.StatusNotifierWatcher")
        .expect("name StatusNotifierWatcher #1")
        .build()
        .await
        .expect("build StatusNotifierWatcher #1");

    let registered_1 = tokio::time::timeout(Duration::from_secs(2), reg_rx_1.recv())
        .await
        .expect("timed out waiting for SNI registration on Watcher #1")
        .expect("registration channel #1 closed");
    assert!(
        registered_1.contains("StatusNotifierItem"),
        "unexpected SNI service name: {registered_1}"
    );

    watcher_conn_1
        .close()
        .await
        .expect("close StatusNotifierWatcher #1");
    wait_for_name_disappearance(&observer_conn, "org.kde.StatusNotifierWatcher").await;

    let update_res = handle
        .update(|tray_state| {
            tray_state.gamemode_active = true;
        })
        .await;
    assert!(update_res.is_some());
    assert!(!handle.is_closed());

    let (reg_tx_2, mut reg_rx_2) = mpsc::unbounded_channel();
    let watcher_conn_2 = zbus::connection::Builder::session()
        .expect("watcher #2 builder")
        .serve_at(
            "/StatusNotifierWatcher",
            MockStatusNotifierWatcher {
                registered_tx: reg_tx_2,
                items: Mutex::new(Vec::new()),
            },
        )
        .expect("serve_at StatusNotifierWatcher #2")
        .name("org.kde.StatusNotifierWatcher")
        .expect("name StatusNotifierWatcher #2")
        .build()
        .await
        .expect("build StatusNotifierWatcher #2");

    let registered_2 = tokio::time::timeout(Duration::from_secs(2), reg_rx_2.recv())
        .await
        .expect("timed out waiting for SNI re-registration on Watcher #2")
        .expect("registration channel #2 closed");
    assert_eq!(registered_1, registered_2);

    handle.shutdown().await;
    let shutdown_deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while !handle.is_closed() {
        assert!(
            tokio::time::Instant::now() < shutdown_deadline,
            "timed out waiting for ksni tray handle to close after shutdown"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    watcher_conn_2
        .close()
        .await
        .expect("close StatusNotifierWatcher #2");
}

#[test]
fn test_dbus_session_gamemode_and_sni_resilience() {
    if std::env::var_os("SYSPILOT_DBUS_ISOLATED").is_none() {
        let dir = IsolatedTestDir::new();
        let conf_path = dir.path.join("session.conf");
        let exe = std::env::current_exe().expect("resolve current test executable");
        let output = syspilot_core::exec::new_std_command("/usr/bin/dbus-run-session")
            .arg("--config-file")
            .arg(&conf_path)
            .arg("--")
            .arg(&exe)
            .arg("--exact")
            .arg("dbus_test::test_dbus_session_gamemode_and_sni_resilience")
            .arg("--nocapture")
            .env("SYSPILOT_DBUS_ISOLATED", "1")
            .env("XDG_STATE_HOME", dir.path.join("state"))
            .env("XDG_CONFIG_HOME", dir.path.join("config"))
            .output()
            .expect("failed to execute /usr/bin/dbus-run-session");

        assert!(
            output.status.success(),
            "isolated dbus-run-session test failed (status: {:?}):\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current_thread runtime for isolated D-Bus test");
    rt.block_on(run_isolated_dbus_suite());
}
