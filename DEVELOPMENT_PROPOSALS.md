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
* **Status:** `[PROPOSED]` *(Successfully validated on local testing node)*
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
- [ ] Apply changes to `core/copilot_config.py`.
- [ ] Apply changes to `gui/syspilot_gui.py`.
- [ ] Apply changes to `copilot/recipe.yaml`.
- [ ] Verify syntax compilation with `python3 -m py_compile`.
- [ ] Validate CLI output with `syspilot --skill`.
- [ ] Push to public GitHub repository (`origin/main`).
