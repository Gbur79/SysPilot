# Changelog

All notable changes to the **SysPilot** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [2.0.1] - 2026-10-31

### Fixed / Live Telemetry Watcher Thread Lifetime & Event Loop Dispatch (`BP-ARGON-20261031-SYS-010-WATCHER`)
- **Slint Worker Thread Affinity & Auto-Refresh (`crates/syspilot-gui/src/main.rs`)**:
  - Replaced illegal worker-thread `ui_weak_watcher.upgrade()` check (which returned `None` within 1s due to Slint's thread-affinity constraint) with atomic `gui_running: Arc<AtomicBool>` lifecycle tracking.
  - Switched telemetry updates to `ui_weak_watcher.upgrade_in_event_loop(...)`, safely upgrading the component reference on the UI main thread and tearing down the worker thread gracefully upon event loop termination.
  - Permanently fixed stale GUI dashboard post-upgrade: window immediately syncs to `FLIGHT READY ✔` / 0 pending packages upon external `status.json` write.
- **Headless GUI Integration Test & Per-Crate Quality Gate (`dev-tools/verify-rust.sh`, `crates/syspilot-gui/tests/watcher_smoke.rs`)**:
  - Implemented headless integration fixture `test_watcher_mtime_lifecycle_and_shutdown` in `syspilot-gui`, verifying mtime watcher polling, state transitions, and atomic cooperative shutdown in <200ms without Wayland/X11 display dependencies.
  - Upgraded `verify-rust.sh` to enforce per-crate non-zero test accounting (`core: 20 | tray: 12 | gui: 1`), permanently preventing zero-test crates from silently passing CI.
- **Sprint 1: Process Concurrency, Reaper Battery Optimization & Underflow Immunity (`crates/syspilot-core/src/terminal.rs`, `BP-ARGON-SPRINT-01-TERMINAL-001`)**:
  - Re-architected `syspilot-child-reaper` with `TrackedProcess` enum separating terminal sessions from detached GUI processes, preventing fatal slot underflow to `usize::MAX`.
  - Hardened `release_slot()` with saturating subtraction (`n.saturating_sub(1)`).
  - Eliminated 345,600 daily idle CPU wakeups by replacing 250ms polling with blocking `rx.recv()` when child list is empty, allowing deep CPU C-states.
  - Hardened `spawn_syspilot_task` with POSIX-safe argument array passing (`exec syspilot "$@"`), eliminating shell word-splitting.
  - Added explicit WezTerm terminal emulator support (`wezterm start --`).
  - Added hermetic unit tests `test_terminal_slot_bounds_and_underflow_immunity` and `test_detached_gui_reaper_isolation` (bringing test count to 35/35).
- **Level-Triggered ALPM Persistence & Post-Upgrade Triage (`EOS-SRE-ARCH-P1`, INC-20261101-0851)**:
  - Eliminated the Nyquist-Shannon sampling flaw in `syspilot-tray` where short pacman updates (<10s) were missed by `db.lck` pulse polling, by introducing persistent mtime tracking of `/var/lib/pacman/local` and `summary.json`.
  - Added asynchronous exit callback mechanism (`TerminalWithCallback`) in `syspilot-core::terminal`, ensuring that when an upgrade or maintenance terminal is closed, triage and GUI sync are triggered immediately within <=100ms.
  - Linked GUI 1-second watcher to detect external modifications to `summary.json`, eliminating the state persistence disconnect.
  - Added hermetic unit test `test_terminal_callback_executed_on_exit` (36/36 tests PASS).

### Fixed / SysPilot Tray Deduplication, Background Triage & Kernel Ingestion (`BP-ARGON-20261031-INCIDENT-001`)
- **Single-Instance Advisory Locking (`crates/syspilot-tray`, `crates/syspilot-gui`, `crates/syspilot-core`)**:
  - Implemented kernel-level single-instance lock via `libc::flock` (`~/.local/state/syspilot/.tray.lock`), preventing duplicate status icons when started under multiple autostart mechanisms.
  - Re-exported shared `acquire_single_instance_lock()` from `syspilot-core` for unified mutex management across tray and GUI (`.gui.lock`).
- **Autonomous Startup & 15-Minute Background Triage (`crates/syspilot-tray`)**:
  - Implemented unconditional cold-startup / reboot background triage, ensuring telemetry is 100% fresh immediately upon login and eliminating false-positive stale cache warnings (`DATA DEGRADED ⚠` / `age > 6h`).
  - Added periodic 15-minute background triage timer with automatic GameMode freeze and post-update / post-pacman unlock refresh.
- **Proactive UEFI Secure Boot Certificate Quirk Classifier (`crates/syspilot-core/src/firmware.rs`)**:
  - Proactively classified UEFI Secure Boot certificates (`uefi_kek`, `uefi_dbx`, `uefi_db`, `uefi_pk`) as bypassed harmless quirks whenever Secure Boot is disabled (`!secure_boot_active`), without requiring an initial NVRAM write failure (`UpdateState == 3`).
  - Permanently resolved recurring false-positive `PRE-FLIGHT ATTENTION ⚠` and `1 firmware update(s) available via LVFS [UPDATE ⚠]` warnings for Microsoft KEK CA on motherboards with NVRAM write constraints (e.g. Haswell Z97).
- **Live Package Triage & Auto-Refreshing GUI Dashboard (`crates/syspilot-tray`, `crates/syspilot-gui`)**:
  - Fixed frozen update counters in `status.json` by invoking live package triage (`check_pkgs: true`) upon daemon startup, pacman unlock events (`pacman_unlocked`), and the 1-hour periodic updater tick, while preserving fast (<50ms) low-I/O triage during resting 15-minute cycles.
  - Added non-blocking background filesystem watcher in `syspilot-gui` polling `status.json` mtime at 1 Hz, automatically updating the open desktop dashboard in real time upon upgrade completion without manual refresh.
- **Canonical Telemetry Ingestion & Native OS/Kernel Discovery (`crates/syspilot-core`)**:
  - Integrated `load_sys_health()` in `triage.rs` to ingest canonical `~/.local/state/system-health/summary.json` (Schema 2.0).
  - Added zero-fork native fallbacks (`detect_kernel()` via `/proc/sys/kernel/osrelease`, `detect_os()` via `/etc/os-release`), permanently resolving "Kernel unknown" and generic "Linux" labels.
- **XDG Autostart Generator Invariant (`autostart/syspilot.desktop`)**:
  - Added `X-systemd-skip=true` to prevent `systemd-xdg-autostart-generator` from generating duplicate `app-syspilot@autostart.service` on KDE Plasma 6 Wayland and GNOME.
- SRE verification: `dev-tools/verify-rust.sh -q` ➔ 32/32 unit tests PASS, Clippy clean, fmt clean. Certified by Auditor Argon Pro (`BP-ARGON-20261031-INCIDENT-001`).

## [2.0.0-alpha.1] - 2026-10-30

### Fixed / Slint Maintenance Action Grid & Typography Centering (`38de341`)
- **Action Button Grid Standardization (`maintenance_card.slint`)**:
  - Enforced a uniform `width: 175px` across all 7 action buttons in System Maintenance cards, replacing ragged, text-dependent button widths with an aligned right-hand action column.
  - Set `min-height: 68px` on `MaintenanceCard` ensuring uniform card proportions.
- **Pixel-Perfect Typography & Vertical Centering (`action_button.slint`, `status_badge.slint`, `appwindow.slint`)**:
  - Standardized interactive buttons and HUD header badge to a shared `height: 32px`.
  - Added explicit `vertical-alignment: center` and `horizontal-alignment: center` to `ActionButton`, `StatusBadge`, and the Copilot metadata header badge, eliminating upward text shift and optical baseline misalignment.
  - SRE verification: `dev-tools/verify-rust.sh -q` ➔ 31/31 unit tests PASS, Clippy clean, fmt clean.

### Fixed / Slint Navigation Bar & Vertical Stretch Constraint (`9f96ab1`)
- **Slint Layout Engine Optimization (`appwindow.slint`, `tab_button.slint`)**:
  - Enforced `vertical-stretch: 0` and fixed `height: 36px` on `TabButton` and `HorizontalLayout` navigation bar, as well as top HUD header and error banners.
  - Assigned `vertical-stretch: 1` exclusively to active tab view containers (Dashboard, System Maintenance `ScrollView`, and Copilot diagnostic console).
  - Eliminated the UI bug where `TabButton` expanded vertically to ~280px upon switching to System Maintenance or Copilot, pushing cards 4–7 out of the visible screen.
  - SRE verification: `dev-tools/verify-rust.sh -q` ➔ 31/31 unit tests PASS, Clippy clean, fmt clean.

### Added / Phase 4: Packaging Cutover & Legacy Python Retirement (`0aae6c6` ➔ `346390d`)
- **Total Python Runtime Emancipation**:
  - Retired 3,802 lines of legacy Python code: `core/triage.py`, `core/copilot_config.py`, `gui/syspilot_gui.py`, `bin/syspilot-sentinel`, `systemd/syspilot-sentinel.service`, `systemd/syspilot-sentinel.timer`.
  - Upgraded `packaging/PKGBUILD` to v2.0.0 pure Rust release build (`-p syspilot-tray -p syspilot-gui`).
  - Purged runtime dependencies: dropped `python` and `python-pyqt6`; added minimal runtime graphics libraries `fontconfig`, `freetype2`, and `hicolor-icon-theme`.
- **Desktop Launcher & XDG Autostart Decoupling**:
  - Implemented dedicated application launcher `packaging/syspilot.desktop` (`Exec=syspilot --gui`) for desktop application menus (KDE Kickoff, GNOME Dash, rofi/wofi).
  - Preserved `autostart/syspilot.desktop` (`Exec=syspilot --tray`) strictly for background tray autostart.
- **Deterministic FAF Runner & Repair Integration**:
  - Added CLI fast-path routing `--faf-repair|--faf-setup` in `bin/syspilot` resolving project tree, `/usr/lib/syspilot/bin`, and PATH.
  - Installed `bin/syspilot-faf-repair` directly to `/usr/bin/syspilot-faf-repair`.
- **SRE Post-Implementation Audit (`SRE-AUDIT-20261030-PATCH-016-PHASE-4-POST`) by Auditor Argon**:
  - Audited commit `0aae6c6`; rejected initial packaging due to FM-1 (FAF isolation), FM-2 (desktop entry conflation), FM-3 (missing `sys-health` in optdepends), FM-4 (ShellCheck SC1007), and FM-5 (README drift).
  - Designed and certified resilient drop-in blueprint in commit `346390d`.
  - Added `'sys-health: canonical system diagnostics and maintenance engine'` to `PKGBUILD` `optdepends`.
  - Formal certification: **CERTIFIED [Argon]** for `346390d`. Test suite: 31/31 unit tests PASS, Clippy clean, fmt clean.

### Added / Multi-Agent CLI Adapter & Hardened Flag Negotiation (`b85dbf3`)
- **Discrete Token Parsing & Flag Adaptation (`crates/syspilot-core/src/copilot.rs`)**:
  - Implemented token-aware flag negotiation converting `-i` / `--prompt-interactive` to `-p` (`--print`) for non-interactive execution without corrupting agent names (`sys-info`) or colliding with flags like `--project` or `--profile`.
  - Added binary identification engine `is_agy_command` protecting against false positives (`pedagogy`, `strategy-tool`).
  - Added discrete token inspection and shell array safety in `bin/syspilot` using `eval "custom_cmd=($SYSPILOT_COPILOT_CMD)"`.
  - Enforced environment variable precedence over `~/.config/syspilot/config.env` values.
- **SRE Post-Implementation Audit (`SRE-AUDIT-20261030-PATCH-016-MULTI-AGENT-POST`) by Auditor Argon**:
  - Formal audit rejected naive substring replacement in commit `8a28b73`.
  - Architected and certified hardened blueprint in commit `b85dbf3`.
  - Added regression test suite: `test_agy_command_identification`, `test_agy_flag_adaptation_hygiene` (workspace test suite: 31/31 passing).

### Added / System Maintenance Suite & AI Copilot Workspace (PATCH-016 Phase 3)
- **Central Terminal Runner (`crates/syspilot-core/src/terminal.rs`)**:
  - Centralized terminal execution engine across the entire workspace (re-exported in `syspilot-tray`).
  - Dynamic discovery for 7 terminal emulators (`konsole`, `ghostty`, `kitty`, `alacritty`, `gnome-terminal`, `foot`, `xterm`) + universal fallback for custom emulators (`wezterm`, `urxvt`, `terminator`) without panics.
  - Slot reservation (`MAX_TERMINALS = 8`) and background child reaper thread.
- **System Maintenance Suite Tab (`appwindow.slint`)**:
  - Dedicated interactive tab with 7 maintenance actions: 3-tier orphan package triage, regional mirror benchmark/ranking, safe cache maintenance, deep clean, CVE advisory inspection, LVFS firmware upgrade, and pacman package pre-caching.
- **Native AI Copilot Workspace (`crates/syspilot-core/src/copilot.rs` & Slint)**:
  - Zero-dependency configuration parser reading `~/.config/syspilot/config.env` and `~/.config/goose/config.yaml`.
  - Dynamic recipe resolution cascade honoring `SYSPILOT_GOOSE_RECIPE`, user recipe folder, packaged recipe (`copilot-admin.yaml`), and development tree.
  - SRE 1-Click Battle-Tested Playbooks: FAF client repair, Steam / Proton launch fix, .pacnew surgical merging, PipeWire audio reset.
  - Non-blocking asynchronous query dispatching with atomic in-flight debouncing (`AtomicBool`) preventing concurrent process spawns.
- **SRE Post-Implementation Audit (`SRE-AUDIT-20261030-PATCH-016-PHASE-3-POST`) by Auditor Argon**:
  - Resolved FM-1 (Goose CLI 1.57+ argument separation: `--recipe` + `--params` vs `--text` isolation).
  - Resolved FM-2 (universal fallback in `build_terminal` eliminating `unreachable!()` panic).
  - Resolved FM-3 (dynamic recipe cascade eliminating hardcoded `eos-admin.yaml`).
  - Resolved FM-4 (debouncing playbook triggers in Slint UI and Rust runtime).
  - Added unit tests: `test_terminal_custom_fallback`, `test_goose_recipe_resolution` (workspace test suite: 28/28 passing).

### Added / Architectural Migration (PATCH-016 Phases 1 & 2)
- **Cargo Workspace Architecture**:
  - Initialized workspace root managing `crates/syspilot-core`, `crates/syspilot-tray`, and `crates/syspilot-gui`.
  - Migrated legacy `rust-tray` to `crates/syspilot-tray`, reducing background RAM to ~5.7 MB RSS on KDE Plasma Wayland.
- **Headless Core Triage Engine (`syspilot-core`)**:
  - Pure Rust implementation of system triage, CVE security audit (`arch-audit`), hardware firmware LVFS probe (`fwupd`), multi-backend host firewall (`firewalld`, `ufw`, `nftables`), 3-tier orphan package classification, and transient systemd unit filtering.
  - Zero Python dependencies for core diagnostics.
- **Native Slint Desktop GUI (`syspilot-gui`)**:
  - Implemented standalone 14 MB native executable using Slint 1.18.1.
  - Dual renderers (`renderer-femtovg` OpenGL + `renderer-software` fail-safe CPU rasterizer) for Maxwell / Wayland resilience.
  - Single instance locking via non-blocking `libc::flock`.
- **SRE Post-Implementation Audit (`SRE-AUDIT-20261030-PATCH-016-PHASE-1-2-POST`) by Auditor Argon**:
  - Triaged and resolved systray warning vs GUI green state discrepancy.
  - Implemented resilient Serde deserialization for `DiskUsage` and `StatusUpdates` (`#[serde(default)]`, alias `avail_gb`), guaranteeing 100% backward compatibility with `sys-health.sh` Schema 1.0.
  - Converted GUI state machine to fail-closed UI: removed optimistic mock defaults from `appwindow.slint`, added reactive error banner (`has_error`, `error_message`), and neutral initial states (`--` / `CONNECTING...`).
  - Added hermetic unit test `test_deserialize_schema_1_0_disk_compatibility` (workspace test suite: 24/24 passing).

---

## [1.2.1] - 2026-10-30

### Added / Fixed
- **PATCH-015 — Universal Secure Boot Resilience & Harmless NVRAM Quirk Classifier (`fwupd`)**:
  - Implemented zero-cost (<0.05ms) pure Python Secure Boot detection in `core/triage.py` (`is_secure_boot_enabled()`).
  - Added universal hardware quirk classification for UEFI certificate updates (`uefi_kek`, `uefi_dbx`) failing on legacy platforms due to NVRAM buffer limits (`efivarsfs: Invalid argument`).
  - When Secure Boot is disabled on host, harmless failed certificate writes are bypassed from `pending_devices` to prevent infinite update loops.
  - Added GUI dashboard status indicator: `PASS ✔ (Hardware up to date • 1 harmless UEFI quirk bypassed)`.
  - Preserved full alert escalation for systems with active Secure Boot.

---

## [1.2.0] - 2026-10-30

### Added
- **PATCH-011 — Arch Linux Security Advisories & CVE Severity Highlighting (`arch-audit`)**:
  - Added non-blocking CVE telemetry in `core/triage.py` querying `arch-audit -u --json` with 3s timeout.
  - Added `Security Advisories (arch-audit)` row in GUI Dashboard *System Updates* with High/Critical CVE badges.
  - Added interactive `[🔍 INSPECT CVE]` action launching terminal CVE inspection.
  - Extended Rust Sentinel (`state.rs` & `tray.rs`) to escalate status to `NeedsAttention` and icon to `dialog-warning` when fixable High-Risk CVEs are detected.
- **PATCH-012 — Hardware Firmware & Peripherals Telemetry & Upgrade Action (`fwupd` / LVFS)**:
  - Added local firmware status probe via `fwupdmgr get-updates --no-unreported-check --json` in `core/triage.py`.
  - Added `Hardware Firmware (fwupd)` row in GUI Dashboard *Third-Party Updates* monitoring SSDs, UEFI, and peripherals.
  - Added safe, interactive `[⚡ UPGRADE FIRMWARE]` button running `fwupdmgr update` in terminal with user confirmation.
- **PATCH-013 — Multi-Backend Host Firewall & Shield Inspection (`firewalld` / `ufw` / `nftables`)**:
  - Implemented multi-backend host firewall detection in `core/triage.py` reading systemd state and default zone in <5ms.
  - Added `Host Firewall (Shield)` row in GUI Dashboard *System Health & Hygiene*.
  - Universalism & fail-safe: systems relying on perimeter routers (e.g. pfSense) report neutral `INFO ℹ` without false-positive alarms.
- **PATCH-014 — On-Demand Package Pre-Caching (`[⚡ PRE-CACHE PACKAGES]`)**:
  - Added `[⚡ PRE-CACHE PACKAGES]` action button in GUI Dashboard *System Updates* to download pending packages into pacman cache interactively before upgrading.
  - Strictly forbidden background sudo execution in Rust tray daemon to prevent TTY authentication failure.

---

## [1.1.1] - 2026-10-30

### Fixed
- **Post-Upgrade Tray State Desynchronization**:
  - Fixed an issue where the tray icon remained an amber warning triangle (`dialog-warning` / `NeedsAttention`) after upgrades completed until the 3600s updater timer elapsed.
  - Implemented `effective_updates(&self)` in `tray.rs` prioritizing clean `SystemSnapshot` state (`updates_total == 0`) over stale background polling cache.
  - Model cache invalidation: reset internal `tray.updates = None` in `refresh()` whenever authoritative telemetry drops to zero updates.
  - Re-evaluating updater hook: added pacman lock release transition detector (`pacman_unlocked`) in `sentinels_timer` spawning a non-blocking background check upon package manager exit.
  - Added unit test `test_effective_updates_cache_invalidation` verifying state precedence across snapshot and background cache.

---

## [1.1.0] - 2026-10-30

### Added
- **PATCH-009 — High-Performance 24/7 Rust Sentinel (`rust-tray`), Native D-Bus (`zbus 5.19`), IPC Single-Instance & Instant Responsiveness**:
  - Implemented ultra-lightweight Rust StatusNotifierItem daemon (`syspilot-tray`) running 24/7 in KDE Plasma Wayland and X11 sessions consuming only **~1.2 MB RAM and 0.0% CPU**.
  - Migrated D-Bus queries from periodic external `busctl` shell executions to native session bus calls via `zbus 5.19`, eliminating ~28,800 fork/exec calls per day.
  - Implemented `sentinels.rs`:
    - Non-blocking kernel reboot detection via `/proc/sys/kernel/osrelease` and `/usr/lib/modules/<version>`.
    - Intelligent pacman database lock monitor: differentiates running ALPM package managers (`pacman`, `yay`, `paru`, `pamac-daemon`) from orphaned `/var/lib/pacman/db.lck` locks.
    - Native `libc::statvfs` disk capacity measurements for `/` and `/home` directly in memory without shell forks.
  - Implemented `updater.rs`:
    - Async Tokio background worker evaluating official packages (`checkupdates` with isolated temporary DB in `/tmp` and auto-healing of stale locks).
    - Prioritized Core Package classification (`linux`, `systemd`, `nvidia`, `grub`, `dracut`, `glibc`, `pipewire`, `wireplumber`).
    - Async AUR update evaluation (`yay -Qua` / `paru -Qua`) and standalone third-party tools triage (`sys-health --software --json`).
    - **GameMode Freeze:** background disk and network scans are automatically paused during active gaming sessions.
  - Spillage-free context menu: flattened context menu to 5 ergonomic actions and rich multi-line tooltip.
  - Hermetic unit tests in `rust-tray/src/tests.rs` expanded to 10 comprehensive tests (`cargo test --locked`, 10/10 PASS).
- **PATCH-010 — Dynamic Hardware Platform & GPU Row & Universal Decoupled AI Copilot**:
  - Added `Hardware Platform & GPU` telemetry row under *System Health & Hygiene* dynamically querying CPU model and GPU architecture/driver via `/proc/cpuinfo` and `nvidia-smi`/`lspci`/`summary.json` without hardcoding.
  - Introduced configuration resolver for `~/.config/syspilot/config.env` (`SYSPILOT_COPILOT_CMD`, `COPILOT_LABEL`, `SYSPILOT_GOOSE_RECIPE`), allowing private custom copilot agents or terminal runners without modifying public git codebase.
  - Removed hardcoded "Goose SRE" labels from tabs and headers in favor of neutral `SysPilot AI Copilot`.
  - Packaging modernization: updated `packaging/PKGBUILD` to v1.1.0 with cargo release build and `syspilot-tray.service` user unit.

### Fixed
- **BUG-01 (Double Window on rapid clicks)**:
  - Added 1.5s atomic debounce in Rust tray `activate()`.
  - Added atomic `fcntl.flock` on `~/.local/state/syspilot/.gui.lock` and early `QLocalServer` binding in `gui/syspilot_gui.py` before heavy Qt widget hierarchy creation.
- **BUG-02 (Amber vs Green Desync Lag)**:
  - Synchronized `effective_status` between Python and Rust, eliminating lag between tray status and GUI dashboard components.
  - Reduced cold-start autotriage delay from 1000ms to 100ms.
- **Decoupling from embedded engine copy**:
  - Removed obsolete embedded copy of `bin/sys-health.sh` in favor of external canonical engine integration.

---

## [1.0.7] - 2026-10-16

### Fixed
- **PATCH-008 — SysPilot / sys-health Parity (Dual-Tier Orphan Telemetry & Severity Harmonization)**:
  - Eliminated false `PRE_FLIGHT_ATTENTION` (🟡) flight readiness degradation when benign orphan packages exist. In alignment with `sys-health.sh`, orphan presence is now strictly informational (`INFO ℹ`) and preserves `FLIGHT_READY` (🟢).
  - Implemented shared 3-tier classification in `core/triage.py` (`ORPHAN_SENSITIVE_PATTERNS`, `_orphan_sensitive_tag()`) matching `classify_orphan_tier()` in `sys-health.sh`:
    - **Tier 1 (Strict Unreferenced)**: candidates eligible for automatic pruning.
    - **Tier 2 (Optional for Apps)**: reverse optional dependencies.
    - **Tier 3 (Heuristically Sensitive)**: packages flagged as desktop, compositor, display manager, kernel, boot, graphics, or audio (e.g. `kwin-x11`).
  - Differentiated GUI display in `gui/syspilot_gui.py`:
    - When Tier 1 packages are detected: displays `INFO ℹ ({count} strict orphan(s) eligible for pruning)` with amber `🗑 PRUNE ORPHANS`.
    - When only Tier 2 or Tier 3 packages exist: displays `INFO ℹ (0 strict unreferenced • {count} candidates in review)` with cyan `🗑 REVIEW CANDIDATES`.
    - Only pacman query failures or database execution errors degrade readiness to `WARN ⚠`.
  - Aligned CLI scorecard (`syspilot -s`) to report informational cyan status for unrequired candidates rather than yellow warnings.
  - Implemented atomic state persistence (`.status.json.tmp.<pid>` -> `os.replace`) to prevent race conditions during concurrent triage reads.

## [1.0.6] - 2026-10-09

### Added
- **PATCH-007 — Deterministic 1-Click FAF Setup & Dynamic Launcher Resolver (`syspilot --faf-repair`)**:
  - Implemented `bin/syspilot-faf-repair`, an autonomous zero-token repair utility that checks prerequisites (`bwrap`, 32-bit Vulkan ICDs), synchronizes components via `update.sh perform`, and dynamically interrogates Steam libraries (including multi-mount `libraryfolders.vdf`) to sync `Game.prefs` with backup safety.
  - Automatically heals frozen/obsolete versioned shortcuts on `~/Desktop` and `~/.local/share/applications` by repointing them to the dynamic runner wrapper (`$HOME/faf-linux/run`), ensuring upstream background updates (`update.sh autoupdate-notify`) are never bypassed.
  - Added CLI fast-paths `--faf-repair` and `--faf-setup` to `bin/syspilot`.
  - Enhanced the `⚡ Setup / Repair FAF Client` playbook button on the Copilot tab in `gui/syspilot_gui.py`.
  - Updated `copilot/playbooks/faf_setup_guide.md` with 1-click self-healing commands, desktop integration best practices, and Scenario C troubleshooting.

## [1.0.5] - 2026-10-09

### Fixed
- **PATCH-005 — Transient Desktop Application Unit False Positive:** SysPilot now ignores failed user units only when systemd confirms that an `app-*.service` or `app-*.scope` candidate is both transient and assigned to `app.slice`. This prevents KDE/desktop launcher remnants, such as failed one-shot `app-java@….service` units, from incorrectly elevating Flight Readiness status while preserving persistent user daemons, all system failures, and generic transient `run-*.scope` diagnostics.

## [1.0.4] - 2026-10-05

### Added
- **Terminal Lifecycle Waiter & Automatic Dashboard Triage Refresh**: Terminal operations (`run_custom_terminal`, `run_guarded_upgrade_terminal`, etc.) are now monitored by a dedicated background thread. When the terminal window closes, `self.terminal_finished` emits on the Qt thread, triggering an instant, automatic `update_ui_from_state()`.
- **Window Focus Regain Refresh (`changeEvent`)**: Automatically refreshes telemetry when the SysPilot GUI window regains user focus (`ActivationChange`).
- **Konsole `--nofork` Process Isolation**: Configured `konsole --nofork` in `get_terminal_cmd` so that Konsole child processes block cleanly until closed, preventing detached zombie execution.
- **Dual-Tier Dependency Intelligence (Strict Orphans vs Optional Candidates)**:
  - `core/triage.py` now queries both `pacman -Qtdq` (strict unrequired orphans) and `pacman -Qdttq` (reverse optional dependencies like `luit` for `xterm`).
  - Strict orphans show as `WARN ⚠` with amber `🗑 PRUNE ORPHANS`, while optional-only packages display as `INFO ℹ (0 strict orphans • X optional candidate)` with a cyan `🗑 REVIEW CANDIDATES` button, keeping the audit section `ALL CLEAR ✔`.

### Fixed
- **GUM Interactive Selection Abort on Enter (`gum choose --no-limit`)**:
  - In `bin/sys-health.sh`, `gum choose --no-limit` requires pressing **[SPACE]** to mark `[✔]` before pressing **[ENTER]**. If users pressed Enter directly, Gum previously returned an empty selection and silently aborted.
  - Added visual checkbox prefixes (`--cursor-prefix="[ ] "`, `--unselected-prefix="[ ] "`, `--selected-prefix="[✔] "`).
  - Added smart single-candidate confirmation fallback: if only 1 package is in the list and the user presses Enter without Space, `gum confirm` automatically asks `"Did you want to remove '<pkg>'?"` defaulting to Yes.
  - Added warning advice when multiple packages are present without Space toggles.

---

## [1.0.3] - 2026-10-05

### Added
- **GUM & Sys-Health Aesthetic UI Redesign (PATCH-003)**:
  - Replaced amorphous cards with high-contrast, contiguous 2-column grid tables mirroring Charm GUM and `sys-health.sh` (`render_audit_section`).
  - Standardized on fixed-width 270px component column with ANSI delimiter line (`│`) and zebra-striped row backgrounds (`#090d15` / `#0f172a`).
  - **Dynamic Severity HUD Header**: GUM double-border top box that dynamically changes color and glow based on system issue severity:
    - 🔴 Red (`#ef4444` / ANSI 196) for `ACTION_REQUIRED`.
    - 🟡 Amber / Gold (`#ffaf00` / ANSI 214) for `PRE_FLIGHT_ATTENTION`.
    - 🔵 Cyan (`#38bdf8` / ANSI 81) when `GameMode` is active.
    - 🟢 Green (`#4ade80` / ANSI 82) for `FLIGHT_READY`.
  - Added real-time environment telemetry chip directly inside HUD: `kernel`, `uptime`, `root storage`, and active `GPU`.
  - Implemented crisp monospace typography (`Hack`, `DejaVu Sans Mono`, `Noto Sans Mono`) for 100% legibility on high-resolution displays.
  - Redesigned action buttons with GUM terminal palette (`gumBtnGreen`, `gumBtnCyan`, `gumBtnAmber`).

---

## [1.0.2] - 2026-10-05

### Added
- **Orphan Packages Dashboard Indicator & Triage**: Added `check_orphan_packages()` to `core/triage.py` using `pacman -Qtdq` (~100ms execution, zero-root). Unrequired dependencies are surfaced directly on the **🛡 System Health & Disk State** card with an interactive `🗑 Prune Orphans` terminal launcher.
- **Reboot Pending Visibility**: Added dynamic amber warning banner on the Dashboard and desktop notification alerts when running kernel modules on disk are replaced by system updates.
- **Cold-Startup Autotriage**: Initial non-blocking asynchronous background sweep (`QTimer.singleShot(1000, ...)`) on GUI startup to ensure package telemetry is immediately accurate after cold boot or desktop login.
- **Periodic Background Refresh**: Hourly automated package update sweeps during standard desktop operations (respecting GameMode inhibition).
- **Configuration Conflicts (.pacnew) on Dashboard**: Direct `.pacnew` status line on the primary Flight Readiness card.
- **Enhanced CLI Flight Readiness Scorecard (`syspilot -s`)**: Displays real-time orphan package counts and reboot pending status in terminal output.

### Fixed
- **Periodic Cache Erasure (Discrepancy with Terminal Upgrades)**: Fixed a critical issue where 5-minute background watchdog checks with `check_pkgs=False` were clobbering `status.json` with empty update stubs, causing the Dashboard to falsely report 0 pending updates while the terminal upgrade showed pending packages.
- **Stale Lock Auto-Healing in `checkupdates`**: Added automatic recovery from orphaned `/tmp/checkup-db-$UID/db.lck` locks before executing `checkupdates`.
- **Status Classification Semantics**: Pending regular updates and unrequired orphan packages now properly elevate the flight readiness state to `PRE_FLIGHT_ATTENTION` (🟡) with detailed `status_reasons` instead of reporting a false `FLIGHT_READY` (🟢).

---

## [1.0.1] - 2026-10-04

### Added
- **Dynamic AI Model Discovery**: Lightweight, zero-token parser reading `~/.config/goose/config.yaml` to detect active provider, model name, and thinking effort (`get_active_model_details()`).
- **Header AI Model Badge**: Added active AI engine badge (`🤖 <model>`) next to the Refresh button in the GUI header with rich tooltips detailing provider and config file path.
- **Stream-JSON Event Processing**: Refactored `CopilotWorker` to run Goose in event stream mode (`--output-format stream-json --no-session`).
- **Single-Line Diagnostic Indicators**: Formatted `toolRequest` diagnostic actions cleanly (e.g., `⚙️ [Diagnostic Check] shell: ping -c 1 192.168.1.1`) rather than dumping unformatted terminal outputs into the chat view.
- **Execution Telemetry Footer**: Displays execution metrics cleanly at the bottom of copilot answers (model, token count, USD cost).
- **CLI Flag `--skill` Dynamic Reporting**: Reports real-time active model target dynamically.

### Fixed
- **Recipe Model/Provider Conflicts**: Removed hardcoded `goose_provider` and `goose_model` defaults from `copilot/recipe.yaml`, fully respecting user configuration in `~/.config/goose/config.yaml`.
- **Chat Window Log Flooding**: Prevented multi-kilobyte background diagnostic dumps from burying the synthesized AI response.

---

## [1.0.0] - 2026-10-04

### Added
- **Initial Release of SysPilot**: Autonomous, token-lean SRE desktop copilot and health sentinel for Arch Linux & derivatives.
- **5-Tab PyQt6 Desktop Suite**:
  - Tab 1: Lean Flight Readiness Dashboard (updates, third-party apps, system health, disk state).
  - Tab 2: System Maintenance (interactive orphan triage, mirror ranking, safe maintenance, deep clean, `.pacnew` manager).
  - Tab 3: Autonomous AI Copilot (Goose SRE assistant).
  - Tab 4: SRE Skill & Persona Blueprint (`sys-pilot-admin` directives editor, playbook inspector).
  - Tab 5: Settings & XDG Autostart management.
- **Zero-Root Local Triage Engine (`core/triage.py`)**: 0-token diagnostics evaluating systemd failed units, disk thresholds, pacnew files, kernel replacement, and package updates.
- **GameMode Integration**: D-Bus and `gamemoded` lock detection to completely inhibit background telemetry and notifications during gaming sessions.
- **System Tray Sentinel Daemon (`bin/syspilot-sentinel`)**: Background monitor with dynamic colored tray icons and non-intrusive desktop notifications.
- **CLI Fast-Paths**: Terminal scorecard (`syspilot -s`), fast JSON triage (`syspilot -j`), and direct passthroughs to `sys-health.sh` modules (`--audit`, `--upgrade`, `--maintenance`, `--orphans`, `--mirrors`, `--gaming`).
- **Onboarding Wizard**: 30-second layman setup wizard for configuring AI Copilot providers (Google Gemini, OpenAI, Anthropic, Ollama).
