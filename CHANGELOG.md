# Changelog

All notable changes to the **SysPilot** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
