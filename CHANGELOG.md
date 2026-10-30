# Changelog

All notable changes to the **SysPilot** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
