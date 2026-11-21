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

/// `ksni 0.3.7` resolves `Handle::shutdown().await` right before `service::run` exits and drops
/// `handle_rx`, so `handle.is_closed()` can lag by one scheduler poll under `current_thread`.
async fn wait_for_handle_closed<T>(handle: &ksni::Handle<T>, context: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while !handle.is_closed() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for ksni tray handle to close ({context})"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
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
    wait_for_handle_closed(&handle, "Part 2 handle.shutdown()").await;
    watcher_conn_2
        .close()
        .await
        .expect("close StatusNotifierWatcher #2");

    run_refresh_and_daemon_loop_isolated_tests(&observer_conn).await;
}

static MOCK_LOCKED: AtomicBoolCompat = AtomicBoolCompat::new(false);

struct AtomicBoolCompat(std::sync::atomic::AtomicBool);

impl AtomicBoolCompat {
    const fn new(v: bool) -> Self {
        Self(std::sync::atomic::AtomicBool::new(v))
    }
    fn store(&self, v: bool) {
        self.0.store(v, Ordering::Release);
    }
    fn load(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

fn mock_collect_sentinels() -> SentinelStatus {
    SentinelStatus {
        pacman_locked: MOCK_LOCKED.load(),
        ..Default::default()
    }
}

fn sample_status_json(status: &str, total: u32, core: u32, reg: u32) -> String {
    format!(
        r#"{{
  "schema_version": "1.0",
  "status": "{status}",
  "gaming_mode": false,
  "reboot_pending": false,
  "updates": {{
    "total": {total},
    "core_count": {core},
    "regular_count": {reg},
    "aur_count": 0
  }}
}}"#
    )
}

async fn run_refresh_and_daemon_loop_isolated_tests(observer_conn: &Connection) {
    // Part 3: refresh() state matrix + shutdown_tray_handle()
    let refresh_dir = IsolatedTestDir::new();
    let refresh_status_path = refresh_dir.path.join("state/status.json");
    fs::write(
        &refresh_status_path,
        sample_status_json("FLIGHT_READY", 2, 1, 1),
    )
    .expect("write initial refresh status");

    let (exit_tx_r, _exit_rx_r) = mpsc::channel(1);
    let tray_r = SysPilotTray::new(
        SystemSnapshot::default(),
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx_r,
    );
    let handle_r = spawn_tray_service(tray_r)
        .await
        .expect("spawn tray for refresh tests");

    let mut cur = SystemSnapshot::default();
    let mut last_err: Option<String> = None;

    assert!(crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    assert_eq!(cur.status, "FLIGHT_READY");
    assert!(last_err.is_none());

    // Unchanged snapshot with no error
    assert!(crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);

    // Unchanged snapshot with stale last_err (tests || vs && in refresh)
    last_err = Some("stale error".into());
    assert!(crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    assert!(last_err.is_none());

    // Missing file -> error recorded
    fs::remove_file(&refresh_status_path).expect("remove status for error path");
    assert!(crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    let first_err = last_err.clone().expect("last_err must be set");
    assert_eq!(cur.source_error.as_deref(), Some(first_err.as_str()));

    // Repeat identical error -> early return path
    assert!(crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);

    // Different error -> updates last_err
    fs::write(&refresh_status_path, "{invalid json").expect("write invalid json");
    assert!(crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    assert_ne!(last_err.as_deref(), Some(first_err.as_str()));

    // shutdown_tray_handle on open handle and already-closed handle
    assert!(crate::shutdown_tray_handle(&handle_r, Duration::from_secs(2)).await);
    wait_for_handle_closed(&handle_r, "Part 3 shutdown_tray_handle()").await;
    assert!(crate::shutdown_tray_handle(&handle_r, Duration::from_secs(2)).await);

    // Closed handle causes refresh to return false across all branches
    assert!(!crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    fs::write(&refresh_status_path, "{other_invalid").expect("write other invalid json");
    assert!(!crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    fs::write(
        &refresh_status_path,
        sample_status_json("ALL_CLEAR", 0, 0, 0),
    )
    .expect("write valid json for closed handle");
    assert!(!crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);
    assert!(!crate::refresh(&handle_r, &refresh_status_path, &mut cur, &mut last_err).await);

    // Part 4: Full run_daemon_loop end-to-end with MockGameMode, late parent dir + inotify, sentinels, pending_pkg_check
    let loop_dir = IsolatedTestDir::new();
    let late_parent = loop_dir.path.join("late_state");
    let loop_status_path = late_parent.join("status.json");

    let gm_count = Arc::new(AtomicI32::new(0));
    let gm_conn = zbus::connection::Builder::session()
        .expect("loop gm builder")
        .serve_at(
            "/com/feralinteractive/GameMode",
            MockGameMode {
                client_count: Arc::clone(&gm_count),
            },
        )
        .expect("serve_at GameMode for loop")
        .name("com.feralinteractive.GameMode")
        .expect("name GameMode for loop")
        .build()
        .await
        .expect("build MockGameMode for loop");

    let (exit_tx, exit_rx) = mpsc::channel(1);
    let tray_loop = SysPilotTray::new(
        SystemSnapshot::default(),
        SentinelStatus::default(),
        None,
        false,
        None,
        exit_tx.clone(),
    );
    let handle_loop = spawn_tray_service(tray_loop)
        .await
        .expect("spawn tray for daemon loop");

    let (fs_tx, fs_rx) = mpsc::channel(8);
    let watcher = crate::StatusWatcher::new(loop_status_path.clone(), fs_tx.clone());
    assert!(!watcher.is_watching());

    let allow_triage = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let triage_calls = Arc::new(Mutex::new(Vec::<(bool, bool)>::new()));
    let allow_clone = Arc::clone(&allow_triage);
    let calls_clone = Arc::clone(&triage_calls);
    let trigger_triage: crate::TriageTrigger = Arc::new(move |check_pkgs, gm, _tx| {
        if !allow_clone.load(Ordering::Acquire) {
            return false;
        }
        calls_clone
            .lock()
            .expect("triage calls mutex")
            .push((check_pkgs, gm));
        true
    });

    let mut state = crate::DaemonState {
        snapshot: SystemSnapshot::default(),
        last_error: None,
        sentinels_state: SentinelStatus::default(),
        gamemode_active: false,
        pending_pkg_check: false,
    };
    let mut rt = crate::DaemonRuntime {
        gamemode_watcher: GameModeWatcher::new(),
        exit_rx,
        fs_tx: fs_tx.clone(),
        fs_rx,
        watcher,
        collect_sentinels: mock_collect_sentinels,
        trigger_triage,
    };

    let gm_count_drv = Arc::clone(&gm_count);
    let late_parent_drv = late_parent.clone();
    let loop_status_drv = loop_status_path.clone();
    let fs_tx_drv = fs_tx.clone();
    let allow_drv = Arc::clone(&allow_triage);
    let exit_tx_drv = exit_tx.clone();

    let driver = tokio::spawn(async move {
        // Wait for updater_timer + triage_timer while allow_triage=false -> latches pending_pkg_check=true
        tokio::time::sleep(Duration::from_millis(80)).await;
        // Inject FsSignal::Fault before directory exists
        let _ = fs_tx_drv.try_send(crate::FsSignal::Fault("synthetic fault".into()));
        tokio::time::sleep(Duration::from_millis(30)).await;
        // Create late_parent and write status.json, then wait for poll to attach inotify
        fs::create_dir_all(&late_parent_drv).expect("create late_parent");
        fs::write(
            &loop_status_drv,
            sample_status_json("FLIGHT_READY", 1, 1, 0),
        )
        .expect("write loop status #1");
        tokio::time::sleep(Duration::from_millis(70)).await;

        // Enable triage and toggle GameMode 0 -> 1 -> 0 (flushes pending_pkg_check=true!)
        allow_drv.store(true, Ordering::Release);
        gm_count_drv.store(1, Ordering::Release);
        tokio::time::sleep(Duration::from_millis(50)).await;
        gm_count_drv.store(0, Ordering::Release);
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Toggle sentinel pacman_locked false -> true -> false (triggers check_pkgs=true)
        MOCK_LOCKED.store(true);
        tokio::time::sleep(Duration::from_millis(50)).await;
        MOCK_LOCKED.store(false);
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Trigger inotify Dirty via real file write
        fs::write(
            &loop_status_drv,
            sample_status_json("PRE_FLIGHT_ATTENTION", 3, 1, 2),
        )
        .expect("write loop status #2");
        tokio::time::sleep(Duration::from_millis(50)).await;

        let _ = exit_tx_drv.send(()).await;
    });

    crate::run_daemon_loop(
        crate::DaemonConfig {
            status_path: loop_status_path.clone(),
            poll_interval: Duration::from_millis(40),
            gamemode_interval: Duration::from_millis(25),
            sentinels_interval: Duration::from_millis(25),
            updater_interval: Duration::from_millis(60),
            triage_interval: Duration::from_millis(50),
            fs_debounce: Duration::from_millis(10),
        },
        &handle_loop,
        &mut state,
        &mut rt,
        std::future::pending::<()>(),
    )
    .await;

    driver.await.expect("driver task");
    assert_eq!(state.snapshot.status, "PRE_FLIGHT_ATTENTION");
    assert_eq!(state.snapshot.updates_total, 3);
    assert!(!state.pending_pkg_check);
    assert!(rt.watcher.is_watching());
    let recorded = triage_calls.lock().expect("recorded calls").clone();
    assert!(recorded.iter().any(|(pkgs, gm)| *pkgs && !*gm));

    drop(rt);
    assert!(crate::shutdown_tray_handle(&handle_loop, Duration::from_secs(2)).await);
    gm_conn.close().await.expect("close MockGameMode for loop");
    wait_for_name_disappearance(observer_conn, "com.feralinteractive.GameMode").await;

    // Part 5: run_daemon_loop shutdown signal exit + external handle close exit
    for exit_via_handle in [false, true] {
        let (_etx, erx) = mpsc::channel(1);
        let t = SysPilotTray::new(
            SystemSnapshot::default(),
            SentinelStatus::default(),
            None,
            false,
            None,
            _etx,
        );
        let h = spawn_tray_service(t)
            .await
            .expect("spawn tray for exit test");
        let (ftx, frx) = mpsc::channel(8);
        let w = crate::StatusWatcher::new(loop_status_path.clone(), ftx.clone());
        let mut st = crate::DaemonState {
            snapshot: SystemSnapshot::default(),
            last_error: None,
            sentinels_state: SentinelStatus::default(),
            gamemode_active: false,
            pending_pkg_check: false,
        };
        let mut runtime = crate::DaemonRuntime {
            gamemode_watcher: GameModeWatcher::new(),
            exit_rx: erx,
            fs_tx: ftx,
            fs_rx: frx,
            watcher: w,
            collect_sentinels: mock_collect_sentinels,
            trigger_triage: Arc::new(|_, _, _| true),
        };
        let cfg = crate::DaemonConfig {
            status_path: loop_status_path.clone(),
            poll_interval: Duration::from_millis(20),
            gamemode_interval: Duration::from_secs(60),
            sentinels_interval: Duration::from_secs(60),
            updater_interval: Duration::from_secs(60),
            triage_interval: Duration::from_secs(60),
            fs_debounce: Duration::from_millis(5),
        };
        if exit_via_handle {
            let h_clone = h.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(15)).await;
                h_clone.shutdown().await;
            });
            crate::run_daemon_loop(cfg, &h, &mut st, &mut runtime, std::future::pending::<()>())
                .await;
            assert!(h.is_closed());
        } else {
            crate::run_daemon_loop(cfg, &h, &mut st, &mut runtime, std::future::ready(())).await;
            assert!(crate::shutdown_tray_handle(&h, Duration::from_secs(2)).await);
        }
    }
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
