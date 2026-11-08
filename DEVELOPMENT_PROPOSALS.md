# SysPilot: Development Proposals & Pending Patches

This document maintains the official architectural roadmap, proposed features, and SRE improvement backlog for the public **SysPilot** repository ([https://github.com/Gbur79/SysPilot](https://github.com/Gbur79/SysPilot)).

Every entry follows strict SRE standards: status tracking, engineering justification, blast-radius analysis, and a drop-in architectural blueprint.

---

## Status Legend
* `[PROPOSED]` — Validated in staging/test nodes (e.g. `sysPilot_Karol`), pending public release.
* `[IN PROGRESS]` — Accepted architectural change currently being implemented in `main`.
* `[IMPLEMENTED]` — Shipped and verified in public release (tagged with version and commit hash).
* `[REJECTED]` — Formally dismissed with technical rationale to avoid future churn.

---

## Patch Registry

### PATCH-001: Dynamic AI Model Discovery & Surgical Stream-JSON Output for Goose Copilot
* **Status:** `[IMPLEMENTED]` *(Shipped in v1.0.1)*
* **Date Proposed:** 2026-10-04
* **Priority:** HIGH (UX clarity, LLM provider agnosticism, zero console output pollution)
* **Target Components:**
  - `core/copilot_config.py` (`get_active_model_details()`, dynamic model discovery for `get_skill_info()`)
  - `gui/syspilot_gui.py` (`CopilotWorker` stream-json parser, `header_model_badge`, Copilot status indicator)
  - `copilot/recipe.yaml` (removal of hardcoded model/provider constraints, SRE rules 9 & 10)
  - `bin/syspilot` (CLI flag `--skill` reporting real-time active model)

#### Context & Root Cause (Engineering Justification):
1. **Model Opacity for End Users:**
   After completing the setup wizard, users had no immediate visual confirmation of which LLM provider or model was actively servicing queries (e.g. Gemini 3.8 Flash, GPT-4o, Claude 3.5 Sonnet, or local Ollama).
2. **Hardcoded Recipe Constraints:**
   `copilot/recipe.yaml` originally contained static configuration defaults (`goose_provider: "google"`, `goose_model: "gemini-3.8-flash"`). When users selected OpenAI, Anthropic, or Ollama in the setup wizard, Goose could encounter configuration conflicts rather than respecting the active profile in `~/.config/goose/config.yaml`.
3. **Telemetry Console Flooding in GUI:**
   The desktop GUI worker previously executed `goose run ... -q` and dumped raw stdout directly into the `QTextEdit` chat view. When the AI agent performed background system triage (e.g. reading configuration files, running `journalctl`, or inspecting multi-kilobyte JSON telemetry), the entire unformatted dump was pasted into the chat window, overwhelming the user and burying the actual answer.

#### Proposed Solution & Architecture:

##### 1. Dynamic Zero-Cost Model Resolver (`core/copilot_config.py`):
Implement a lightweight, token-free parser reading `~/.config/goose/config.yaml`:
```python
def get_active_model_details() -> Dict[str, Any]:
    """Retrieve active provider, model name, and thinking effort from Goose configuration."""
    ...
```
Update `get_skill_info()` and `bin/syspilot --skill` so that the model target dynamically reports runtime truth rather than a static string.

##### 2. Stream-JSON Event Processing (`gui/syspilot_gui.py`):
Refactor `CopilotWorker` to execute Goose in structured event stream mode (`--output-format stream-json --no-session`):
* **Surgical Diagnostic Indicators (`toolRequest`):** Instead of dumping raw shell outputs, emit clean, single-line progress badges:
  `⚙️ [Diagnostic Check] shell: ping -c 1 192.168.1.1`
* **Clean Text Streaming (`text`):** Stream the synthesized markdown answer under `💬 Copilot Response:`.
* **Execution Telemetry Footer (`complete`):** Report execution metrics cleanly at the bottom:
  `✔ Analysis Complete | Model: <model> | Total Tokens: <count> | Cost: $<usd>`

##### 3. Visual Status Badges in GUI:
* **Main Window Header:** Add an active model badge next to the Refresh button:
  `[ 🤖 gemini-3.8-flash ]` (with rich tooltip indicating provider and config path).
* **AI Copilot Tab:** Update the connection indicator:
  `🟢 Active Model: gemini-3.8-flash (Google Gemini) | Token-Lean Mode Active`.

##### 4. Provider-Agnostic SRE Recipe (`copilot/recipe.yaml`):
* Remove `goose_provider` and `goose_model` from the recipe `settings:`, delegating model selection entirely to the user's active Goose profile.
* Add SRE guardrails:
  - *Rule 9:* Strictly forbid dumping raw configuration files, bash scripts, or unparsed JSON into stdout.
  - *Rule 10:* Answer conversational or metadata questions directly without invoking intrusive system diagnostic sweeps.

#### Implementation & Verification Checklist:
- [x] Staged and validated on local hardware node (`sysPilot_Karol`).
- [x] Apply changes to `core/copilot_config.py`.
- [x] Apply changes to `gui/syspilot_gui.py`.
- [x] Apply changes to `copilot/recipe.yaml`.
- [x] Verify syntax compilation with `python3 -m py_compile`.
- [x] Validate CLI output with `syspilot --skill`.
- [x] Push to public GitHub repository (`origin/main`).

---

### PATCH-002: Dashboard Hygiene Visibility, Stale Cache Erasure Fix & Cold-Startup Autotriage
* **Status:** `[IMPLEMENTED]`
* **Date Proposed:** 2026-10-05
* **Priority:** HIGH (Eliminates dashboard vs terminal upgrade discrepancies, surfaces orphan packages & reboot alerts)
* **Target Components:**
  - `core/triage.py` (`check_orphan_packages()`, non-destructive cache preservation in `run_triage()`, `checkupdates` lock auto-heal)
  - `gui/syspilot_gui.py` (`setup_lean_dashboard_tab()`, `update_ui_from_state()`, non-blocking startup triage `_check_initial_refresh()`, 1-hour periodic package refresh)
  - `bin/syspilot` (CLI `-s` terminal flight readiness scorecard with orphan package and reboot status)
  - `bin/syspilot-sentinel` (reboot alert notifications and persistent non-destructive caching)

#### Context & Root Cause (Engineering Justification):
1. **Destructive Cache Clobbering on Periodic Light Triage:**
   The GUI periodic timer (running every 5 minutes) invoked `run_triage(check_pkgs=False)`. That light triage run previously generated an empty `updates: {"total": 0, ...}` payload and unconditionally overwrote `~/.local/state/syspilot/status.json` on disk. As a consequence, valid package update telemetry detected during a manual scan was erased within 5 minutes, resulting in the dashboard reporting "0 pending updates" while terminal upgrade tools found multiple updates.
2. **Cold-Startup Blindness:**
   When launched via autostart or tray (`syspilot --tray`), the dashboard simply read stale disk cache without queuing an initial asynchronous background package check.
3. **Total Blindness to Orphan Packages on the Dashboard:**
   While `bin/sys-health.sh` in the terminal alerted users to unrequired orphan packages during pre-flight checks, `core/triage.py` and the GUI dashboard completely lacked orphan package detection, leaving users unaware of dependency bloat.
4. **Missing Pending Reboot and Pacnew Indicators on Dashboard:**
   Reboot flags (running kernel replaced on disk) and configuration file conflicts (`.pacnew`) were buried or absent from the main Flight Readiness card.

#### Proposed Solution & Architecture:
1. **Orphan Package & Stale Lock Detection in `core/triage.py`:**
   - Implemented `check_orphan_packages()` using `pacman -Qtdq` (~100ms execution, zero root).
   - In `run_triage()`, preserved cached `updates` and `standalone_software` when `check_pkgs=False`.
   - Added automatic removal of stale `/tmp/checkup-db-$UID/db.lck` locks before querying `checkupdates`.
   - Updated flight readiness classification so that pending regular updates and orphans elevate the status to `PRE_FLIGHT_ATTENTION` (🟡).
2. **Dashboard UI Enrichment in `gui/syspilot_gui.py`:**
   - Added `self.reboot_lbl` (amber banner shown whenever `reboot_pending` is true).
   - Added `self.orphans_lbl` and an interactive `🗑 Prune Orphans` button directly inside the **🛡 System Health & Disk State** card.
   - Added `self.pacnew_dash_lbl` showing `.pacnew` conflicts.
   - Dynamic update badge highlighting (`#38bdf8`) when updates are pending.
   - Added non-blocking async startup refresh (`QTimer.singleShot(1000, ...)`).
   - Scheduled hourly background package refreshes during standard desktop operation.
3. **Scorecard Enrichment (`bin/syspilot -s` & `syspilot-sentinel`):**
   - Added real-time orphan count and reboot pending alerts to the terminal scorecard and desktop notifications.

#### Implementation & Verification Checklist:
- [x] Staged and verified in `Projects/sysPilot/` and `Projects/sysPilot_Karol/`.
- [x] Syntax validated via `python3 -m py_compile`.
- [x] Verified non-destructive caching with `core/triage.py --no-pkg`.
- [x] Verified CLI status output with `syspilot -s`.

---

### PATCH-003: GUM & Sys-Health Aesthetic UI Redesign (ANSI Contiguous Tables & Dynamic Severity HUD)
* **Status:** `[IMPLEMENTED]` *(Shipped in v1.0.3)*
* **Date Proposed:** 2026-10-05
* **Priority:** HIGH (Sol SRE UI certification, 100% visual parity with `sys-health.sh`, enhanced legibility for desktop users)
* **Target Components:**
  - `gui/syspilot_gui.py` (`DARK_STYLESHEET`, `create_grid_header()`, `create_grid_row()`, `setup_lean_dashboard_tab()`, `update_ui_from_state()`, dynamic double-border HUD)
  - `packaging/PKGBUILD` (bump to v1.0.3)
  - `CHANGELOG.md` (release v1.0.3)

#### Context & Root Cause:
The original SysPilot GUI suffered from low-contrast card styling with floating vertical text labels, lack of tabular alignment, and visual divergence from `sys-health.sh`. For users who value the rapid, unambiguous terminal diagnostics of `sys-health.sh`, the dashboard lacked the structured, 2-column key-value matrix and GUM ANSI palette that makes status triage effortless.

#### Architectural Solution:
1. **Dynamic Severity HUD Header**:
   - Double-border box matching Charm GUM (`border: 2px solid <color>; border-radius: 6px`).
   - Dynamic border & background color based on triage severity:
     - 🔴 Red (`#ef4444`) for `ACTION_REQUIRED`.
     - 🟡 Gold/Amber (`#ffaf00`) for `PRE_FLIGHT_ATTENTION`.
     - 🔵 Cyan (`#38bdf8`) for active `GameMode`.
     - 🟢 Green (`#4ade80`) for `FLIGHT_READY`.
   - Embedded sub-chip displaying real-time hardware telemetry: `kernel`, `uptime`, `root storage`, and active `GPU`.
2. **Contiguous Grid Tables (`render_audit_section` Alignment)**:
   - Fixed 270px component name column, ASCII vertical delimiter `│`, and monospace value column.
   - Alternating row zebra striping (`#090d15` and `#0f172a`) with subtle row dividers (`#2d3748`).
   - Standardized GUM badge colors: ANSI 82 green (`PASS ✔`), ANSI 214 gold (`UPDATE ⚠` / `WARN ⚠`), ANSI 196 red (`FAIL ✖`), ANSI 81 cyan (`INFO ℹ`).
3. **Integrated Action Toolbars**:
   - Buttons styled after Charm GUM terminal buttons: `gumBtnGreen` (`Run Guarded Upgrade`), `gumBtnCyan` (`Diagnostic Audit`, `Standalone Triage`), `gumBtnAmber` (`Prune Orphans`).

#### Verification:
- [x] Rendered and verified via live screen grab (`/home/gbur/Desktop/SysPilot_Actual_Redesign_Live.png`).
- [x] Syntax checked via `python3 -m py_compile`.
- [x] Applied to both `Projects/sysPilot` and `Projects/sysPilot_Karol`.

---

### PATCH-004: Terminal Process Lifecycle Monitor, Auto-Refresh & Gum Interactive Selection Fallback
* **Status:** `[IMPLEMENTED]` *(Shipped in v1.0.4)*
* **Date Proposed:** 2026-10-05
* **Priority:** HIGH (Fixes silent interactive selection abort in Gum, enables auto-refresh when terminal closes, distinguishes strict orphans from optional candidates)
* **Target Components:**
  - `Projects/sysPilot/bin/sys-health.sh` & `Projects/eos-cleaner/sys-health.sh` (`triage_orphan_packages()`: Gum checkbox prefixes, single-item auto-confirm on Enter, warning on empty toggle)
  - `Projects/sysPilot/core/triage.py` (`check_orphan_packages()`: dual-tier detection `pacman -Qtdq` vs `pacman -Qdttq`)
  - `Projects/sysPilot/gui/syspilot_gui.py` (`_spawn_terminal()` background waiter with `terminal_finished` Qt signal, window focus auto-refresh via `changeEvent`, `konsole --nofork`, strict vs optional label differentiation)
  - `Projects/sysPilot/bin/syspilot` (CLI `-s` output reporting strict vs optional candidates)
  - `Projects/sysPilot/CHANGELOG.md` & `PKGBUILD` (bump to v1.0.4)

#### Context & Root Cause:
1. **Dashboard Not Auto-Refreshing After Terminal Work:**
   When a user launched a terminal operation from SysPilot (e.g. `Prune Orphans`, `Guarded Upgrade`), SysPilot fired `subprocess.Popen` without waiting. When the terminal finished and closed, SysPilot was not notified, leaving stale counts on the dashboard until manually refreshed.
2. **Konsole Detached Subprocess Forking:**
   On KDE Plasma, launching `konsole -e` by default attaches to a single-process server and returns instantly, breaking standard `.wait()` calls unless `--nofork` is passed.
3. **Gum Multi-Selection Trapping (`gum choose --no-limit`):**
   In interactive selection mode (Option 2), `gum choose --no-limit` requires users to press **[SPACE]** to mark `[✔]` before pressing **[ENTER]**. If a user simply navigated to `luit` and pressed Enter, Gum returned an empty string, causing the script to abort with "No packages selected. Aborted."
4. **Distinction Between Strict Orphans and Optional Dependencies:**
   `luit` is not a strict orphan (it is optionally used by `xterm`). `pacman -Qtdq` finds 0 strict orphans, while `pacman -Qdttq` finds `luit`. Flagging `luit` as a scary orphan warning (`WARN ⚠`) confused users when strict prune found nothing to delete.

#### Architectural Solution:
1. **Interactive Checkbox Prefixes & Fallback Confirmation (`sys-health.sh`):**
   - Added `--cursor-prefix="[ ] "`, `--unselected-prefix="[ ] "`, `--selected-prefix="[✔] "`, and `--selected.foreground="82"`.
   - If user presses Enter without marking Space:
     - If only 1 package is in the list, `gum confirm --default=true "Did you want to remove '<pkg>'?"` prompts the user directly, making deletion seamless.
     - If multiple packages are present, a clear instructional warning is shown.
2. **Terminal Process Lifecycle Waiter (`gui/syspilot_gui.py`):**
   - Replaced fire-and-forget `subprocess.Popen` with `_spawn_terminal(term_cmd)`, which monitors the terminal process in a background thread and emits `self.terminal_finished` upon exit.
   - `terminal_finished` automatically triggers `self.trigger_refresh()`, updating all UI tables instantly.
   - Added `changeEvent(event)`: when the window regains focus (`ActivationChange`), it syncs from disk.
   - Passed `konsole --nofork` in `get_terminal_cmd`.
3. **Dual-Tier Orphan Telemetry (`triage.py`):**
   - Evaluates both `-Qtdq` (strict) and `-Qdttq` (optional candidate).
   - Strict orphans show as `WARN ⚠` with amber `🗑 PRUNE ORPHANS`.
   - Optional-only candidates show as `INFO ℹ (0 strict orphans • 1 optional candidate: luit)` with cyan `🗑 REVIEW CANDIDATES`, keeping the section `ALL CLEAR ✔`.

#### Verification:
- [x] Verified interactive Gum choose with single-item fallback.
- [x] Verified automatic dashboard refresh after terminal exit.
- [x] Captured live screen: `/home/gbur/Desktop/SysPilot_Orphan_AutoRefresh_Fixed.png`.

---

### PATCH-005: [IMPLEMENTED] Metadata-Gated Transient Desktop Application Unit Filtering
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.0.5)*
- **Data zgłoszenia:** 2026-10-09
- **Źródło:** SysPilot / `sys-health.sh` parity incident — KDE Plasma launcher retained failed `app-java@aafa9650aae64437abdb03e7b8454da5.service` after a one-shot Java help invocation.
- **Komponenty:**
  - `core/triage.py` (`check_failed_services()`, `get_transient_desktop_app_units()`)
  - `CHANGELOG.md`
  - regression coverage for mocked `systemctl --user` output
- **Kontekst i Uzasadnienie:**
  `sys-health.sh` omits failed user units matching `^app-.*\.(service|scope)$`, while SysPilot previously surfaced the same KDE-created transient application unit as a persistent health advisory. A direct name-only port would resolve the observed false positive but could hide legitimate persistent user daemons or generated XDG autostart services named `app-*.service`. Desktop-entry and XDG-autostart standards do not reserve this unit-name namespace.
- **Proponowane Rozwiązanie:**
  Treat the regex as a candidate selector only. Query the user manager once in batch with `systemctl --user show` and suppress a candidate only when `Transient=yes` and `Slice=app.slice`. Preserve all system-manager failures, all `run-*.scope` units, and all candidates whose metadata query fails, times out, or is incomplete. This restores the intended Plasma/sys-health outcome while failing open for diagnosability and cross-desktop portability.

---

### PATCH-006: [PROPOSED] Failed User-Unit Probe Confidence and Safe Scoped Remediation
- **Status:** `[PROPOSED]`
- **Data zgłoszenia:** 2026-10-09
- **Źródło:** PATCH-005 SRE architectural review.
- **Komponenty:**
  - `core/triage.py` (explicit system/user systemd probe availability and diagnostic reason fields)
  - `gui/syspilot_gui.py` (Inspect and selected-unit Clear Failed State workflow)
  - `bin/syspilot` (optional scoped failed-unit inspection/reset commands)
  - regression fixtures for missing `XDG_RUNTIME_DIR`, unavailable user D-Bus, timeout, and sudo/SSH contexts
- **Kontekst i Uzasadnienie:**
  Current broad exception handling makes an unavailable user manager or D-Bus session indistinguishable from “zero failed user units.” Furthermore, `systemctl --user reset-failed` only clears retained systemd state and a blanket action may erase failure evidence for unrelated services.
- **Proponowane Rozwiązanie:**
  Add explicit per-manager probe status (`available`, `unavailable`, `timeout`, and diagnostic text) so the GUI never presents an unavailable user-manager probe as a clean check. For real non-app failures, provide Inspect commands before a confirmation-gated `systemctl --user reset-failed <selected-unit...>` action. Never use `sudo` for the user-manager action; reject or safely re-exec elevated CLI invocations in the original user’s runtime context. A destructive all-unit reset, if provided, must require explicit `--all --yes`.
  *(Uwaga architektoniczna SRE: Ścisła synergia z `sys-health.sh` PATCH-035 / Pakiet 1 w zakresie precyzyjnego journalctl i reset-failed).*

---

### PATCH-007: [IMPLEMENTED] Deterministic 1-Click FAF (Forged Alliance Forever) Setup & Dynamic Desktop Shortcut Resolver
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.0.6)*
- **Data zgłoszenia:** 2026-10-09
- **Źródło:** SRE Architectural Audit (Karol / Sol / Gemini Flash) — Forged Alliance Forever runner bypassed background updates due to static versioned desktop shortcuts.
- **Komponenty:**
  - `bin/syspilot-faf-repair` (new dedicated deterministic helper with `--check` and `--repair` modes)
  - `bin/syspilot` (added `--faf-repair` and `--faf-setup` CLI fast-paths)
  - `gui/syspilot_gui.py` (enhanced 1-click Copilot SRE playbook button prompt)
  - `copilot/playbooks/faf_setup_guide.md` (added automated self-healing, dynamic desktop integration, and Scenario C troubleshooting)
  - `copilot/skills/sys-pilot-admin/SKILL.md`
- **Kontekst i Uzasadnienie:**
  Manual desktop launchers often link directly to obsolete versioned binaries (e.g. `~/faf-linux/faf-client-2026.7.0/faf-client`) instead of the dynamic wrapper script (`~/faf-linux/run`). This completely bypasses the built-in background update cycle (`update.sh autoupdate-notify`), locking players to outdated versions indefinitely even when upstream releases are downloaded. Furthermore, multi-drive Steam setups and missing `Game.prefs` synchronization regularly cause crashes on Linux.
- **Proponowane Rozwiązanie:**
  Deploy a deterministic, zero-token CLI repair and setup helper (`bin/syspilot-faf-repair`) accessible directly via `syspilot --faf-repair`. The utility validates prerequisites (`bwrap`, 32-bit Vulkan ICDs), synchronizes components via `update.sh perform`, dynamically discovers Steam libraries across multi-mount setups via `libraryfolders.vdf`, syncs `Game.prefs` with backup preservation, generates dynamic `.desktop` launchers for both XDG menu and Desktop pointing to `$HOME/faf-linux/run`, and refreshes desktop database caches.

---

### PATCH-008: [IMPLEMENTED] SysPilot / sys-health Parity — Dual-Tier Orphan Telemetry & Severity Harmonization
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.0.7)*
- **Data zgłoszenia:** 2026-10-16
- **Źródło:** SRE Architectural Audit (Sol Subagent `20261016_5`) — `kwin-x11` package discrepancy between `sys-health.sh` (INFO/ALL_CLEAR) and SysPilot (WARN/PRE_FLIGHT_ATTENTION).
- **Komponenty:**
  - `core/triage.py` (`check_orphan_packages()`, `run_triage()`, `ORPHAN_SENSITIVE_PATTERNS`)
  - `gui/syspilot_gui.py` (Orphan row rendering, button text & classes)
  - `bin/syspilot` (Scorecard `-s` orphan reporting)
  - `CHANGELOG.md` & `packaging/PKGBUILD` (bump to v1.0.7)
- **Kontekst i Uzasadnienie:**
  Following upstream KDE Plasma 6.8 deprecation of standalone X11 sessions, `kwin-x11` became unrequired. While `sys-health.sh` categorized it as an informational advisory (`INFO ℹ`) and marked it Tier 3 (heuristically sensitive), SysPilot's triage wrapper unconditionally elevated any orphan package to `PRE_FLIGHT_ATTENTION` (🟡) with a `WARN ⚠` label. This created an inconsistency where a healthy system was falsely flagged as degraded, and clicking "PRUNE ORPHANS (1)" opened a dialog offering 0 strict candidate removals.
- **Wdrożone Rozwiązanie:**
  1. Synchronized the 3-tier orphan classification model (`Tier 1`: strict unreferenced, `Tier 2`: optional for apps, `Tier 3`: sensitive desktop/boot/graphics/audio components) into `core/triage.py`.
  2. Harmonized severity: orphan presence is now strictly informational (`INFO ℹ`) and does not degrade `Overall Status` from `FLIGHT_READY` (🟢).
  3. Added fail-visible checking: only pacman execution errors or timeouts trigger `WARN ⚠` and elevate flight attention.
  4. Updated GUI and CLI to distinguish between Tier 1 prune candidates (`PRUNE ORPHANS`) and Tier 2/3 review candidates (`REVIEW CANDIDATES`).
  5. Enforced atomic write persistence for `status.json`.



---

### PATCH-009: [IMPLEMENTED] Ultra-Lightweight 24/7 Rust Tray Sentinel, Native D-Bus (`zbus 5.19`), Single-Instance Lock & Zero-Lag IPC
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.1.0)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** SRE Architectural Audit (Master Plan v2.1) — Elimination of high CPU/RAM overhead, double-window spawn bug, and Amber/Green status desynchronization lag.
- **Komponenty:**
  - `rust-tray/Cargo.toml` (`zbus 5.19`, `ksni 0.3`, `tokio`, `libc 0.2`)
  - `rust-tray/src/sentinels.rs` (non-blocking kernel reboot, pacman lockfile, `libc::statvfs` disk probes)
  - `rust-tray/src/updater.rs` (async `checkupdates` with `/tmp` isolated DB, AUR, standalone updates, GameMode freeze)
  - `rust-tray/src/gamemode.rs` (native session D-Bus calls via `zbus`, 0 fork/exec)
  - `rust-tray/src/tray.rs` (5-item flattened context menu, multi-line tooltip, 1.5s activate debounce)
  - `rust-tray/src/tests.rs` (10 hermetic unit tests with TempTestDir)
  - `gui/syspilot_gui.py` (`fcntl.flock` on `.gui.lock`, early `QLocalServer` binding, 100ms startup triage)
- **Kontekst i Uzasadnienie:**
  1. Spawning `busctl` every 3 seconds to check GameMode caused ~28,800 fork/exec cycles per day.
  2. Double-clicking the tray icon often spawned two concurrent GUI processes due to ~400ms Qt widget initialization before socket listening.
  3. Status in the tray desynchronized with the GUI dashboard due to inconsistent `effective_status` semantics and an artificial 1000ms delay.
- **Wdrożone Rozwiązanie:**
  - Full native D-Bus integration in Rust via `zbus 5.19` directly on user session bus.
  - Rust Sentinel runs continuously as `syspilot-tray.service` using only **~1.2 MB RAM and 0.0% CPU**.
  - Single-instance enforcement via atomic `fcntl.flock` and 1.5s tray debounce.
  - Aligned status semantics ensuring instantaneous triage refresh.

---

### PATCH-010: [IMPLEMENTED] Dynamic Hardware Platform & GPU Row, Universal Decoupled AI Copilot & External Engine Decoupling
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.1.0)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** User Request & SRE Architectural Governance — Removal of rig-bias, universal hardware detection, decoupled copilot configuration, and clean segregation of private identities.
- **Komponenty:**
  - `gui/syspilot_gui.py` (`Hardware Platform & GPU` row under *System Health & Hygiene*, `get_hardware_summary()`)
  - `core/copilot_config.py` (`get_user_copilot_config()`, dynamic `is_copilot_ready()`, `get_active_model_details()`)
  - `bin/syspilot` (`SYSPILOT_COPILOT_CMD` CLI runner support, removed embedded `bin/sys-health.sh`)
  - `packaging/PKGBUILD` (version bump to 1.1.0, cargo release build, `syspilot-tray.service`)
  - `copilot/skills/sys-pilot-admin/SKILL.md` (clean public SRE skill without private rig leaks)
  - `copilot/quirks/maxwell_nvidia.md` (universalized DP 1.2 MST daisy-chain bandwidth documentation)
- **Kontekst i Uzasadnienie:**
  1. The GUI lacked a dedicated hardware status row, making it impossible to see GPU/CPU state when hardware warnings were flagged.
  2. The Copilot button was tightly coupled to hardcoded tool names, preventing users from connecting custom agents or private runners without modifying git-tracked files.
  3. Embedded engine copies in `bin/sys-health.sh` created maintenance drift against canonical external tools.
- **Wdrożone Rozwiązanie:**
  - Implemented dynamic hardware detection probing `/proc/cpuinfo` and `nvidia-smi`/`lspci` in <11ms without hardcoding.
  - Introduced configuration resolver for `~/.config/syspilot/config.env` (`SYSPILOT_COPILOT_CMD`, `COPILOT_LABEL`, `SYSPILOT_GOOSE_RECIPE`).
  - Purged private rig identifiers from public repository code while supporting private overrides locally.
  - Fully decoupled `sys-health` as an external canonical tool.

---

### PATCH-011: [IMPLEMENTED] Arch Linux Security Advisories & CVE Severity Highlighting (arch-audit Integration)
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.2.0)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** SRE Architectural Roadmap & User Initiative (HowToGeek Security Integration)
- **Komponenty:**
  - `core/triage.py` (`check_security_vulnerabilities()`, integration with `arch-audit -u --json`)
  - `gui/syspilot_gui.py` (Security Vulnerabilities row in *System Updates*, dynamic High/Critical CVE badges, `[🔍 INSPECT CVE]`)
  - `rust-tray/src/state.rs` & `rust-tray/src/tray.rs` (security flag in Tooltip and NeedsAttention escalation)
- **Kontekst i Uzasadnienie:**
  SysPilot currently aggregates total package update counts, but treats a benign utility upgrade identically to a zero-day remote code execution vulnerability patch in `chromium`, `systemd`, or `glibc`. The official `arch-audit` package queries Arch Linux Security Tracker data (`security.archlinux.org`). Integrating this empowers users with actionable security intelligence.
- **Proponowane Rozwiązanie:**
  1. Add token-free, non-blocking check `arch-audit -u --json` in `core/triage.py` with 3-second execution timeout.
  2. If pending updates contain High Risk CVEs, display a prominent warning banner and badge in *System Updates*:
     `Security Advisories (CVE) │ ALERT 🚨 (N High-Risk CVEs in <packages>) [INSPECT ADVISORIES]`.
  3. If all updates are non-security or no updates exist:
     `Security Advisories (CVE) │ PASS ✔ (No known unpatched CVEs)`.
  4. Escalate tray icon to `dialog-warning` when fixable high-risk security advisories are detected.

---

### PATCH-012: [IMPLEMENTED] Hardware Firmware & Peripherals Telemetry & Safe Update Action (fwupd Integration)
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.2.0)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** SRE Architectural Roadmap & User Initiative (Arch Wiki Fwupd / LVFS)
- **Komponenty:**
  - `core/triage.py` (`check_firmware_updates()`, query via `fwupdmgr get-updates --no-unreported-check --json`)
  - `gui/syspilot_gui.py` (Hardware Firmware row in *Third-Party Updates*, interactive `[⚡ UPGRADE FIRMWARE]` button launching terminal runner)
  - `rust-tray/src/state.rs` (firmware telemetry fields)
- **Kontekst i Uzasadnienie:**
  Standard Arch Linux package updates completely ignore device firmware (UEFI microcode, NVMe/SATA SSDs, wireless dongles like Logitech Unifying, TPMs). The machine has `fwupd 2.1.9` installed with 5 detected updateable devices. Without firmware monitoring, critical storage controller bugs and wireless vulnerabilities remain unpatched indefinitely.
- **Proponowane Rozwiązanie:**
  1. Add firmware query in `core/triage.py` executing `fwupdmgr get-updates --no-unreported-check --json` safely with fallback when daemon is inactive.
  2. Surface a dedicated row in GUI under *Third-Party Updates*:
     `Hardware Firmware (fwupd) │ PASS ✔ (All devices up to date)` or `UPDATE ⚠ (ADATA SSD / Logitech Receiver)` with interactive `UPGRADE FIRMWARE` action.
  3. Safe guarded execution: firmware flashing is performed inside an explicit terminal window with battery/power safeguards.

---

### PATCH-013: [IMPLEMENTED] Host Firewall Health & Shield Inspection Row (Multi-Backend)
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.2.0)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** SRE Architectural Roadmap & User Initiative (HowToGeek Firewall Hardening)
- **Komponenty:**
  - `core/triage.py` (`check_host_firewall()`, multi-backend probing `firewalld`/`ufw`/`nftables`)
  - `gui/syspilot_gui.py` (Host Firewall row under *System Health & Hygiene*)
  - `rust-tray/src/state.rs` (firewall telemetry fields)
- **Kontekst i Uzasadnienie:**
  While perimeter defense is handled by the upstream pfSense router, modern host security requires endpoint firewall verification. The system runs `firewalld` (`firewalld.service active`), but SysPilot provided zero visibility into whether the host firewall is active, which zone is active, or if rogue processes opened listening ports.
- **Proponowane Rozwiązanie:**
  1. Add `check_host_firewall()` inspecting `firewalld`, `ufw`, or `nftables` state in <5ms without D-Bus blocking.
  2. Surface telemetry row:
     `Host Firewall (Shield) │ PASS ✔ (Active: firewalld • default zone: public)`.
  3. Universalism standard: report `INFO ℹ` if no host firewall is active (perimeter protected).

---

### PATCH-014: [IMPLEMENTED] On-Demand Package Pre-Caching (Instant Upgrade UX)
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.2.0)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** SRE Architectural Roadmap & User Initiative (man checkupdates.8.en)
- **Komponenty:**
  - `gui/syspilot_gui.py` (`[⚡ PRE-CACHE PACKAGES]` action button, terminal download runner)
- **Kontekst i Uzasadnienie:**
  Downloading multi-gigabyte package archives during an interactive upgrade locks the package manager for minutes. By providing an interactive pre-caching action, packages can be downloaded into `/var/cache/pacman/pkg` before initiating the full transaction.
- **Proponowane Rozwiązanie:**
  1. Add interactive `[⚡ PRE-CACHE PACKAGES]` button in GUI Dashboard *System Updates*.
  2. Execute pre-caching in terminal with progress visibility.
  3. Zero sudo in Rust background daemon.

---

### PATCH-015: [IMPLEMENTED] Universal Secure Boot Resilience & Hardware Quirk Classification in fwupd Firmware Triage
- **Status:** `[IMPLEMENTED]` *(Shipped in v1.2.1)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** SRE Architectural Roadmap & User Incident Triage (`efivarsfs: Invalid argument` on MSI Z97)
- **Komponenty:**
  - `core/triage.py` (`is_secure_boot_enabled()`, `check_firmware_updates()` with NVRAM quirk classifier)
  - `gui/syspilot_gui.py` (Firmware status rendering with bypassed quirk count and tooltip)
- **Kontekst i Uzasadnienie:**
  On older UEFI platforms (e.g. Haswell / MSI Z97 / AMI UEFI circa 2014) where Secure Boot is disabled (`bootctl status` -> `Secure Boot: disabled`), attempting to flash large 2023 Microsoft KEK certificates fails with `efivarsfs: Error writing to file descriptor: Invalid argument` due to platform NVRAM write size constraints. Without smart classification, SysPilot entered an infinite warning loop for an irrelevant certificate that cannot be written.
- **Proponowane Rozwiązanie:**
  1. Pure Python, non-root, instant (<0.05ms) Secure Boot detection via `/sys/firmware/efi/efivars/SecureBoot-*`.
  2. Universal quirk classifier in `check_firmware_updates()`: when Secure Boot is disabled and a certificate update fails due to NVRAM limits, classify it into `bypassed_quirks` rather than `pending_devices`.
  3. If Secure Boot is enabled on modern systems, failures remain flagged as actionable alerts.
  4. Display status in GUI: `PASS ✔ (Hardware up to date • 1 harmless UEFI quirk bypassed)`.

---

### PATCH-016: [APPROVED] Full Native Rust GUI & Triage Engine (Eliminating Python & PyQt6 Runtime via Slint)
- **Status:** `[COMPLETED]` *(SRE Certified — Audit `SRE-AUDIT-20261030-PATCH-016-SLINT-MIGRATION-V1`, `SRE-AUDIT-20261030-PATCH-016-PHASE-1-2-POST`, `SRE-AUDIT-20261030-PATCH-016-PHASE-3-POST`, `SRE-AUDIT-20261030-PATCH-016-MULTI-AGENT-POST`, `SRE-AUDIT-20261030-PATCH-016-PHASE-4-POST`, `SRE-AUDIT-20261030-PATCH-016-UI-POLISH-POST`)*
- **Data zgłoszenia:** 2026-10-30
- **Źródło:** Long-Term Architectural Vision (v2.0.0 Milestone) & SRE Audit (Auditor Argon Pro)
- **Audyty SRE:**
  - `SRE-AUDIT-20261030-PATCH-016-SLINT-MIGRATION-V1` (Auditor Argon): Blueprint architektoniczny Cargo Workspace, Slint dual-renderers, zero-hallucination Serde / Context7.
  - `SRE-AUDIT-20261030-PATCH-016-PHASE-1-2-POST` (Auditor Argon): Audyt powdrożeniowy Faz 1 & 2. Triage rozbieżności amber-warning w trayu vs green GUI. Wdrożenie resilient Serde (`#[serde(default)]`, alias `avail_gb`), fail-closed UI w Slint (`has_error`, `error_message`, neutral `--`), hermetyczny test kompatybilności Schema 1.0/2.0 (`test_deserialize_schema_1_0_disk_compatibility`). Testy: 24/24 PASS.
  - `SRE-AUDIT-20261030-PATCH-016-PHASE-3-POST` (Auditor Argon): Audyt powdrożeniowy Fazy 3 (Maintenance & Copilot). Wykrycie i naprawa FM-1 (wzajemne wykluczanie flag `--text` i `--recipe` w Goose CLI 1.57+ oraz wstrzykiwanie parametrów `user_query`, `health_context`, `working_directory`), FM-2 (likwidacja paniki `unreachable!()` w `build_terminal` dla niestandardowych terminali z universal fallbackiem), FM-3 (dynamiczna kaskada rezolucji receptury Goose), FM-4 (debouncing przycisków playbooków w Slint i `AtomicBool` w Rust). Testy: 28/28 PASS.
  - `SRE-AUDIT-20261030-PATCH-016-MULTI-AGENT-POST` (Auditor Argon): Audyt adaptera CLI agentów AI (`8a28b73` ➔ `b85dbf3`). Wykrycie 4 wad: FM-A (uszkadzanie flag `-i` w substringach nazw agentów i flag typu `--interactive`), FM-B (fałszywa kolizja substringu `-p` z `--project`/`--profile`), FM-C (POSIX word-splitting w `bin/syspilot`), FM-D (nadpisywanie zmiennych środowiskowych procesu plikiem z dysku). Blueprint wdrożył dyskretny token parser, tablicową ewaluację w bashu i testy higieny flag (`test_agy_command_identification`, `test_agy_flag_adaptation_hygiene`). Decyzja: CERTIFIED [Argon] dla `b85dbf3`. Testy: 31/31 PASS.
  - `SRE-AUDIT-20261030-PATCH-016-PHASE-4-POST` (Auditor Argon): Audyt wdrożenia Fazy 4 (Packaging Cutover & Python Retirement `0aae6c6` ➔ `346390d`). Wykrycie 5 wad w commitcie `0aae6c6`: FM-1 (izolacja `syspilot-faf-repair` w `/usr/lib/syspilot/bin` poza `$PATH` i brak opcji `--faf-repair` w routerze), FM-2 (instalacja launchera tacki w `/usr/share/applications/syspilot.desktop` zamiast kokpitu GUI), FM-3 (brak `sys-health` w `optdepends`), FM-4 (ostrzeżenie SC1007 w `bin/syspilot`), FM-5 (dryf dokumentacji w `README.md`). Decyzja: REJECTED [Argon] dla `0aae6c6`. Wdrożenie blueprintu w commitcie `346390d`: dodanie funkcji `launch_faf_repair()`, rozdzielenie wpisów `.desktop` (`packaging/syspilot.desktop` z `Exec=syspilot --gui`), dodanie `sys-health` do `optdepends`, eliminacja ostrzeżeń linera i aktualizacja `README.md`. Ostateczna decyzja: **CERTIFIED [Argon]** dla `346390d`.
  - `SRE-AUDIT-20261030-PATCH-016-UI-POLISH-POST` (Auditor Argon): Audyt poprawek układu i typografii Slint (`9f96ab1`, `38de341`). Zlikwidowanie pionowego rozciągania `TabButton` (ograniczenie do `height: 36px`, `vertical-stretch: 0`), ujednolicenie szerokości kolumny przycisków w System Maintenance (`width: 175px`, `min-height: 68px`) oraz precyzyjne centrowanie pionowe i poziome etykiet w nagłówku HUD i przyciskach akcji (`height: 32px`). Decyzja: **CERTIFIED [Argon]**. Testy: 31/31 PASS.
- **Docelowe Komponenty:**
  - `Cargo.toml` (Cargo Workspace root)
  - `crates/syspilot-core/` (Pure Rust headless triage library: pacman/AUR updates, CVE arch-audit, fwupd/NVRAM, firewall, statvfs, terminal runners)
  - `crates/syspilot-tray/` (Ultralight 24/7 background sentinel daemon ~1.8 MB RSS, depending on `syspilot-core`, `ksni`, `zbus`)
  - `crates/syspilot-gui/` (On-demand Slint GUI dashboard, compiling dual renderers `renderer-femtovg` + `renderer-software` fail-safe)
  - `packaging/PKGBUILD` (Cutover to pure Rust build, dropping `python-pyqt6` dependency)
- **Kontekst i Uzasadnienie:**
  SysPilot v1.2.1 operates as a hybrid architecture: an ultralight 24/7 Rust Sentinel (~1.2 MB RAM) paired with an on-demand Python/PyQt6 GUI (~75 MB RAM, ~1.2s cold start, heavy runtime dependencies). The objective of v2.0.0 is total Python elimination, achieving a unified, ultra-fast native desktop suite consuming <15 MB RAM with instant (<50ms) window launches.
- **SRE Mandatory Guardrails (from Audit):**
  1. **Dual Renderer Architecture:** Slint must compile both `renderer-femtovg` (OpenGL GPU) and `renderer-software` (CPU rasterizer via `wl_shm`). Guarantees zero crashing during GPU context drops or DP MST mode changes on legacy NVIDIA Maxwell / KWin Wayland (`SLINT_BACKEND=software` fallback).
  2. **Wayland Window Focus & Single Instance:** Tray activation under KWin Wayland uses XDG Activation Token (`xdg_activation_v1`) via D-Bus (`org.syspilot.Gui.ActivateWindow(token)`) preventing taskbar yellow-flash focus loss.
  3. **Strict Crate Isolation:** Slint GUI code is strictly isolated in `crates/syspilot-gui`. The 24/7 daemon `crates/syspilot-tray` depends only on `syspilot-core` to guarantee micro-sized background memory footprint.
  4. **Universalism Invariant:** Zero hardcoded machine identifiers in `.slint` templates; Copilot tab remains generic `SysPilot AI Copilot` configurable via `~/.config/syspilot/config.env`.
- **Plan Etapów Wdrożenia (Phased Rollout):**
  - **Faza 1 (Core & CLI):** [DONE] Port `core/triage.py` to `crates/syspilot-core`. Headless unit tests (`cargo test -p syspilot-core`) and Schema 2.0 JSON parity.
  - **Faza 2 (Slint Dashboard MVP):** [DONE] Implement Slint Dashboard tab. Validate execution on KDE Plasma Wayland (14 MB standalone binary). Post-implementation SRE audit verified & resolved.
  - **Faza 3 (Maintenance & Copilot Parity):** [DONE] Implement Maintenance terminal runners (`terminal.rs`), Copilot console, navigation tabs, and SRE playbook triggers. Post-implementation SRE audit verified & resolved.
  - **Faza 4 (Packaging Cutover):** [DONE] Retire `gui/syspilot_gui.py` and `core/triage.py`. Update PKGBUILD to pure Rust release build. SRE blueprint hardened and certified in `346390d`.

---

### PATCH-017: [IMPLEMENTED] SysPilot Rust Tray Deduplication (flock), Autonomous Background Triage & Dynamic Kernel Telemetry Ingestion
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-20261031-INCIDENT-001`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-10-31
- **Źródło:** Incident Triage (Duplicate tray icons, stale telemetry cache `age > 6h`, "Kernel unknown")
- **Audyt SRE:**
  - `BP-ARGON-20261031-INCIDENT-001` (Auditor Argon Pro): Zatwierdzenie blueprintu architektonicznego. Wdrożenie kernel-level `libc::flock` w `syspilot-tray` i `syspilot-gui`, dodanie `X-systemd-skip=true` do autostartu desktop, wdrożenie bezwarunkowego triage przy zimnym starcie i restarcie komputera oraz periodycznego timera 15 min w `syspilot-tray`, integracja odczytu `summary.json` z silnika `sys-health.sh` oraz natywnych fallbacków do `/proc/sys/kernel/osrelease` i `/etc/os-release`.
- **Docelowe Komponenty:**
  - `crates/syspilot-core/src/models.rs` (`detect_kernel()`, `detect_os()`, `acquire_single_instance_lock()`, resilient `read_system_snapshot()`)
  - `crates/syspilot-core/src/triage.rs` (`load_sys_health()`, canonical `summary.json` ingestion)
  - `crates/syspilot-core/src/firmware.rs` (proactive Secure Boot certificate quirk classifier on disabled Secure Boot)
  - `crates/syspilot-tray/src/main.rs` (flock deduplication, startup triage, 15-minute background poller)
  - `crates/syspilot-gui/src/main.rs` (shared flock deduplication)
  - `autostart/syspilot.desktop` (`X-systemd-skip=true`)
  - `Cargo.toml` (Bump version to `0.2.3`)
- **Kontekst i Uzasadnienie:**
  Po wycofaniu pythonowego `syspilot-sentinel` w Fazie 4, czysty Rust v2 nie posiadał demona odświeżającego telemetrię w tle (triage działał tylko na kliknięcie w GUI). Po wyłączeniu komputera na noc, poranny start wczytywał cache starszy niż 6 godzin, co wywoływało fałszywy alarm `DATA UNAVAILABLE: Telemetry cache stale (age 38344s)` i pomarańczowy trójkąt w tacce. Ponadto brak `flock` i równoległe istnienie usługi systemd oraz pliku autostartu XDG w KDE Plasma 6 skutkowało odpaleniem dwóch procesów tray i dwóch ikon. Trzecią wadą było hardkodowane `sys_health: None` w `triage.rs`, skutkujące etykietą "Kernel unknown".
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 32/32 unit tests PASS (w tym nowy test `test_dynamic_kernel_and_os_fallback`), Clippy clean, fmt clean.

---

### PATCH-018: [IMPLEMENTED] SysPilot GUI Live Telemetry Watcher Thread Lifetime & Event Loop Dispatch
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-20261031-SYS-010-WATCHER`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-10-31
- **Źródło:** Bug Report / Live Workstation Triage (Dashboard nie odświeża się automatycznie po zakończeniu `RUN GUARDED UPGRADE`)
- **Audyt SRE:**
  - `BP-ARGON-20261031-SYS-010-WATCHER` (Auditor Argon Pro): Zatwierdzenie blueprintu architektonicznego. Wymiana nieprawidłowego wywołania `ui_weak_watcher.upgrade()` w wątku roboczym tła (zwracającego `None` z powodu ograniczenia thread-affinity Slinta) na atomową flagę cyklu życia okna `gui_running: Arc<AtomicBool>` oraz bezpieczną dyspozycję UI przez `ui_weak_watcher.upgrade_in_event_loop(...)`.
- **Docelowe Komponenty:**
  - `crates/syspilot-gui/src/main.rs` (1Hz live watcher thread lifecycle, `upgrade_in_event_loop`, graceful thread shutdown on window close)
  - `CHANGELOG.md` (wpis v2.0.1)
  - `packaging/PKGBUILD` (synchronizacja `pkgver=2.0.1`)
- **Kontekst i Uzasadnienie:**
  Po uruchomieniu aktualizacji jądra i sterowników przez opcję "RUN GUARDED UPGRADE", proces aktualizacji w terminalu zakończył się sukcesem, a `syspilot-tray` poprawnie zaktualizował plik stanu `status.json` do 0 oczekujących aktualizacji. Mimo to otwarte okno GUI pozostawało zamrożone na starym stanie sprzed aktualizacji. Diagnoza wykazała, że wątek tła monitorujący `status.json` w `syspilot-gui` kończył działanie (`break`) już w 1. sekundzie po otwarciu okna z powodu ewaluacji `ui_weak_watcher.upgrade().is_none()`. W Slint 1.18 metoda `Weak::upgrade()` zwraca `None`, jeśli nie jest wywołana na wątku tworzącym okno.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 32/32 unit tests PASS, Clippy clean, fmt clean. Produkcyjna instalacja `~/.local/bin/syspilot-gui` zweryfikowana.

---

### PATCH-019: [IMPLEMENTED] Sprint 1: Process Concurrency, Reaper Battery Optimization & Underflow Immunity
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-SPRINT-01-TERMINAL-001`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-01
- **Źródło:** SysPilot SRE Audit Sprint 1 (`crates/syspilot-core/src/terminal.rs`)
- **Audyt SRE:**
  - `BP-ARGON-SPRINT-01-TERMINAL-001` (Auditor Argon Pro): Wykrycie i naprawa krytycznego błędu niedomiaru slotów (`ACTIVE_TERMINALS` underflow do `usize::MAX`) wywoływanego przez procesy `spawn_detached_gui`, eliminacja niepotrzebnych 345 600 wybudzeń CPU na dobę przez zastąpienie 250ms pollingu blokującym `rx.recv()`, zabezpieczenie `spawn_syspilot_task` przed shell word-splitting (`exec syspilot "$@"`), dodanie obsługi `wezterm` oraz 2 testów jednostkowych weryfikujących odporność slotów.
- **Docelowe Komponenty:**
  - `crates/syspilot-core/src/terminal.rs` (`TrackedProcess`, saturating `release_slot()`, zero-idle `reaper()`, safe bash fallback, wezterm support)
  - `crates/syspilot-core/src/tests.rs` (`test_terminal_slot_bounds_and_underflow_immunity`, `test_detached_gui_reaper_isolation`)
  - `CHANGELOG.md`
- **Kontekst i Uzasadnienie:**
  W trakcie audytu Sprint 1 zidentyfikowano ukrytą bombę z opóźnionym zapłonem: uruchomienie okna GUI z poziomu zasobnika i jego późniejsze zamknięcie wywoływało dekrementację licznika terminali mimo braku wcześniejszej rezerwacji slotu. W rezultacie licznik przekręcał się do `18446744073709551615`, trwale blokując otwieranie jakichkolwiek okien terminala z komunikatami SRE. Dodatkowo pętla żniwiarza niepotrzebnie wybudzała procesor co 250 ms, drenując baterię.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 35/35 unit tests PASS (core: 22, tray: 12, gui: 1), Clippy clean, fmt clean.

---

### PATCH-020: [IMPLEMENTED] Deterministic Level-Triggered ALPM Persistence & Post-Upgrade Triage Callbacks
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `EOS-SRE-ARCH-P1` / `INC-20261101-0851`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-01
- **Źródło:** Live Workstation Incident (Krótka aktualizacja 4 pakietów trwająca 1s/5s została pominięta przez okresowy 10-sekundowy timer `syspilot-tray`)
- **Audyt SRE:**
  - `EOS-SRE-ARCH-P1` (Auditor Argon Pro): Likwidacja luki próbkowania Nyquista w monitorowaniu `db.lck`. Wprowadzenie śledzenia trwałego czasu modyfikacji bazy ALPM (`/var/lib/pacman/local`) oraz `summary.json`, wprowadzenie asynchronicznych callbacków wyjścia procesu w `syspilot-core::terminal` (`TrackedProcess::TerminalWithCallback`), natychmiastowa synchronizacja GUI po zamknięciu terminala upgrade oraz nasłuch zmian `summary.json` w wątku watchera GUI.
- **Docelowe Komponenty:**
  - `crates/syspilot-core/src/sentinels.rs` (`pacman_db_mtime`, `summary_mtime`, `get_pacman_db_mtime()`, `get_summary_mtime()`)
  - `crates/syspilot-core/src/terminal.rs` (`TerminalWithCallback`, `spawn_terminal_command_with_callback`, `spawn_syspilot_task_with_callback`)
  - `crates/syspilot-tray/src/main.rs` (detekcja `pacman_db_changed` i `summary_changed`)
  - `crates/syspilot-gui/src/main.rs` (`trigger_task_with_post_triage`, podwójny watcher `status.json` i `summary.json`)
  - `crates/syspilot-core/src/tests.rs` (`test_terminal_callback_executed_on_exit`)
  - `CHANGELOG.md`
- **Kontekst i Uzasadnienie:**
  Zdarzenie z 08:51 ujawniło problem próbkowania impulsów: transakcja pacmana dla 4 małych pakietów trwała zaledwie 1 sekundę (5 sekund z mirror sync). Przy timerze 10 s plik `db.lck` pojawiał się i znikał pomiędzy tyknięciami zegara, przez co `pacman_unlocked` nigdy nie stawało się prawdą. Równocześnie `sys-health.sh` wygenerował `summary.json` o 08:51:40 ze statusem `ALL_CLEAR`, lecz żaden komponent nie reagował na ten plik. Nowa architektura czteropoziomowa opiera się na stanach trwałych (*level-triggered*), gwarantując natychmiastowe odświeżenie widoku i zniknięcie żółtego trójkąta.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 36/36 unit tests PASS (core: 23, tray: 12, gui: 1), Clippy clean, fmt clean. Test dymny na stacji roboczej: `status.json` zaktualizowany natychmiast do `FLIGHT_READY` (0 updates).

---

### PATCH-021: [IMPLEMENTED] Sprint 1 100% Completion: Copilot Anti-Hang Timeout & Subprocess Reaping Hardening
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-SPRINT-01-COMPLETION-001`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-01
- **Źródło:** SysPilot SRE Audit Plan (`SRE_AUDIT_PLAN.md` Sprint 1 Completion)
- **Audyt SRE:**
  - `BP-ARGON-SPRINT-01-COMPLETION-001` (Auditor Argon Pro): Likwidacja ryzyka blokowania wątków i trwałego zablokowania flagi `COPILOT_IN_FLIGHT = true` przy braku odpowiedzi CLI/API. Wprowadzenie asynchronicznej obsługi Tokio z twardym timeoutem (domyślnie 90s, konfigurowalnym przez `SYSPILOT_COPILOT_TIMEOUT_SECS`) i automatycznym zabijaniem procesu potomnego (`kill_on_drop(true)`). Wprowadzenie `spawn_shell_task_with_callback` w `terminal.rs` dla pełnej symetrii z zadaniami syspilot. Dodanie hermetycznych testów weryfikujących zwalnianie slotów i brak procesów zombie przy nagłym SIGKILL.
- **Docelowe Komponenty:**
  - `crates/syspilot-core/src/copilot.rs` (`default_copilot_timeout`, `execute_copilot_query_with_timeout`, `execute_copilot_query_async`, `run_command_with_timeout`)
  - `crates/syspilot-core/src/terminal.rs` (`spawn_shell_task_with_callback`)
  - `crates/syspilot-core/src/tests.rs` (`test_killed_child_reaped_and_slot_freed`, `test_copilot_timeout_enforcement`)
  - `CHANGELOG.md`
  - `AUDIT_MATRIX.md`
  - `SRE_AUDIT_PLAN.md`
- **Kontekst i Uzasadnienie:**
  Wcześniejsza implementacja `execute_copilot_query` wywoływała proces synchronicznie bez limitu czasu. Zawieszenie połączenia sieciowego lub oczekiwanie na interaktywne wejście blokowało wątek tła GUI na stałe, uniemożliwiając kolejne zapytania do Copilota aż do restartu okna. Nowa architektura gwarantuje natychmiastowe ubicie zawieszonego procesu i bezpieczne wyczyszczenie stanu w UI.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 38/38 unit tests PASS (core: 25, tray: 12, gui: 1), Clippy clean, fmt clean.

---

### PATCH-022: [IMPLEMENTED] Sprint 2 100% Completion: Network & Pacman I/O Hardening
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-SPRINT-02-001`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-01
- **Źródło:** SysPilot SRE Audit Plan (`SRE_AUDIT_PLAN.md` Sprint 2 Completion)
- **Audyt SRE:**
  - `BP-ARGON-SPRINT-02-001` (Auditor Argon Pro): Likwidacja luki nieograniczonego oczekiwania na procesy I/O i sieć w `updater.rs`. Wprowadzenie `run_command_with_timeout` (15s dla `checkupdates`/`yay`, 10s dla `sys-health`) z `cmd.stdin(Stdio::null())` i `cmd.kill_on_drop(true)`. Rozszerzenie `kill_on_drop(true)` na `security.rs`, `firmware.rs` i `orphans.rs`. Likwidacja wyścigu TOCTOU wokół `db.lck` w `sentinels.rs` (`check_pacman_lock_at`) przez weryfikację PID i powtórne sprawdzenie istnienia pliku po skanowaniu `/proc`. Rozszerzenie klasyfikacji quirków firmware w `firmware.rs` o błędy braku miejsca w NVRAM i listy odwołań certyfikatów. Dodanie 4 hermetycznych testów jednostkowych (offline mode, timeout kill, TOCTOU lock, NVRAM quirk).
- **Docelowe Komponenty:**
  - `crates/syspilot-core/src/updater.rs` (`run_command_with_timeout`, `default_checkupdates_timeout`, `default_standalone_timeout`, `is_process_running`)
  - `crates/syspilot-core/src/security.rs` (`cmd.kill_on_drop(true)`, `default_security_timeout`)
  - `crates/syspilot-core/src/firmware.rs` (`cmd.kill_on_drop(true)`, `default_firmware_timeout`, NVRAM error quirks)
  - `crates/syspilot-core/src/sentinels.rs` (`check_pacman_lock_at`, TOCTOU defense)
  - `crates/syspilot-core/src/orphans.rs` (`cmd.kill_on_drop(true)`)
  - `crates/syspilot-core/src/tests.rs` (`test_subprocess_timeout_enforcement_and_kill_on_drop`, `test_updater_offline_mode_resilience`, `test_pacman_lock_toctou_resilience`, `test_fwupd_nvram_quirk_insufficient_space`)
  - `CHANGELOG.md`
  - `AUDIT_MATRIX.md`
  - `SRE_AUDIT_PLAN.md`
- **Kontekst i Uzasadnienie:**
  Zawieszenie połączenia sieciowego podczas sprawdzania aktualizacji oficjalnych repozytoriów lub AUR mogło trwale zablokować pętlę roboczą demona zasobnika lub wątki GUI. Ponadto, brak flagi `kill_on_drop(true)` powodował wycieki procesów potomnych w tabeli procesów Linuksa. Nowy moduł gwarantuje deterministyczne limity czasu, czyste ubijanie procesów i zero fałszywych alarmów o osieroconych blokadach pacmana.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 42/42 unit tests PASS (core: 29, tray: 12, gui: 1), Clippy clean, fmt clean.

---

### PATCH-023: [IMPLEMENTED] Sprint 3 100% Completion: Telemetry & State Ingestion Hardening
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-SPRINT-03-001`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-01
- **Źródło:** SysPilot SRE Audit Plan (`SRE_AUDIT_PLAN.md` Sprint 3 Completion)
- **Audyt SRE:**
  - `BP-ARGON-SPRINT-03-001` (Auditor Argon Pro): Hartowanie trwałości telemetrii, odporności na NTP/clock-skew, wielowątkowego zapisu atomowego i deserializacji Serde. Wprowadzenie `CLOCK_SKEW_TOLERANCE_SECS = 60` w `models.rs` eliminujące fałszywe alarmy `DATA DEGRADED ⚠` przy cofnięciu zegara. Dodanie funkcji `read_system_snapshot_or_fallback` odpornej na zniekształcone dane. Wprowadzenie unikalnego nazewnictwa plików tymczasowych (`.{name}.tmp.{pid}.{seq}.{nanos}`) z `sync_all()` na pliku i katalogu nadrzędnym w `triage.rs`. Różnicowanie statusu `user_manager_available` w `services.rs` zapobiegające fałszywym alarmom o awarii usług systemowych. Rozszerzenie `kill_on_drop(true)` na `services.rs`, `firewall.rs` i `triage.rs`. Autorytatywna obsługa zmiennych `SYSPILOT_BIN_*` w `updater.rs`. Pobieranie telemetrii `checks.mirrors` z `summary.json`. Dodanie 5 hermetycznych testów jednostkowych (clock-skew, atomic write contention, property fuzzing, mirrors telemetry, services differentiation).
- **Docelowe Komponenty:**
  - `crates/syspilot-core/src/models.rs` (`CLOCK_SKEW_TOLERANCE_SECS`, `read_system_snapshot_or_fallback`, `FailedServicesStatus.user_manager_available`, `SysHealthMirrors`)
  - `crates/syspilot-core/src/triage.rs` (`ATOMIC_WRITE_SEQ`, directory fsync, `checks.mirrors` ingestion, `kill_on_drop(true)`)
  - `crates/syspilot-core/src/services.rs` (`query_failed_units`, `user_manager_available`, `kill_on_drop(true)`)
  - `crates/syspilot-core/src/firewall.rs` (`kill_on_drop(true)`)
  - `crates/syspilot-core/src/updater.rs` (`find_executable` authoritative env handling)
  - `crates/syspilot-core/src/tests.rs` (`test_clock_skew_resilience`, `test_concurrent_atomic_status_writes`, `test_json_fuzz_property_resilience`, `test_sys_health_ingests_mirrors_telemetry`, `test_services_failed_units_differentiation`)
  - `CHANGELOG.md`
  - `AUDIT_MATRIX.md`
  - `SRE_AUDIT_PLAN.md`
- **Kontekst i Uzasadnienie:**
  Drobne przesunięcia zegara systemowego przez NTP (np. cofnięcie o 1–60 s) powodowały błąd w obliczaniu wieku snapshotu (`duration_since(mtime)`), co prowadziło do fałszywego oznaczania danych jako przeterminowane (`is_stale() == true`). Ponadto, wielowątkowy zapis do wspólnego pliku `.status.json.tmp.{pid}` mógł prowadzić do kolizji i utraty integralności pliku w przypadku awarii zasilania bez fsync katalogu. Nowy moduł zapewnia pełną spójność ACID na poziomie systemu plików oraz absolutną odporność parsera JSON.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 47/47 unit tests PASS (core: 34, tray: 12, gui: 1), Clippy clean, fmt clean.

---

### PATCH-024: [IMPLEMENTED] Sprint 4 100% Completion: 24/7 Tray Sentinel & D-Bus Hardening
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-SPRINT-04-001`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-01
- **Źródło:** SysPilot SRE Audit Plan (`SRE_AUDIT_PLAN.md` Sprint 4 Completion)
- **Audyt SRE:**
  - `BP-ARGON-SPRINT-04-001` (Auditor Argon Pro): Odporność demona zasobnika na restarty sesyjnego D-Bus, zapobieganie wyciekom deskryptorów inotify, deduplikacja zadań triage w tle oraz nieblokujący shutdown. Wprowadzenie `GameModeWatcher` z buforowanym proxy i leniwą re-koneksją (500ms timeout) przy `is_closed()` lub błędzie I/O. Jawne wyrejestrowanie watcha (`unwatch`) przy `FsSignal::Fault` oraz asercja `parent.is_dir()`. Wdrożenie blokady atomowej `triage_in_flight: Arc<AtomicBool>` z RAII `TriageGuard`, eliminującej sztormy procesów i kolizje na `/tmp/checkup-db-<uid>/db.lck`. Przeniesienie wywołań triage z wątku głównego do nieblokującego `tokio::spawn`. Obudowanie `handle.shutdown()` 2-sekundowym timeoutem zapobiegającym zawieszaniu procesu podczas wylogowywania ze środowiska Plasma Wayland. Dodanie telemetrii błędów we wszystkich akcjach menu. Dodanie 4 testów jednostkowych headless (renderowanie błędów w menu, stany krytyczne sentineli, flaga atomowa deduplikacji, GameMode bez D-Bus).
- **Docelowe Komponenty:**
  - `crates/syspilot-tray/src/gamemode.rs` (`GameModeWatcher`, persistent proxy, lazy reconnect)
  - `crates/syspilot-tray/src/main.rs` (`TriageGuard`, `try_acquire_triage_lock`, `try_spawn_triage`, `try_spawn_triage_and_updater`, inotify leak prevention, 2s shutdown timeout, observability logging)
  - `crates/syspilot-tray/src/tray.rs` (error handling in `action_gui`, `action`, `activate`, `Exit SysPilot`)
  - `crates/syspilot-tray/src/tests.rs` (`test_tray_menu_telemetry_error_rendering`, `test_tray_menu_critical_sentinel_states`, `test_triage_deduplication_atomic_flag`, `test_gamemode_resilience_without_dbus`)
  - `CHANGELOG.md`
  - `AUDIT_MATRIX.md`
  - `SRE_AUDIT_PLAN.md`
- **Kontekst i Uzasadnienie:**
  Jednorazowe pozyskanie sesji D-Bus na starcie powodowało, że restart demona D-Bus lub chwilowe odcięcie socketu trwale wyłączało integrację z GameMode. Ponadto, brak wywołań `unwatch` przy błędach watchera prowadził do powolnego wycieku deskryptorów inotify w jądrze przy wielotygodniowym uptime. Równoległe tyknięcia timerów mogły uruchomić wiele procesów triage naraz, powodując kolizje blokad pacmana. Nowa architektura gwarantuje całkowitą autonomię demona 24/7.
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 51/51 unit tests PASS (core: 34, tray: 16, gui: 1), Clippy clean, fmt clean.

---

### PATCH-025: [IMPLEMENTED] Star Trek Okuda 1987 LCARS Cockpit Dual-Skin Architecture, Astronomical Stardate Engine, Dynamic Window Geometry & SRE Universalism Hardening
- **Status:** `[IMPLEMENTED]` *(SRE Certified — Audit `BP-ARGON-SYSPILOT-UNIVERSALISM-MAINTAIN-H630`, Auditor Argon Pro)*
- **Data zgłoszenia:** 2026-11-07
- **Źródło:** SysPilot Aesthetic & SRE Universalism Evolution (User Karol Specification & SRE Universalism Gate)
- **Audyt SRE:**
  - `BP-ARGON-SYSPILOT-UNIVERSALISM-MAINTAIN-H630` (Auditor Argon Pro): Wdrożenie dwuskórkowej architektury UI (Classic Dark Cyberpunk oraz Star Trek Okuda 1987 LCARS) w natywnym Slint 1.18. Certyfikacja matematycznego silnika Stardate ($STARDATE = YEAR + \frac{\text{day of year}}{\text{days in year}} + \frac{\text{seconds since midnight}}{86400.0}$) przeliczanego asynchronicznie na pętli zdarzeń Slint co 10s. Usunięcie pustej przestrzeni pod kartami na ekranie Dashboard i Maintain poprzez wprowadzenie proporcjonalnej geometrii okna $H(N) = \text{clamp}(180.0 + N \times 56.0, 590.0, 1200.0)$ ($590\text{px}$ Dashboard, $630\text{px}$ Maintain dla $N=8$, $780\text{px}$ Copilot). Wdrożenie standardu SRE Universalism: dynamiczna detekcja dystrybucji z `/etc/os-release` via `detect_os()`, bezwzględna eliminacja lokalnych założeń sprzętowych poprzez zero-fork detektor GPU w sysfs/procfs (`detect_gpu()`) dla NVIDIA, AMD, Intel i wirtualnych adapterów DRI, neutralne opisy wolumenów dyskowych oraz zorientowany na perimeter status zapory sieciowej. Dynamiczne przełączanie skórek `HOLODECK ⇄` z utrwaleniem w `~/.config/syspilot/skin` oraz zapamiętywaniem aktywnej zakładki w `~/.config/syspilot/last_tab`. Wyrównanie akapitów wierszy serwisowych do osi $x = 138\text{px}$.
- **Docelowe Komponenty:**
  - `crates/syspilot-gui/ui/appwindow.slint` (dual-skin layout, dynamic stardate header, proportional window height, Holodeck button)
  - `crates/syspilot-gui/ui/components/lcars_*.slint` (elbows, pod headers, action buttons, tab buttons, telemetry rows, maintenance rows)
  - `crates/syspilot-gui/ui/fonts/Antonio.ttf` (embedded condensed LCARS typeface)
  - `crates/syspilot-gui/src/main.rs` (stardate computation timer, dynamic window sizing, OS/kernel formatting, refresh tactile pacing, unit tests)
  - `crates/syspilot-core/src/skin.rs` (atomic skin & last_tab persistence in ~/.config/syspilot)
  - `crates/syspilot-core/src/models.rs` (zero-fork `detect_gpu()`, universal telemetry models, version 2.1.0)
  - `crates/syspilot-core/src/tests.rs` (unit tests for GPU detection, skin persistence, tab persistence)
  - `bin/syspilot` (version bump to 2.1.0)
  - `packaging/PKGBUILD` (pkgver=2.1.0)
  - `Cargo.toml` / `Cargo.lock` (version bump to 2.1.0)
  - `README.md`
  - `CHANGELOG.md`
- **Kontekst i Uzasadnienie:**
  Wersja 2.1.0 realizuje postulat podwójnej tożsamości wizualnej (modern cyberpunk vs kultowy interfejs LCARS z 1987 r. autorstwa Michaela Okudy) z zachowaniem pełnej ergonomii, zerowego narzutu na pamięć RAM i zgodności ze standardem SRE Universalism (brak zahardkodowanych parametrów stacji roboczej developera w kodzie publicznym).
- **Weryfikacja:** `dev-tools/verify-rust.sh -q` ➔ 65/65 unit tests PASS (core: 40, tray: 16, gui: 9), Clippy clean, fmt clean. `dev-tools/test-cli.sh` ➔ 13/13 tests PASS.

---

### PATCH-026: [IMPLEMENTED] High-Assurance SRE Tooling Suite, Arch Trust Triad (Zero-Root Guard), Universal Reboot & Kernel/DKMS Module Guard (`reboot_safety.rs`), and Zero-Subprocess Gaming Readiness Probe (`gaming.rs`)
* **Status:** `[IMPLEMENTED]` *(Cross-Model Architectural Review: `408c0e1f`, `0246c885` [Argon], Claude Opus 5.5 + Live Recon `de8cda85` | Executed by `eos-executor` sessions `9613b632`, `f9cee70a`, `5ff8404b`, `86362026`, `547056ef`)*
* **Data zgłoszenia / wdrożenia:** 2026-11-07 / 2026-11-08
* **Źródło:** Strategic Community Pivot & Forum Incident Triage (EndeavourOS Post #82290 — User Trust, Low-End Hardware Optimization & Gaming FPS Health)
* **Wdrożone komponenty:**
  1. **Arch Trust Triad (Zero-Root Invariant):** Blokada uruchomieniowa `libc::getuid() == 0` w `syspilot-tray` i `syspilot-gui` weryfikowana statycznie przez `dev-tools/lint-universalism.sh`.
  2. **Potrójna Blokada Subprocesów (`clippy.toml` + `exec.rs` + `lint-universalism.sh`):** Zakaz wywoływania `Command::new` poza centralnym modułem `crates/syspilot-core/src/exec.rs` z atomowym licznikiem `SUBPROCESS_SPAWN_COUNT` (`exec::spawn_count() == 0` w testach sond). Zastąpienie `Command::new("find")` w `triage.rs` czystą funkcją `find_pacnew_files_at(root)` odporną na katalogi `0700` w `/etc` oraz pętle symlinków.
  3. **Poligon Uniwersalizmu (`sys_root.rs`, `mock_rig.rs`, `dump-fixture.sh`):** Funkcja `sub_path(root, abs_path)` chroniąca przed pułapką `Path::join`, `MockRig` z 5 archetypami sprzętowymi/dystrybucyjnymi (Arch+AMD RDNA3, EOS+NVIDIA Maxwell DKMS, CachyOS+Intel Hybrid Laptop, Broken Reboot State, Headless VM) oraz `MockRig::from_host_fixture` materializujący rzeczywisty zrzut `host_endeavouros_maxwell.json`.
  4. **Benchmark Zasobów (`dev-tools/bench-footprint.sh`):** Weryfikacja `/proc/$pid/smaps_rollup` (`Private_Dirty <= 1536 kB`, `Pss <= 4500 kB`), liczby wątków (`<= 8`) i deskryptorów plików (`<= 22`) z izolacją `XDG_STATE_HOME` i pomiarem właściwego PID potomnego pod `dbus-run-session`.
  5. **Killer Feature #1 — Strażnik Bezpiecznego Rebootu (`crates/syspilot-core/src/reboot_safety.rs`):** Bezprocesowa weryfikacja `/usr/lib/modules/<kver>/` (`modules.dep`, `modules.builtin`, fizyczne pliki `.ko`/`.ko.zst` z weryfikacją nagłówka magic `zstd` `[0x28, 0xb5, 0x2f, 0xfd]`, odporność na martwe katalogi po odinstalowanych kernelach) oraz obrazów `/boot/initramfs-*.img` pod kątem brakujących modułów aktywnego sterownika GPU PCI `0x03` (`black_screen_risk` podnoszący status do `ACTION_REQUIRED`).
  6. **Killer Feature #2 — Zero-Subprocess Gaming & FPS Readiness Probe (`crates/syspilot-core/src/gaming.rs`):** Bezprocesowa weryfikacja 64-bit i 32-bit Vulkan ICD oraz bibliotek `/usr/lib` i `/usr/lib32` dopasowanych do aktywnego sterownika PCI (`icd_matches_active_drivers`, obsługa współdzielonego `nvidia_icd.json` na Archu oraz osobnych `.i686.json` dla Mesa/RADV/ANV), odczyt `scaling_governor`, statusu hybrydowego dGPU oraz pracy na baterii.
* **Weryfikacja (`PROOF-EXEC-001`):** `dev-tools/verify-rust.sh -q` ➔ `RUST_VERIFY: PASS | TESTS: core: 46 | tray: 16 | gui: 9 | Total: 71/71 | CLIPPY: CLEAN | FMT: CLEAN | LINT: CLEAN | TARGET: syspilot-workspace` (`exec::spawn_count() == 0`, `Private_Dirty: 680 kB`, `Pss: 3534 kB`).

---

### PATCH-027: [DRAFT / RFC] Tab 03 Strategic Pivot — Deterministic SRE Tactical Playbooks & Passive AI Telemetry Export
* **Status:** `[DRAFT / RFC]` *(Approved Design — Scheduled for Next Session)*
* **Zakres:** Wycofanie czatbota z Tab 03 w GUI (`crates/syspilot-gui/ui/appwindow.slint`) i zastąpienie go deterministycznymi, modułowymi playbookami SRE (`03 SRE TACTICAL PLAYBOOKS`: Wine/Proton shader & prefix hygiene, PipeWire reset, `pacdiff` reviewer, `paccache` prune), dynamicznym loaderem skryptów użytkownika z `~/.config/syspilot/playbooks.d/` oraz pasywnym przyciskiem `[ 📋 Copy SRE Telemetry for AI ]`.
* **Status Weryfikacji:** `NOT YET IMPLEMENTED`.

---

## Clerical Inbox

This bounded section records untriaged observations from active testing. Entries here
are not approved patches and do not authorize source changes.

<!-- CLERICAL-INBOX:BEGIN -->
<!-- Entries are appended immediately above CLERICAL-INBOX:END by project_inbox.py. -->

- [x] [ISSUE-20261018T140524916820-A227] [2026-10-18] Brak wyboru agenta przy Ask AI copilot
  > RESOLVED in PATCH-010 via ~/.config/syspilot/config.env (SYSPILOT_COPILOT_CMD / COPILOT_LABEL).
  <!-- observation-sha256:3f073ee998c8c12510762dea19b5cee3e1c824a6692764ae715e9ac009106def -->


- [ ] [ISSUE-20261018T154614949255-DDE9] [2026-10-18] Brak statusu mirrorlist na dashboardzie i ostrzeżenia o primary DEAD w Guarded System Update
  > Dashboard sysPilot nie pokazuje statusu Mirrorlist, podczas gdy sys-health raportuje ostrzeżenie 'WAN ⚠ (Arch: 4d, primary DEAD, 5 srv │ Distro: 4d, 28)'. Uruchomienie opcji 'Run Guarded System Update' w sysPilot nie wyświetla informacji ani ostrzeżenia o martwym mirrorze głównym ('primary DEAD').
  > Expected: Prezentacja stanu i ostrzeżeń mirrorlist na Dashboardzie sysPilot oraz uwzględnienie ostrzeżenia o stanie 'primary DEAD' przy uruchamianiu Guarded System Update.
  > SRE Triage / Arch: Powiązano architektonicznie z `sys-health.sh` PATCH-044 (Pakiet 1). Silnik wyeksportuje ustrukturyzowane pola `arch_primary_alive` i `arch_age_days` do `summary.json`, a SysPilot skonsumuje je bezpośrednio w `core/triage.py` bez duplikowania zapytań sieciowych.
  <!-- observation-sha256:b24329c166fe8a25a32762a8285eebce22bd2891b9f987b751696e2a6f7cf8ce -->


- [x] [ISSUE-20261019T172602824270-4222] [2026-10-19] Ikona tarczy w systray pozostaje zielona \(All Clear\) pomimo wykrycia oczekujacych aktualizacji
  > RESOLVED in PATCH-009 via updater.rs (checkupdates) and effective_status harmonization.
  <!-- observation-sha256:19e474dd9e85b100e232344a9a1daa2881a1a5eceffa954fd2032a53d4c04d47 -->


- [x] [ISSUE-20261028T102401996549-A984] [2026-10-28] Brak karty i statusu GPU/Hardware na dashboardzie SysPilot przy ostrzeżeniach w trayu
  > RESOLVED in PATCH-010 via Hardware Platform & GPU row in GUI.
  <!-- observation-sha256:7e9e5d6a5f7013b8f5a4b8ba3cc3f577cbb53d6db2dafa6a674dc047528a97e2 -->

<!-- CLERICAL-INBOX:END -->

## Rejected Decisions Index

<!-- REJECTED-DECISIONS:BEGIN -->
- None recorded as of 2026-10-18.
<!-- REJECTED-DECISIONS:END -->
