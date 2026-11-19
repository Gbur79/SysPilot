#[cfg(test)]
mod dbus_test;
mod gamemode;
mod sentinels;
mod state;
mod terminal;
#[cfg(test)]
mod tests;
mod tray;
mod updater;

use ksni::{Handle, TrayMethods};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use state::{SystemSnapshot, get_status_path, read_system_snapshot};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tray::SysPilotTray;

pub(crate) enum FsSignal {
    Dirty,
    Fault(String),
}

pub(crate) fn relevant(event: &Event, target: &Path) -> bool {
    matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|path| path == target)
}

pub(crate) fn bounded_error(error: impl ToString) -> String {
    error.to_string().chars().take(240).collect()
}

pub(crate) async fn read_with_retry(path: PathBuf) -> Result<SystemSnapshot, String> {
    let delays = [0, 40, 120];
    let mut last_error = String::new();

    for delay in delays {
        if delay != 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let candidate = path.clone();
        match tokio::task::spawn_blocking(move || read_system_snapshot(&candidate)).await {
            Ok(Ok(snapshot)) => return Ok(snapshot),
            Ok(Err(error)) => last_error = error,
            Err(error) => last_error = format!("snapshot worker failed: {error}"),
        }
    }

    Err(bounded_error(last_error))
}

pub(crate) async fn refresh(
    handle: &Handle<SysPilotTray>,
    path: &Path,
    current: &mut SystemSnapshot,
    last_error: &mut Option<String>,
) -> bool {
    match read_with_retry(path.to_path_buf()).await {
        Ok(snapshot) => {
            let changed = *current != snapshot || last_error.is_some();
            *current = snapshot.clone();
            *last_error = None;

            if changed {
                handle
                    .update(move |tray| {
                        apply_tray_snapshot_update(tray, snapshot);
                    })
                    .await
                    .is_some()
            } else {
                !handle.is_closed()
            }
        }
        Err(error) => {
            if last_error.as_deref() == Some(error.as_str()) {
                return !handle.is_closed();
            }

            eprintln!("SysPilot: {error}");
            *last_error = Some(error.clone());
            current.source_error = Some(error.clone());
            let failed_snapshot = current.clone();
            handle
                .update(move |tray| tray.snapshot = failed_snapshot)
                .await
                .is_some()
        }
    }
}

pub(crate) fn apply_tray_snapshot_update(tray: &mut SysPilotTray, snapshot: SystemSnapshot) {
    tray.snapshot = snapshot;
    if tray.snapshot.updates_check_state == syspilot_core::UpdateCheckState::Ok {
        if tray.snapshot.updates_total == 0 {
            tray.updates = None;
        } else if let Some(u) = tray.updates.as_mut() {
            u.core_count = tray.snapshot.core_updates;
            u.regular_count = tray.snapshot.regular_updates;
            u.aur_count = tray.snapshot.aur_updates;
            u.check_state = tray.snapshot.updates_check_state;
            u.standalone_count = tray.snapshot.standalone_updates;
            u.standalone_tools = tray.snapshot.standalone_tools.clone();
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

pub struct TriageGuard(pub Arc<AtomicBool>);

impl Drop for TriageGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub fn try_acquire_triage_lock(in_flight: &Arc<AtomicBool>) -> Option<TriageGuard> {
    if in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        Some(TriageGuard(in_flight.clone()))
    } else {
        None
    }
}

#[inline]
pub(crate) fn trim_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        let _ = libc::malloc_trim(0);
    }
}

pub(crate) fn try_spawn_triage_with<F, Fut>(
    in_flight: &Arc<AtomicBool>,
    check_pkgs: bool,
    gamemode_active: bool,
    runner: F,
) -> bool
where
    F: FnOnce(bool, bool) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    if gamemode_active {
        return false;
    }
    let Some(guard) = try_acquire_triage_lock(in_flight) else {
        return false;
    };

    tokio::spawn(async move {
        let _guard = guard;
        runner(check_pkgs, gamemode_active).await;
    });
    true
}

pub(crate) async fn spawn_tray_service(
    tray: SysPilotTray,
) -> Result<Handle<SysPilotTray>, ksni::Error> {
    tray.assume_sni_available(true).spawn().await
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::getuid() == 0 } {
        eprintln!("FATAL: SysPilot tray daemon must not be executed as root (UID 0).");
        std::process::exit(1);
    }

    if !syspilot_core::acquire_single_instance_lock(".tray.lock") {
        return Ok(());
    }

    let status_path = get_status_path();
    let mut gamemode_watcher = gamemode::GameModeWatcher::new();
    let sentinels_state = sentinels::collect_sentinels();
    let gamemode_active = gamemode_watcher.is_active().await;

    let mut state = build_initial_state(&status_path, sentinels_state.clone(), gamemode_active);

    let (exit_tx, exit_rx) = mpsc::channel(1);
    let tray = SysPilotTray::new(
        state.snapshot.clone(),
        sentinels_state,
        None,
        gamemode_active,
        terminal::resolve_syspilot(),
        exit_tx,
    );
    let handle = spawn_tray_service(tray).await?;

    let (fs_tx, fs_rx) = mpsc::channel(8);
    let watcher = StatusWatcher::new(status_path.clone(), fs_tx.clone());
    let triage_in_flight = Arc::new(AtomicBool::new(false));
    let trigger_triage: TriageTrigger = Arc::new(move |check_pkgs, gm, tx| {
        try_spawn_triage_with(
            &triage_in_flight,
            check_pkgs,
            gm,
            move |pkgs, gm_flag| async move {
                let _ = syspilot_core::run_triage(pkgs, gm_flag).await;
                trim_heap();
                let _ = tx.try_send(FsSignal::Dirty);
            },
        )
    });

    trigger_startup_triage(&trigger_triage, &mut state, &fs_tx);

    let mut rt = DaemonRuntime {
        gamemode_watcher,
        exit_rx,
        fs_tx,
        fs_rx,
        watcher,
        collect_sentinels: sentinels::collect_sentinels,
        trigger_triage,
    };

    run_daemon_loop(
        DaemonConfig::production(status_path),
        &handle,
        &mut state,
        &mut rt,
        shutdown_signal(),
    )
    .await;

    drop(rt);
    let _ = shutdown_tray_handle(&handle, Duration::from_secs(2)).await;
    Ok(())
}

pub(crate) fn build_initial_state(
    status_path: &Path,
    sentinels_state: sentinels::SentinelStatus,
    gamemode_active: bool,
) -> DaemonState {
    let (snapshot, last_error) = match read_system_snapshot(status_path) {
        Ok(snapshot) => (snapshot, None),
        Err(error) => {
            let error = bounded_error(error);
            let snap = SystemSnapshot {
                source_error: Some(error.clone()),
                ..Default::default()
            };
            (snap, Some(error))
        }
    };
    DaemonState {
        snapshot,
        last_error,
        sentinels_state,
        gamemode_active,
        pending_pkg_check: false,
    }
}

pub(crate) fn trigger_startup_triage(
    trigger: &TriageTrigger,
    state: &mut DaemonState,
    fs_tx: &mpsc::Sender<FsSignal>,
) {
    state.pending_pkg_check = !trigger(true, state.gamemode_active, fs_tx.clone());
}

pub(crate) fn dispatch_notify_result(
    result: notify::Result<Event>,
    target: &Path,
    fs_tx: &mpsc::Sender<FsSignal>,
) {
    match result {
        Ok(event) if relevant(&event, target) => {
            let _ = fs_tx.try_send(FsSignal::Dirty);
        }
        Err(error) => {
            if let Err(send_err) = fs_tx.try_send(FsSignal::Fault(bounded_error(error))) {
                eprintln!("SysPilot: status watcher fault signal dropped: {send_err}");
            }
        }
        _ => {}
    }
}

fn build_watcher(target: PathBuf, fs_tx: mpsc::Sender<FsSignal>) -> Option<RecommendedWatcher> {
    RecommendedWatcher::new(
        move |result| dispatch_notify_result(result, &target, &fs_tx),
        notify::Config::default(),
    )
    .map_err(|error| eprintln!("SysPilot: watcher unavailable; polling fallback active: {error}"))
    .ok()
}

pub(crate) struct StatusWatcher {
    watcher: Option<RecommendedWatcher>,
    parent: Option<PathBuf>,
    watching: bool,
    target: PathBuf,
    fs_tx: mpsc::Sender<FsSignal>,
}

impl StatusWatcher {
    pub(crate) fn new(target: PathBuf, fs_tx: mpsc::Sender<FsSignal>) -> Self {
        let mut sw = Self {
            watcher: None,
            parent: target.parent().map(Path::to_path_buf),
            watching: false,
            target,
            fs_tx,
        };
        sw.ensure_watching();
        sw
    }

    #[cfg(test)]
    pub(crate) fn is_watching(&self) -> bool {
        self.watching
    }

    pub(crate) fn ensure_watching(&mut self) -> bool {
        if self.watching {
            return true;
        }
        let Some(parent) = self.parent.as_deref() else {
            return false;
        };
        if !parent.is_dir() {
            return false;
        }
        if self.watcher.is_none() {
            self.watcher = build_watcher(self.target.clone(), self.fs_tx.clone());
        }
        let Some(watcher) = self.watcher.as_mut() else {
            return false;
        };
        if watcher.watch(parent, RecursiveMode::NonRecursive).is_ok() {
            self.watching = true;
        } else {
            self.watcher = None;
        }
        self.watching
    }

    pub(crate) fn handle_fault(&mut self, error: &str) {
        if self.watching {
            if let (Some(watcher), Some(parent)) = (self.watcher.as_mut(), self.parent.as_deref()) {
                let _ = watcher.unwatch(parent);
            }
            self.watching = false;
        }
        self.watcher = None;
        eprintln!("SysPilot: filesystem watcher error: {error}");
    }
}

pub(crate) fn drain_fs_signals(fs_rx: &mut mpsc::Receiver<FsSignal>) -> Option<String> {
    let mut fault = None;
    while let Ok(sig) = fs_rx.try_recv() {
        if let FsSignal::Fault(err) = sig {
            fault = Some(err);
        }
    }
    fault
}

pub(crate) fn evaluate_sentinel_transition(
    prev: &sentinels::SentinelStatus,
    next: &sentinels::SentinelStatus,
) -> (bool, bool) {
    let changed = next != prev;
    let pacman_unlocked = prev.pacman_locked && !next.pacman_locked;
    let pacman_db_changed = next.pacman_db_mtime != prev.pacman_db_mtime;
    let summary_changed = next.summary_mtime != prev.summary_mtime;
    (
        changed,
        pacman_unlocked || pacman_db_changed || summary_changed,
    )
}

pub(crate) async fn shutdown_tray_handle(
    handle: &Handle<SysPilotTray>,
    timeout_dur: Duration,
) -> bool {
    if tokio::time::timeout(timeout_dur, handle.shutdown())
        .await
        .is_err()
    {
        eprintln!(
            "SysPilot: handle shutdown timed out after {}s (D-Bus teardown race); exiting",
            timeout_dur.as_secs()
        );
        return false;
    }
    true
}

pub(crate) fn make_interval(period: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval
}

pub(crate) type TriageTrigger =
    Arc<dyn Fn(bool, bool, mpsc::Sender<FsSignal>) -> bool + Send + Sync>;

pub(crate) struct DaemonConfig {
    pub status_path: PathBuf,
    pub poll_interval: Duration,
    pub gamemode_interval: Duration,
    pub sentinels_interval: Duration,
    pub updater_interval: Duration,
    pub triage_interval: Duration,
    pub fs_debounce: Duration,
}

impl DaemonConfig {
    fn production(status_path: PathBuf) -> Self {
        Self {
            status_path,
            poll_interval: Duration::from_secs(30),
            gamemode_interval: Duration::from_secs(3),
            sentinels_interval: Duration::from_secs(10),
            updater_interval: Duration::from_secs(3600),
            triage_interval: Duration::from_secs(900),
            fs_debounce: Duration::from_millis(100),
        }
    }
}

pub(crate) struct DaemonState {
    pub snapshot: SystemSnapshot,
    pub last_error: Option<String>,
    pub sentinels_state: sentinels::SentinelStatus,
    pub gamemode_active: bool,
    pub pending_pkg_check: bool,
}

pub(crate) struct DaemonRuntime {
    pub gamemode_watcher: gamemode::GameModeWatcher,
    pub exit_rx: mpsc::Receiver<()>,
    pub fs_tx: mpsc::Sender<FsSignal>,
    pub fs_rx: mpsc::Receiver<FsSignal>,
    pub watcher: StatusWatcher,
    pub collect_sentinels: fn() -> sentinels::SentinelStatus,
    pub trigger_triage: TriageTrigger,
}

pub(crate) async fn run_daemon_loop(
    config: DaemonConfig,
    handle: &Handle<SysPilotTray>,
    state: &mut DaemonState,
    rt: &mut DaemonRuntime,
    shutdown: impl Future<Output = ()>,
) {
    let mut poll = make_interval(config.poll_interval);
    let mut gamemode_timer = make_interval(config.gamemode_interval);
    let mut sentinels_timer = make_interval(config.sentinels_interval);
    let mut updater_timer = make_interval(config.updater_interval);
    let mut triage_timer = make_interval(config.triage_interval);

    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                eprintln!("SysPilot: shutdown signal received, terminating tray loop");
                break;
            }
            _ = rt.exit_rx.recv() => {
                eprintln!("SysPilot: user exit requested via tray menu");
                break;
            }
            _ = triage_timer.tick() => {
                let _ = (rt.trigger_triage)(false, state.gamemode_active, rt.fs_tx.clone());
            }
            Some(signal) = rt.fs_rx.recv() => match signal {
                FsSignal::Dirty => {
                    tokio::time::sleep(config.fs_debounce).await;
                    if let Some(err) = drain_fs_signals(&mut rt.fs_rx) {
                        rt.watcher.handle_fault(&err);
                    }
                    if !refresh(
                        handle,
                        &config.status_path,
                        &mut state.snapshot,
                        &mut state.last_error,
                    ).await {
                        eprintln!("SysPilot: handle closed during status refresh; exiting");
                        break;
                    }
                    if state.pending_pkg_check
                        && !state.gamemode_active
                        && (rt.trigger_triage)(true, false, rt.fs_tx.clone())
                    {
                        state.pending_pkg_check = false;
                    }
                }
                FsSignal::Fault(error) => {
                    rt.watcher.handle_fault(&error);
                    if !refresh(
                        handle,
                        &config.status_path,
                        &mut state.snapshot,
                        &mut state.last_error,
                    ).await {
                        eprintln!("SysPilot: handle closed during fault recovery refresh; exiting");
                        break;
                    }
                }
            },
            _ = poll.tick() => {
                rt.watcher.ensure_watching();
                if !refresh(
                    handle,
                    &config.status_path,
                    &mut state.snapshot,
                    &mut state.last_error,
                ).await {
                    eprintln!("SysPilot: handle closed during periodic poll; exiting");
                    break;
                }
            }
            _ = gamemode_timer.tick() => {
                let active = rt.gamemode_watcher.is_active().await;
                if active != state.gamemode_active {
                    let was_active = state.gamemode_active;
                    state.gamemode_active = active;
                    if handle
                        .update(move |tray| tray.gamemode_active = active)
                        .await
                        .is_none()
                    {
                        eprintln!("SysPilot: handle closed during gamemode update; exiting");
                        break;
                    }
                    if was_active && !active {
                        let check_pkgs = state.pending_pkg_check;
                        if (rt.trigger_triage)(check_pkgs, false, rt.fs_tx.clone()) && check_pkgs {
                            state.pending_pkg_check = false;
                        }
                    }
                }
            }
            _ = sentinels_timer.tick() => {
                let current_sentinels = (rt.collect_sentinels)();
                let (changed, needs_pkg_triage) =
                    evaluate_sentinel_transition(&state.sentinels_state, &current_sentinels);
                if changed {
                    state.sentinels_state = current_sentinels.clone();
                    if handle
                        .update(move |tray| tray.sentinels = current_sentinels)
                        .await
                        .is_none()
                    {
                        eprintln!("SysPilot: handle closed during sentinels update; exiting");
                        break;
                    }
                }
                if needs_pkg_triage || (state.pending_pkg_check && !state.gamemode_active) {
                    state.pending_pkg_check =
                        !(rt.trigger_triage)(true, state.gamemode_active, rt.fs_tx.clone());
                }
            }
            _ = updater_timer.tick() => {
                state.pending_pkg_check =
                    !(rt.trigger_triage)(true, state.gamemode_active, rt.fs_tx.clone());
            }
        }

        if handle.is_closed() {
            eprintln!("SysPilot: tray handle closed; exiting event loop");
            break;
        }
    }
}
