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


