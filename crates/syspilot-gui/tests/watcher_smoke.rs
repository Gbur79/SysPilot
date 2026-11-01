use std::fs::{self, File};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
