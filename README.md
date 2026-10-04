# ✈️ SysPilot

> **The Token-Lean SRE Desktop Copilot for Arch Linux & Derivatives**  
> *Deterministic local diagnostics • Zero token waste • Automated gaming & system rescue.*

---

## 🌟 Overview

**SysPilot** bridges the gap between passive Linux command-line diagnostics and everyday users. Instead of waiting for the system to break or manually running scripts in the terminal, **SysPilot** runs quietly in the system tray, monitors overall flight readiness, automatically inhibits itself during gaming sessions, and embeds an autonomous **AI Copilot** powered by [Goose](https://github.com/block/goose) and **Google Gemini Flash**.

Unlike generic AI assistants that hallucinate or blindly execute destructive commands, SysPilot pairs the execution capabilities of the **Goose CLI** with the specialized **`sys-pilot-admin` SRE skill** (built upon the battle-tested `eos-admin` Linux engineering architecture).

---

## 💡 The Token-Lean Philosophy

Most AI agents on Linux waste tens of thousands of tokens by indiscriminately dumping raw system logs (`dmesg`, `journalctl`), asking random questions, and trying out obsolete Ubuntu fixes.

**SysPilot flips this model on its head:**

```
                  ┌───────────────────────────────────────────────┐
                  │          STEP 0: Local Triage Engine          │
                  │     (0 tokens / 0 cost / 100% Determinism)    │
                  │  Discovers updates, failed units, disk, pacnew│
                  └───────────────────────┬───────────────────────┘
                                          │
                                          ▼
                  ┌───────────────────────────────────────────────┐
                  │       STEP 1: Embedded SRE Playbooks          │
                  │   (0 tokens / Instant Battle-Tested Knowledge)│
                  │   FAF runner, Proton GE, PipeWire, .pacnew    │
                  └───────────────────────┬───────────────────────┘
                                          │
                      Only when complex reasoning is required
                                          ▼
                  ┌───────────────────────────────────────────────┐
                  │     STEP 2: Goose AI Copilot (Gemini Flash)   │
                  │    (Surgical context: ~400 - 900 tokens)      │
                  │    Model receives pre-filtered root causes    │
                  │       Cost: $0.0001 | Latency: ~1.5s          │
                  └───────────────────────────────────────────────┘
```

By having the local bash/Python engine perform 100% of the heavy lifting for free, the AI Copilot receives only the exact surgical slice of telemetry needed to resolve the fault.

---

## 🛡️ Core Features

- **Zero-Sudo Boot Triage (<1s execution):**
  - Evaluates pending repository updates (`checkupdates`) and AUR packages (`yay`/`paru`) without root privileges and without locking `/var/lib/pacman/db.lck`.
  - Distinguishes **Core System Updates** (kernel, systemd, glibc, dracut, grub, mesa, nvidia) from regular packages.
  - Queries active and failed `systemd` units (system and user sessions).
  - Detects `.pacnew` configuration conflicts and un-rebooted kernel upgrades.
- **Standalone & Third-Party Apps Update Triage:**
  - Audits standalone and user-level applications: AUR packages, Flatpak runtimes, UV/Python, Goose CLI, Pipx, and Steam.
  - **Automated Goose Updates:** Monitors the Goose AI binary; whenever an upstream update is detected, a dynamic **`[⚡ Update Goose AI Agent]`** button appears directly on the dashboard.
- **Dedicated System Maintenance Suite:**
  - 1) **Orphan Package Triage & Prune:** Interactive 3-tier orphan purger that preserves optional dependencies (`optdepends`).
  - 2) **Refresh & Rank Regional Mirrors:** Benchmarks and ranks fastest regional mirrors with an atomic fallback safety gate.
  - 3) **Clean (Safe Maintenance):** Safe pacman package cache trimming (retains latest 2), journal vacuuming, and temporary run cleanup.
  - 4) **Deep Clean:** Purges user Trash, browser caches, thumbnail caches, and legacy diagnostic runs.
- **Transparent SRE Skill & Persona Architecture:**
  - Dedicated **SRE Skill & Persona** tab explaining the dualism between Goose (the terminal runner) and `sys-pilot-admin` (the SRE brain).
  - **Editable User Custom Directives:** Users can persist custom hardware quirks, audio setups, or package preferences that the Copilot strictly follows in every session.
  - **Playbook Arsenal Viewer:** Inspect the exact step-by-step engineering procedures the AI Copilot follows before executing fixes.
- **Layman-Friendly AI Copilot Onboarding (30 Seconds Setup):**
  - Zero terminal hassle: on first run, a clean graphical setup wizard guides users to connect their preferred AI engine.
  - Recommends **Google Gemini Flash** (ultra-fast, generous free tier on Google AI Studio, no credit card required).
  - Seamlessly configures Goose CLI profiles, API key secrets (`0600` permissions), and registers the `sys-pilot-admin` SRE skill automatically.
- **Smart GameMode Inhibit (Zero FPS Stutter):**
  - Queries Feral GameMode (`gamemoded` / D-Bus) and active game processes.
  - Automatically suspends all background scans and desktop notifications during gaming sessions.
- **Modern PyQt6 System Tray & Dashboard:**
  - Status Indicators:
    - 🟢 **Flight Ready:** System clean, stable, primed.
    - 🟡 **Pre-Flight Attention:** Core updates or `.pacnew` files pending.
    - 🔴 **Action Required:** Failed services or critically low disk space.
    - 🎮 **Gaming Mode:** Diagnostics paused.
  - One-click triggers for **Guarded System Upgrades**, **Safe Maintenance**, and **Full Health Audits**.

---

## 🖥️ Graphical Dashboard Layout

The SysPilot graphical interface is organized into 5 clean, focused tabs:

1. **✈ Dashboard:** Lean operational overview. Shows Official Updates, Standalone & 3rd-Party App Updates (with 1-click Goose updater), Root Disk Usage, and GameMode status.
2. **🛠 System Maintenance:** One-click safe maintenance tools (Orphan package triage, Mirror ranking, Safe Cache Trimming, Deep Clean, and `.pacnew` reconciler).
3. **🤖 AI Copilot (Goose SRE):** Embedded AI troubleshooting console with 1-click playbook buttons (FAF Setup, Proton launch fix, PipeWire audio repair, `.pacnew` diffing) and the 30-second onboarding wizard.
4. **🧠 SRE Skill & Persona:** Full transparency view. Inspect pre-gathered 0-token telemetry, customize persistent user directives for your rig, and browse loaded playbooks.
5. **⚙ Settings & Autostart:** Configure autostart on user login and select default AI reasoning models.

---

## 🚀 Quick Start & CLI Reference

### 1. Launching SysPilot
```bash
# Launch the Graphical Dashboard & Tray Icon
syspilot

# Launch minimized to system tray (used for autostart)
syspilot --tray

# Print instant terminal flight readiness scorecard (0 tokens)
syspilot -s

# Output machine-readable JSON triage
syspilot -j
```

### 2. Maintenance Operations via CLI
```bash
# Run Guarded System Upgrade (pre-flight checks -> upgrade -> post-audit)
syspilot -u

# Audit and update standalone applications (AUR, Flatpak, UV, Goose)
syspilot --software

# Interactive 3-tier orphan package resolver
syspilot -o

# Benchmark and rank fastest regional mirrors
syspilot --mirrors

# Run safe maintenance (pacman cache trim & journal vacuum)
syspilot -m

# Run deep clean (trash, thumbnails, browser caches)
syspilot -d

# Check gaming & Steam/Proton readiness
syspilot -g
```

### 3. Asking the AI Copilot from CLI
```bash
# Ask Copilot to diagnose a specific issue
syspilot --copilot "Why is my sound crackling on headphones?"

# Launch an interactive SRE Copilot terminal session
syspilot --copilot-shell

# Inspect active SRE skill directives, guardrails, and loaded playbooks
syspilot --skill
```

### 4. Enabling Autostart on Login
```bash
syspilot --enable-autostart
```
*(Or toggle the autostart checkbox in the SysPilot Settings tab).*

---

## 📦 Directory Structure

```text
sysPilot/
├── bin/
│   ├── syspilot                 # Unified master CLI & GUI runner
│   ├── syspilot-sentinel        # Background monitoring daemon
│   └── sys-health.sh            # Hardened SRE health & diagnostic engine
├── core/
│   ├── triage.py                # High-speed (<1s) zero-sudo telemetry collector
│   └── copilot_config.py        # Goose configuration & skill management engine
├── gui/
│   └── syspilot_gui.py          # Modern PyQt6 Tray & Dashboard application
├── copilot/
│   ├── recipe.yaml              # Goose SRE recipe definition
│   ├── skills/
│   │   └── sys-pilot-admin/     # Token-Lean Copilot system instructions (SKILL.md)
│   ├── playbooks/               # Battle-tested recipes (FAF, Proton, Audio, Pacnew)
│   └── quirks/                  # Hardware & driver quirks
├── autostart/
│   └── syspilot.desktop         # XDG autostart entry
├── packaging/
│   └── PKGBUILD                 # Arch Linux / AUR package specification
└── systemd/
    ├── syspilot-sentinel.service# Optional systemd user service
    └── syspilot-sentinel.timer  # Periodic 3-hour evaluation timer
```

---

## 📜 Case Study: The FAF (Forged Alliance Forever) Miracle

Setting up Forged Alliance Forever on Arch Linux is notoriously difficult: the AUR package only ships the Java frontend, requiring manual orchestration of Steam Linux Runtime containers (`pressure-vessel`), custom Proton GE wine prefixes, 32-bit Vulkan drivers, and cross-copying configuration files from Windows directories.

With **SysPilot Copilot**, a user simply clicks **„Setup / Repair FAF Client”** or types *„Setup FAF”*. The Copilot:
1. Verifies multilib and 32-bit NVIDIA/AMD driver stacks locally via `syspilot` telemetry.
2. Deploys the isolated `faf-linux` runner inside `~/faf-linux`.
3. Automatically syncs `Game.prefs` from the Steam Proton compatdata directory into the FAF prefix.
4. Completes in seconds, saving hours of manual debugging and ensuring game night with family is never cancelled.

---

## 📄 License & Community Independence

SysPilot is open-source software licensed under the **Apache License 2.0**.
*SysPilot is an independent community project and is not officially affiliated with EndeavourOS, Arch Linux, or Block Inc. (Goose).*
