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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let dbus_conn = zbus::Connection::session().await.ok();
    let mut sentinels_state = sentinels::collect_sentinels();
    let mut gamemode_active = gamemode::is_gamemode_active(dbus_conn.as_ref()).await;

    // Run initial background update check if not in gaming session
    let initial_updates = updater::run_full_update_check(gamemode_active).await;

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
        watching = watcher.watch(parent, RecursiveMode::NonRecursive).is_ok();
    }

    let mut poll = tokio::time::interval(Duration::from_secs(30));
    let mut gamemode_timer = tokio::time::interval(Duration::from_secs(3));
    let mut sentinels_timer = tokio::time::interval(Duration::from_secs(10));
    let mut updater_timer = tokio::time::interval(Duration::from_secs(3600));

    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    gamemode_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    sentinels_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    updater_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = exit_rx.recv() => break,
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
                        break;
                    }
                }
                Some(FsSignal::Fault(error)) => {
                    watching = false;
                    eprintln!("SysPilot: filesystem watcher error: {error}");
                }
                None => {}
            },
            _ = poll.tick() => {
                if !watching {
                    if let (Some(watcher), Some(parent)) =
                        (watcher.as_mut(), parent.as_deref())
                    {
                        watching = watcher
                            .watch(parent, RecursiveMode::NonRecursive)
                            .is_ok();
                    }
                }
                if !refresh(
                    &handle,
                    &status_path,
                    &mut snapshot,
                    &mut last_error,
                ).await {
                    break;
                }
            }
            _ = gamemode_timer.tick() => {
                let active = gamemode::is_gamemode_active(dbus_conn.as_ref()).await;
                if active != gamemode_active {
                    gamemode_active = active;
                    if handle
                        .update(move |tray| tray.gamemode_active = active)
                        .await
                        .is_none()
                    {
                        break;
                    }
                }
            }
            _ = sentinels_timer.tick() => {
                let current_sentinels = sentinels::collect_sentinels();
                let pacman_unlocked = sentinels_state.pacman_locked && !current_sentinels.pacman_locked;
                if current_sentinels != sentinels_state {
                    sentinels_state = current_sentinels.clone();
                    let update_sentinels = current_sentinels.clone();
                    if handle
                        .update(move |tray| tray.sentinels = update_sentinels)
                        .await
                        .is_none()
                    {
                        break;
                    }
                }
                if pacman_unlocked && !gamemode_active {
                    let handle_clone = handle.clone();
                    tokio::spawn(async move {
                        if let Some(updates) = updater::run_full_update_check(false).await {
                            let _ = handle_clone.update(move |tray| tray.updates = Some(updates)).await;
                        }
                    });
                }
            }
            _ = updater_timer.tick() => {
                if let Some(updates) = updater::run_full_update_check(gamemode_active).await {
                    if handle
                        .update(move |tray| tray.updates = Some(updates))
                        .await
                        .is_none()
                    {
                        break;
                    }
                }
            }
        }

        if handle.is_closed() {
            break;
        }
    }

    if !handle.is_closed() {
        handle.shutdown().await;
    }
    Ok(())
}
