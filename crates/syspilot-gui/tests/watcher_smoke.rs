use std::fs::{self, File};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

#[test]
fn test_watcher_mtime_lifecycle_and_shutdown() {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("syspilot-gui-test-{nanos}"));
    fs::create_dir_all(&tmp_dir).unwrap();
    let status_path = tmp_dir.join("status.json");

    // 1. Initial status.json with 2 updates pending
    let initial_json = r#"{
        "schema_version": "1.0",
        "status": "PRE_FLIGHT_ATTENTION",
        "status_reasons": ["2 package updates pending"],
        "informational_reasons": [],
        "timestamp": "1793470000",
        "gaming_mode": false,
        "reboot_pending": false,
        "updates": {
            "total": 2,
            "core_count": 0,
            "regular_count": 2,
            "aur_count": 0
        },
        "failed_services": {"system": [], "user": []},
        "disk": {
            "root": {"total_gb": 100.0, "used_gb": 20.0, "free_gb": 80.0, "used_pct": 20},
            "home": {"total_gb": 100.0, "used_gb": 20.0, "free_gb": 80.0, "used_pct": 20}
        },
        "pacnew": {"count": 0, "files": []},
        "orphans": {
            "checked": true, "query_status": "CLEAN", "severity": "INFO",
            "affects_flight_status": false, "tiering_complete": true,
            "count": 0, "packages": [], "optional_count": 0, "optional_packages": [],
            "candidate_count": 0, "tier1_count": 0, "tier1_packages": [],
            "tier2_count": 0, "tier2_packages": [], "tier3_count": 0, "tier3_packages": [],
            "tier3_tags": {}, "unclassified_packages": [], "message": "clean"
        }
    }"#;

    {
        let mut f = File::create(&status_path).unwrap();
        f.write_all(initial_json.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    let initial_snapshot = syspilot_core::read_system_snapshot(&status_path).unwrap();
    assert_eq!(initial_snapshot.updates_total, 2);
    assert_eq!(initial_snapshot.effective_status(), "PRE_FLIGHT_ATTENTION");

    // 2. Spawn headless telemetry watcher simulating main.rs lifecycle
    let gui_running = Arc::new(AtomicBool::new(true));
    let gui_running_watcher = Arc::clone(&gui_running);
    let watcher_observed_updates = Arc::new(AtomicU32::new(initial_snapshot.updates_total));
    let watcher_updates_clone = Arc::clone(&watcher_observed_updates);
    let status_path_clone = status_path.clone();

    let watcher_handle = std::thread::spawn(move || {
        let mut last_mtime = fs::metadata(&status_path_clone)
            .and_then(|m| m.modified())
            .ok();
        while gui_running_watcher.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(25));
            if !gui_running_watcher.load(Ordering::Relaxed) {
                break;
            }

            let current_mtime = fs::metadata(&status_path_clone)
                .and_then(|m| m.modified())
                .ok();
            if current_mtime != last_mtime {
                last_mtime = current_mtime;
                if let Ok(snapshot) = syspilot_core::read_system_snapshot(&status_path_clone) {
                    watcher_updates_clone.store(snapshot.updates_total, Ordering::SeqCst);
                }
            }
        }
    });

    // 3. Sleep 100ms to ensure watcher survives initial iteration (regression guard for 1s exit)
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(watcher_observed_updates.load(Ordering::SeqCst), 2);

    // 4. Update status.json to simulate post-upgrade transition to FLIGHT_READY (0 updates)
    let updated_json = initial_json
        .replace("\"total\": 2", "\"total\": 0")
        .replace("\"regular_count\": 2", "\"regular_count\": 0")
        .replace(
            "\"status\": \"PRE_FLIGHT_ATTENTION\"",
            "\"status\": \"FLIGHT_READY\"",
        );

    // Allow timestamp advance
    std::thread::sleep(Duration::from_millis(50));
    {
        let mut f = File::create(&status_path).unwrap();
        f.write_all(updated_json.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    // 5. Wait up to 500ms for watcher to detect mtime change
    let mut updated = false;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(50));
        if watcher_observed_updates.load(Ordering::SeqCst) == 0 {
            updated = true;
            break;
        }
    }
    assert!(
        updated,
        "Telemetry watcher failed to detect status.json mtime update"
    );

    // 6. Test graceful shutdown via AtomicBool guard
    gui_running.store(false, Ordering::Relaxed);
    let join_res = watcher_handle.join();
    assert!(join_res.is_ok(), "Watcher thread failed to join gracefully");

    // Cleanup
    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_button_atomic_debounce_simulation() {
    let last_spawn = Arc::new(AtomicU64::new(0));
    const DEBOUNCE_WINDOW_MS: u64 = 750;

    fn try_spawn(now_ms: u64, last: &AtomicU64) -> bool {
        let mut prev = last.load(Ordering::Acquire);
        loop {
            if now_ms.saturating_sub(prev) < DEBOUNCE_WINDOW_MS {
                return false;
            }
            match last.compare_exchange_weak(prev, now_ms, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return true,
                Err(actual) => prev = actual,
            }
        }
    }

    // First click at t=1000ms succeeds
    assert!(try_spawn(1000, &last_spawn));
    assert_eq!(last_spawn.load(Ordering::SeqCst), 1000);

    // Rapid double-clicks at t=1050ms, 1200ms, 1749ms must be debounced (rejected)
    assert!(!try_spawn(1050, &last_spawn));
    assert!(!try_spawn(1200, &last_spawn));
    assert!(!try_spawn(1749, &last_spawn));

    // Next click at t=1750ms (exactly 750ms later) succeeds
    assert!(try_spawn(1750, &last_spawn));
    assert_eq!(last_spawn.load(Ordering::SeqCst), 1750);

    // Concurrent burst simulation across 10 threads
    let threads: Vec<_> = (0..10)
        .map(|_| {
            let last_clone = Arc::clone(&last_spawn);
            std::thread::spawn(move || {
                // All try to spawn at t=1800ms
                try_spawn(1800, &last_clone)
            })
        })
        .collect();

    let accepted = threads
        .into_iter()
        .filter_map(|h| h.join().ok())
        .filter(|&v| v)
        .count();
    // Since t=1800 is only 50ms after t=1750, all must be rejected
    assert_eq!(accepted, 0);
}

#[test]
fn test_copilot_async_concurrency_and_raii_unwind() {
    let in_flight = Arc::new(AtomicBool::new(false));

    struct TestGuard(Arc<AtomicBool>);
    impl Drop for TestGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }

    // 1. Concurrency lock gate: only one query can be in flight
    assert!(
        in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    );
    // Second attempt fails
    assert!(
        in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    );
    in_flight.store(false, Ordering::Release);

    // 2. RAII Unwind simulation: thread panics while holding guard
    let in_flight_panic = Arc::clone(&in_flight);
    let handle = std::thread::spawn(move || {
        assert!(
            in_flight_panic
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        );
        let _guard = TestGuard(Arc::clone(&in_flight_panic));
        panic!("simulated worker thread panic during copilot query");
    });

    let join_res = handle.join();
    assert!(join_res.is_err(), "Worker thread should have panicked");

    // Guard Drop must have reset in_flight despite the panic
    assert!(
        !in_flight.load(Ordering::Acquire),
        "in_flight was not reset to false after thread unwind"
    );

    // A new query can now acquire cleanly
    assert!(
        in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    );
}

#[test]
fn test_snapshot_state_and_sentinel_mapping() {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("syspilot-gui-mapping-{nanos}"));
    fs::create_dir_all(&tmp_dir).unwrap();
    let status_path = tmp_dir.join("status.json");

    // 1. Corrupted JSON test -> syspilot_core should return Err for read_system_snapshot
    {
        let mut f = File::create(&status_path).unwrap();
        f.write_all(b"not valid json").unwrap();
        f.flush().unwrap();
    }
    let res = syspilot_core::read_system_snapshot(&status_path);
    assert!(res.is_err(), "Corrupted status.json must return Err");

    // Fallback snapshot handles the error gracefully and sets has_data_problem()
    let fallback = syspilot_core::read_system_snapshot_or_fallback(&status_path);
    assert!(fallback.has_data_problem());
    assert!(fallback.source_error.is_some());

    // 2. Valid snapshot with degraded/stale state
    let degraded_snapshot = syspilot_core::SystemSnapshot {
        status: "ACTION_REQUIRED".into(),
        cache_stale: true,
        source_error: Some("pacman db lock timeout".into()),
        ..Default::default()
    };
    assert!(degraded_snapshot.has_data_problem());
    assert_eq!(
        degraded_snapshot.source_error.as_deref(),
        Some("pacman db lock timeout")
    );

    // Verify effective_status mapping
    assert_eq!(degraded_snapshot.effective_status(), "ACTION_REQUIRED");

    // Cleanup
    let _ = fs::remove_dir_all(&tmp_dir);
}
