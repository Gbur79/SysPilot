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
