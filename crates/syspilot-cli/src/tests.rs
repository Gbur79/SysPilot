use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::UNIX_EPOCH;

fn make_exec(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

fn set_mtime(path: &Path, mtime: SystemTime) {
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
}

fn test_env(root: &Path, now: SystemTime) -> CliEnv {
    let state_home = root.join("state");
    let config_home = root.join("config");
    fs::create_dir_all(state_home.join("system-health")).unwrap();
    fs::create_dir_all(config_home.join("syspilot")).unwrap();
    CliEnv {
        root: root.to_path_buf(),
        state_home,
        config_home,
        xdg_bin_home: None,
        home: Some(root.join("home")),
        path_env: None,
        current_exe: None,
        sys_health_env: None,
        proc_stat_env: None,
        max_status_age_env: None,
        shell_fallback_env: None,
        now,
    }
}

#[test]
fn test_cli_env_from_process_and_config_parsing() {
    let proc_env = CliEnv::from_process();
    assert_eq!(proc_env.root, Path::new("/"));
    assert!(proc_env.current_exe.is_some());
    assert!(proc_env.now > UNIX_EPOCH);

    let now_fixed = UNIX_EPOCH + Duration::from_secs(1_700_005_000);
    let empty_env = CliEnv::from_lookup(|_| Some(OsString::from("")), None, now_fixed);
    assert_eq!(empty_env.state_home, PathBuf::from(".local/state"));
    assert_eq!(empty_env.config_home, PathBuf::from(".config"));
    assert_eq!(empty_env.xdg_bin_home, None);
    assert_eq!(empty_env.home, None);
    assert_eq!(empty_env.sys_health_env, None);
    assert_eq!(empty_env.proc_stat_env, None);
    assert_eq!(empty_env.max_status_age_env, None);
    assert_eq!(empty_env.shell_fallback_env, None);

    let populated_env = CliEnv::from_lookup(
        |k| match k {
            "HOME" => Some(OsString::from("/h")),
            "XDG_STATE_HOME" => Some(OsString::from("/st")),
            "XDG_CONFIG_HOME" => Some(OsString::from("/cfg")),
            "XDG_BIN_HOME" => Some(OsString::from("/xb")),
            "SYSPILOT_SYS_HEALTH" => Some(OsString::from("/sh")),
            "SYSPILOT_PROC_STAT" => Some(OsString::from("/ps")),
            "SYSPILOT_MAX_STATUS_AGE" => Some(OsString::from("99")),
            "SYSPILOT_SHELL_FALLBACK_BIN" => Some(OsString::from("/fb")),
            _ => None,
        },
        None,
        now_fixed,
    );
    assert_eq!(populated_env.state_home, PathBuf::from("/st"));
    assert_eq!(populated_env.config_home, PathBuf::from("/cfg"));
    assert_eq!(populated_env.xdg_bin_home, Some(PathBuf::from("/xb")));
    assert_eq!(populated_env.home, Some(PathBuf::from("/h")));
    assert_eq!(populated_env.sys_health_env.as_deref(), Some("/sh"));
    assert_eq!(populated_env.proc_stat_env, Some(PathBuf::from("/ps")));
    assert_eq!(populated_env.max_status_age_env.as_deref(), Some("99"));
    assert_eq!(populated_env.shell_fallback_env, Some(PathBuf::from("/fb")));

    let cfg = parse_config_env(
        "# SYSPILOT_MAX_STATUS_AGE=999\nexport SYSPILOT_MAX_STATUS_AGE=\"3600\"\nSYSPILOT_SYS_HEALTH='/opt/sys-health'\nOTHER=1\n",
    );
    assert_eq!(cfg.max_status_age, Some(3600));
    assert_eq!(cfg.sys_health.as_deref(), Some("/opt/sys-health"));

    let bad = parse_config_env("SYSPILOT_MAX_STATUS_AGE=notnum\nSYSPILOT_SYS_HEALTH=\"\"\n");
    assert_eq!(bad, ConfigOverrides::default());

    let tmp = tempfile::tempdir().unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_700_005_000);
    let mut env = test_env(tmp.path(), now);
    assert_eq!(load_effective_config(&env), (STALE_AFTER_SECS, None));

    fs::write(
        env.config_home.join("syspilot/config.env"),
        "SYSPILOT_MAX_STATUS_AGE=120\nSYSPILOT_SYS_HEALTH=/from/cfg\n",
    )
    .unwrap();
    assert_eq!(
        load_effective_config(&env),
        (120, Some("/from/cfg".to_string()))
    );

    env.max_status_age_env = Some("45".into());
    env.sys_health_env = Some("/from/env".into());
    assert_eq!(
        load_effective_config(&env),
        (45, Some("/from/env".to_string()))
    );
}

#[test]
fn test_bounded_file_security_and_engine_checks() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let now = UNIX_EPOCH + Duration::from_secs(1_700_005_000);
    let mut env = test_env(root, now);

    let regular = root.join("ok.txt");
    fs::write(&regular, b"hello").unwrap();
    assert!(read_bounded_regular_file(&regular, 5).is_ok());
    assert!(read_bounded_regular_file(&regular, 4).is_err());

    let link = root.join("link.txt");
    symlink(&regular, &link).unwrap();
    assert!(read_bounded_regular_file(&link, 16).is_err());
    assert!(read_bounded_regular_file(root, 16).is_err());

    assert!(!have_engine_at(&env, None));
    assert!(!have_engine_at(&env, Some("")));
    assert!(!have_engine_at(&env, Some("/nonexistent/sys-health")));

    let embedded1 = root.join("sysPilot/bin/sys-health.sh");
    make_exec(&embedded1, "#!/bin/sh\nexit 0\n");
    assert!(is_embedded_engine_path(&embedded1));
    assert!(!have_engine_at(&env, embedded1.to_str()));

    let embedded2 = root.join("syspilot/bin/sys-health.sh");
    make_exec(&embedded2, "#!/bin/sh\nexit 0\n");
    assert!(is_embedded_engine_path(&embedded2));
    assert!(!have_engine_at(&env, embedded2.to_str()));

    let custom_engine = root.join("opt/custom-sys-health");
    make_exec(&custom_engine, "#!/bin/sh\nexit 0\n");
    assert!(have_engine_at(&env, custom_engine.to_str()));

    let real_engine = root.join("usr/bin/sys-health");
    make_exec(&real_engine, "#!/bin/sh\nexit 0\n");
    assert!(!is_embedded_engine_path(&real_engine));
    assert!(have_engine_at(&env, None));
    assert!(have_engine_at(&env, Some("")));
    assert!(have_engine_at(&env, real_engine.to_str()));
    env.sys_health_env = Some(real_engine.to_string_lossy().into_owned());
    assert!(have_engine_at(&env, env.sys_health_env.as_deref()));
}

#[test]
fn test_help_version_unknown_and_status_exit_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let env = test_env(tmp.path(), UNIX_EPOCH + Duration::from_secs(1_700_005_000));
    let mut out = Vec::new();
    let mut err = Vec::new();

    assert_eq!(
        plan_cli(&["-h".into()], &env, &mut out, &mut err),
        CliAction::Exit(0)
    );
    let h_out = String::from_utf8(out.clone()).unwrap();
    assert!(h_out.contains("Usage: syspilot [OPTION]"));

    out.clear();
    assert_eq!(
        plan_cli(&["--help".into()], &env, &mut out, &mut err),
        CliAction::Exit(0)
    );
    assert_eq!(String::from_utf8(out.clone()).unwrap(), h_out);

    out.clear();
    assert_eq!(
        plan_cli(&["-v".into()], &env, &mut out, &mut err),
        CliAction::Exit(0)
    );
    assert_eq!(
        String::from_utf8(out.clone()).unwrap(),
        format!("syspilot {VERSION}\n")
    );

    out.clear();
    assert_eq!(
        plan_cli(&["--version".into()], &env, &mut out, &mut err),
        CliAction::Exit(0)
    );
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("syspilot {VERSION}\n")
    );

    err.clear();
    assert_eq!(
        plan_cli(&["--bogus".into()], &env, &mut Vec::new(), &mut err),
        CliAction::Exit(1)
    );
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("unknown option: --bogus")
    );

    assert_eq!(status_exit_code("ALL_CLEAR"), 0);
    assert_eq!(status_exit_code("FLIGHT_READY"), 0);
    assert_eq!(status_exit_code("ACTION_REQUIRED"), 1);
    assert_eq!(status_exit_code("REVIEW_WARNINGS"), 2);

    for f in [
        "-a",
        "--audit",
        "-u",
        "--upgrade",
        "--software",
        "-o",
        "--orphans",
        "--mirrors",
        "-m",
        "--maintenance",
        "-d",
        "--deep-clean",
        "--check-update",
        "--self-update",
        "-g",
        "--gaming",
        "--copilot",
        "--copilot-shell",
        "--faf-repair",
        "--faf-setup",
        "--enable-autostart",
        "--disable-autostart",
    ] {
        assert!(is_heavy_fallback_flag(f), "expected heavy flag: {f}");
    }
    assert!(!is_heavy_fallback_flag("-s"));
    assert!(!is_heavy_fallback_flag("--unknown"));
}

#[test]
fn test_json_and_status_formatting_btime_and_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let now_secs = 1_700_050_000u64;
    let now = UNIX_EPOCH + Duration::from_secs(now_secs);
    let mut env = test_env(root, now);

    // 1. Missing summary without engine vs with engine
    let mut err = Vec::new();
    assert_eq!(
        plan_cli(&["-s".into()], &env, &mut Vec::new(), &mut err),
        CliAction::Exit(1)
    );
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("sys-health is not installed")
    );

    make_exec(&root.join("usr/bin/sys-health"), "#!/bin/sh\nexit 0\n");
    let mut err = Vec::new();
    assert_eq!(
        plan_cli(&["-j".into()], &env, &mut Vec::new(), &mut err),
        CliAction::Exit(1)
    );
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("run: syspilot --audit")
    );

    // 2. Invalid JSON schema
    let summary_path = env.state_home.join("system-health/summary.json");
    fs::write(&summary_path, b"{\"schema_version\":\"1.0.0\"}").unwrap();
    let mut err = Vec::new();
    assert_eq!(
        plan_cli(&["-s".into()], &env, &mut Vec::new(), &mut err),
        CliAction::Exit(1)
    );
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("invalid or incompatible summary schema")
    );

    // 3. Valid JSON without trailing newline + future clock skew
    let json_no_nl = br#"{"schema_version":"2.0.0","status":"FLIGHT_READY","timestamp":"2026-11-22T12:00:00Z","counts":{"errors":0,"warnings":0},"environment":{"kernel":"6.12.1"},"gaming":{"detected":false}}"#;
    fs::write(&summary_path, json_no_nl).unwrap();
    set_mtime(
        &summary_path,
        UNIX_EPOCH + Duration::from_secs(now_secs + 30),
    );
    let mut out = Vec::new();
    assert_eq!(
        plan_cli(&["--json".into()], &env, &mut out, &mut Vec::new()),
        CliAction::Exit(0)
    );
    assert!(out.ends_with(b"}\n") && !out.ends_with(b"\n\n"));

    // 4. Valid JSON WITH trailing newline (no double newline) + exact age == max_age boundary + stale
    let json_with_nl = format!("{}\n", std::str::from_utf8(json_no_nl).unwrap());
    fs::write(&summary_path, json_with_nl.as_bytes()).unwrap();
    set_mtime(
        &summary_path,
        UNIX_EPOCH + Duration::from_secs(now_secs - STALE_AFTER_SECS),
    );
    let mut out_eq_j = Vec::new();
    assert_eq!(
        plan_cli(&["-j".into()], &env, &mut out_eq_j, &mut Vec::new()),
        CliAction::Exit(0)
    );
    let mut out_eq_s = Vec::new();
    assert_eq!(
        plan_cli(&["-s".into()], &env, &mut out_eq_s, &mut Vec::new()),
        CliAction::Exit(0)
    );
    assert!(!String::from_utf8(out_eq_s).unwrap().contains("(STALE)"));

    set_mtime(
        &summary_path,
        UNIX_EPOCH + Duration::from_secs(now_secs - 25_000),
    );
    let mut out = Vec::new();
    assert_eq!(
        plan_cli(&["-j".into()], &env, &mut out, &mut Vec::new()),
        CliAction::Exit(3)
    );
    assert_eq!(out, json_with_nl.as_bytes());

    // 5. Status pre-boot via root/proc/stat, boundary == btime, and SYSPILOT_PROC_STAT
    fs::create_dir_all(root.join("proc")).unwrap();
    let mtime_secs = now_secs - 100;
    set_mtime(&summary_path, UNIX_EPOCH + Duration::from_secs(mtime_secs));
    fs::write(
        root.join("proc/stat"),
        format!("btime {}\n", mtime_secs + 10),
    )
    .unwrap();
    let mut out = Vec::new();
    assert_eq!(
        plan_cli(&["--status".into()], &env, &mut out, &mut Vec::new()),
        CliAction::Exit(0)
    );
    let s_pre = String::from_utf8(out).unwrap();
    assert!(s_pre.contains("Cache age:    100s (PRE-BOOT)"));
    assert!(s_pre.contains("Game profile: not reported"));
    assert!(s_pre.contains("Info:         cached summary predates current boot"));

    let proc_eq = root.join("proc_eq");
    fs::write(&proc_eq, format!("btime {mtime_secs}\n")).unwrap();
    env.proc_stat_env = Some(proc_eq);
    let mut out = Vec::new();
    assert_eq!(
        plan_cli(&["-s".into()], &env, &mut out, &mut Vec::new()),
        CliAction::Exit(0)
    );
    assert!(!String::from_utf8(out).unwrap().contains("(PRE-BOOT)"));

    // 6. Stale + ACTION_REQUIRED + gaming_detected=true
    let action_json = br#"{"schema_version":"2.0.0","status":"ACTION_REQUIRED","timestamp":"t","counts":{"errors":2,"warnings":1},"environment":{"kernel":"6.12.2"},"gaming":{"detected":true}}"#;
    fs::write(&summary_path, action_json).unwrap();
    set_mtime(
        &summary_path,
        UNIX_EPOCH + Duration::from_secs(now_secs - 25_000),
    );
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(
        plan_cli(&["-s".into()], &env, &mut out, &mut err),
        CliAction::Exit(3)
    );
    let s_stale = String::from_utf8(out).unwrap();
    assert!(s_stale.contains("(STALE) (PRE-BOOT)"));
    assert!(s_stale.contains("Game profile: detected"));
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("cached data is stale")
    );

    // Fresh ACTION_REQUIRED -> exit 1
    set_mtime(
        &summary_path,
        UNIX_EPOCH + Duration::from_secs(now_secs - 10),
    );
    fs::write(env.proc_stat_env.as_ref().unwrap(), "btime 100\n").unwrap();
    assert_eq!(
        plan_cli(&["-s".into()], &env, &mut Vec::new(), &mut Vec::new()),
        CliAction::Exit(1)
    );
}

#[test]
fn test_resolution_gui_spawn_and_exec_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let mut env = test_env(root, UNIX_EPOCH + Duration::from_secs(1_700_050_000));

    // Missing binaries
    assert_eq!(
        plan_cli(&["--gui".into()], &env, &mut Vec::new(), &mut Vec::new()),
        CliAction::Exit(1)
    );
    assert_eq!(
        plan_cli(&["--tray".into()], &env, &mut Vec::new(), &mut Vec::new()),
        CliAction::Exit(1)
    );
    let mut fb_err = Vec::new();
    assert_eq!(
        plan_cli(&["--audit".into()], &env, &mut Vec::new(), &mut fb_err),
        CliAction::Exit(1)
    );
    assert!(
        String::from_utf8(fb_err)
            .unwrap()
            .contains("fallback shell router is not installed")
    );

    // Ancestor repo layout & self-exec guard
    let repo = root.join("repo");
    let exe = repo.join("target/release/syspilot");
    let gui_bin = repo.join("target/release/syspilot-gui");
    let tray_bin = repo.join("target/debug/syspilot-tray");
    let fallback_bin = repo.join("bin/syspilot");
    make_exec(&exe, "#!/bin/sh\nexit 0\n");
    make_exec(&gui_bin, "#!/bin/sh\nexit 0\n");
    make_exec(&tray_bin, "#!/bin/sh\nexit 0\n");
    make_exec(&fallback_bin, "#!/bin/sh\nexit 0\n");
    env.current_exe = Some(exe.clone());
    env.shell_fallback_env = Some(exe.clone());

    // Self-exec explicit override must be skipped in favor of repo/bin/syspilot
    assert_eq!(resolve_shell_fallback_at(&env), Some(fallback_bin.clone()));
    assert_eq!(
        resolve_companion_bin_at(&env, "syspilot-gui"),
        Some(gui_bin.clone())
    );
    assert_eq!(
        resolve_companion_bin_at(&env, "syspilot-tray"),
        Some(tray_bin.clone())
    );

    // SpawnDetached success and failure
    assert_eq!(
        execute_or_plan(
            &["--gui".into()],
            &env,
            &mut Vec::new(),
            &mut Vec::new(),
            |_, _, _| Ok(())
        ),
        0
    );
    assert!(launch_gui_bin(&root.join("missing-gui")).is_err());

    // ExecReplace for --tray (guard=false) and --audit (guard=true)
    let mut captured = None;
    let rc_tray = execute_or_plan(&[], &env, &mut Vec::new(), &mut Vec::new(), |b, a, g| {
        captured = Some((b.to_path_buf(), a.to_vec(), g));
        Ok(())
    });
    assert_eq!(rc_tray, 0);
    assert_eq!(captured, Some((tray_bin, vec![], false)));

    let rc_audit =
        execute_or_plan(
            &["--audit".into(), "--fast".into()],
            &env,
            &mut Vec::new(),
            &mut Vec::new(),
            |b, a, g| {
                let cmd = build_exec_command(b, a, g);
                assert_eq!(cmd.get_program(), fallback_bin.as_os_str());
                let envs: Vec<_> = cmd.get_envs().collect();
                assert!(envs.iter().any(|(k, v)| *k == "SYSPILOT_SHELL_FALLBACK"
                    && *v == Some(std::ffi::OsStr::new("1"))));
                Err("mock exec err".into())
            },
        );
    assert_eq!(rc_audit, 1);

    // XDG_BIN_HOME -> ../lib/syspilot/bin/syspilot fallback
    let xdg_bin = root.join("xdg/bin");
    let xdg_lib = root.join("xdg/lib/syspilot/bin/syspilot");
    make_exec(&xdg_lib, "#!/bin/sh\nexit 0\n");
    env.current_exe = None;
    env.shell_fallback_env = None;
    env.xdg_bin_home = Some(xdg_bin);
    assert_eq!(resolve_shell_fallback_at(&env), Some(xdg_lib));
}
