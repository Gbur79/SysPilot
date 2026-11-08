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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tray::SysPilotTray;

enum FsSignal {
    Dirty,
    Fault(String),
}

fn relevant(event: &Event, target: &Path) -> bool {
    matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|path| path == target)
}

fn bounded_error(error: impl ToString) -> String {
    error.to_string().chars().take(240).collect()
}

async fn read_with_retry(path: PathBuf) -> Result<SystemSnapshot, String> {
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

async fn refresh(
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
                        tray.snapshot = snapshot;
                        if tray.snapshot.updates_total == 0 {
                            tray.updates = None;
                        }
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

fn try_spawn_triage(in_flight: &Arc<AtomicBool>, fast: bool, force_refresh: bool) -> bool {
    let Some(guard) = try_acquire_triage_lock(in_flight) else {
        return false;
    };

    tokio::spawn(async move {
        let _guard = guard;
        let _ = syspilot_core::run_triage(fast, force_refresh).await;
    });
    true
}

fn try_spawn_triage_and_updater(in_flight: &Arc<AtomicBool>, handle: Handle<SysPilotTray>) -> bool {
    let Some(guard) = try_acquire_triage_lock(in_flight) else {
        return false;
    };

    tokio::spawn(async move {
        let _guard = guard;
        let _ = syspilot_core::run_triage(true, false).await;
        if let Some(updates) = updater::run_full_update_check(false).await {
            let _ = handle
                .update(move |tray| tray.updates = Some(updates))
                .await;
        }
    });
    true
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
    let (mut snapshot, mut last_error) = match read_system_snapshot(&status_path) {
        Ok(snapshot) => (snapshot, None),
        Err(error) => {
            let error = bounded_error(error);
            (
                SystemSnapshot {
                    source_error: Some(error.clone()),
                    ..Default::default()
                },
                Some(error),
            )
        }
    };

    let mut gamemode_watcher = gamemode::GameModeWatcher::new();
    let mut sentinels_state = sentinels::collect_sentinels();
    let mut gamemode_active = gamemode_watcher.is_active().await;

    // Run initial background update check if not in gaming session
    let initial_updates = updater::run_full_update_check(gamemode_active).await;

    // Atomic deduplication lock preventing overlapping background triage runs
    let triage_in_flight = Arc::new(AtomicBool::new(false));

    // Trigger initial background triage unconditionally on startup (cold boot or restart)
    // as long as not in an active gaming session, ensuring telemetry is immediately fresh.
    if !gamemode_active {
        try_spawn_triage(&triage_in_flight, true, false);
    }

    let (exit_tx, mut exit_rx) = mpsc::channel(1);
    let tray = SysPilotTray::new(
        snapshot.clone(),
        sentinels_state.clone(),
        initial_updates,
        gamemode_active,
        terminal::resolve_syspilot(),
        exit_tx,
    );
    let handle = tray.spawn().await?;

    let (fs_tx, mut fs_rx) = mpsc::channel(1);
    let _fs_tx_guard = fs_tx.clone();
    let target = status_path.clone();
    let mut watcher: Option<RecommendedWatcher> = RecommendedWatcher::new(
        move |result: notify::Result<Event>| match result {
            Ok(event) if relevant(&event, &target) => {
                let _ = fs_tx.try_send(FsSignal::Dirty);
            }
            Err(error) => {
                let _ = fs_tx.try_send(FsSignal::Fault(bounded_error(error)));
            }
            _ => {}
        },
        notify::Config::default(),
    )
    .map_err(|error| eprintln!("SysPilot: watcher unavailable; polling fallback active: {error}"))
    .ok();

    let parent = status_path.parent().map(Path::to_path_buf);
    let mut watching = false;
    if let (Some(watcher), Some(parent)) = (watcher.as_mut(), parent.as_deref()) {
        if parent.is_dir() {
            watching = watcher.watch(parent, RecursiveMode::NonRecursive).is_ok();
        }
    }

    let mut poll = tokio::time::interval(Duration::from_secs(30));
    let mut gamemode_timer = tokio::time::interval(Duration::from_secs(3));
    let mut sentinels_timer = tokio::time::interval(Duration::from_secs(10));
    let mut updater_timer = tokio::time::interval(Duration::from_secs(3600));
    let mut triage_timer = tokio::time::interval(Duration::from_secs(900));

    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    gamemode_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    sentinels_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    updater_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    triage_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                eprintln!("SysPilot: shutdown signal received, terminating tray loop");
                break;
            }
            _ = exit_rx.recv() => {
                eprintln!("SysPilot: user exit requested via tray menu");
                break;
            }
            _ = triage_timer.tick() => {
                if !gamemode_active {
                    try_spawn_triage(&triage_in_flight, false, false);
                }
            }
            signal = fs_rx.recv() => match signal {
                Some(FsSignal::Dirty) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    while fs_rx.try_recv().is_ok() {}
                    if !refresh(
                        &handle,
                        &status_path,
                        &mut snapshot,
                        &mut last_error,
                    ).await {
                        eprintln!("SysPilot: handle closed during status refresh; exiting");
                        break;
                    }
                }
                Some(FsSignal::Fault(error)) => {
                    if watching {
                        if let (Some(watcher), Some(parent)) =
                            (watcher.as_mut(), parent.as_deref())
                        {
                            let _ = watcher.unwatch(parent);
                        }
                        watching = false;
                    }
                    eprintln!("SysPilot: filesystem watcher error: {error}");
                }
                None => {}
            },
            _ = poll.tick() => {
                if !watching {
                    if let (Some(watcher), Some(parent)) =
                        (watcher.as_mut(), parent.as_deref())
                    {
                        if parent.is_dir() {
                            let _ = watcher.unwatch(parent);
                            watching = watcher
                                .watch(parent, RecursiveMode::NonRecursive)
                                .is_ok();
                        }
                    }
                }
                if !refresh(
                    &handle,
                    &status_path,
                    &mut snapshot,
                    &mut last_error,
                ).await {
                    eprintln!("SysPilot: handle closed during periodic poll; exiting");
                    break;
                }
            }
            _ = gamemode_timer.tick() => {
                let active = gamemode_watcher.is_active().await;
                if active != gamemode_active {
                    let was_active = gamemode_active;
                    gamemode_active = active;
                    if handle
                        .update(move |tray| tray.gamemode_active = active)
                        .await
                        .is_none()
                    {
                        eprintln!("SysPilot: handle closed during gamemode update; exiting");
                        break;
                    }
                    if was_active && !active {
                        try_spawn_triage(&triage_in_flight, false, false);
                    }
                }
            }
            _ = sentinels_timer.tick() => {
                let current_sentinels = sentinels::collect_sentinels();
                let pacman_unlocked = sentinels_state.pacman_locked && !current_sentinels.pacman_locked;
                let pacman_db_changed = current_sentinels.pacman_db_mtime != sentinels_state.pacman_db_mtime;
                let summary_changed = current_sentinels.summary_mtime != sentinels_state.summary_mtime;
                if current_sentinels != sentinels_state {
                    sentinels_state = current_sentinels.clone();
                    let update_sentinels = current_sentinels.clone();
                    if handle
                        .update(move |tray| tray.sentinels = update_sentinels)
                        .await
                        .is_none()
                    {
                        eprintln!("SysPilot: handle closed during sentinels update; exiting");
                        break;
                    }
                }
                if (pacman_unlocked || pacman_db_changed || summary_changed) && !gamemode_active {
                    try_spawn_triage_and_updater(&triage_in_flight, handle.clone());
                }
            }
            _ = updater_timer.tick() => {
                if let Some(updates) = updater::run_full_update_check(gamemode_active).await {
                    if handle
                        .update(move |tray| tray.updates = Some(updates))
                        .await
                        .is_none()
                    {
                        eprintln!("SysPilot: handle closed during updater update; exiting");
                        break;
                    }
                    if !gamemode_active {
                        try_spawn_triage(&triage_in_flight, true, false);
                    }
                }
            }
        }

        if handle.is_closed() {
            eprintln!("SysPilot: tray handle closed; exiting event loop");
            break;
        }
    }

    // Explicit cleanup of inotify watcher descriptor
    if let (Some(mut watcher), Some(parent)) = (watcher.take(), parent.as_deref()) {
        if watching {
            let _ = watcher.unwatch(parent);
        }
    }

    if !handle.is_closed()
        && tokio::time::timeout(Duration::from_secs(2), handle.shutdown())
            .await
            .is_err()
    {
        eprintln!("SysPilot: handle shutdown timed out after 2s (D-Bus teardown race); exiting");
    }
    Ok(())
}
