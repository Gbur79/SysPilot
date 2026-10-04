# ✈️ SysPilot

> **The Token-Lean SRE Desktop Copilot for Arch Linux & Derivatives**  
> *Deterministic local diagnostics • Zero token waste • Automated gaming & system rescue.*

---

## 🌟 Overview

**SysPilot** bridges the gap between passive Linux command-line diagnostics and everyday users. Instead of waiting for the system to break or manually running scripts in the terminal, **SysPilot** runs quietly in the system tray, monitors overall flight readiness, automatically inhibits itself during gaming sessions, and embeds an autonomous **AI Copilot** powered by [Goose](https://github.com/block/goose) and **Google Gemini Flash**.

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
- **Dedicated System Maintenance Suite:**
  - 1) **Orphan Package Triage & Prune:** Interactive 3-tier orphan purger that preserves optional dependencies (`optdepends`).
  - 2) **Refresh & Rank Regional Mirrors:** Benchmarks and ranks fastest regional mirrors with an atomic fallback safety gate.
  - 3) **Clean (Safe Maintenance):** Safe pacman package cache trimming (retains latest 2), journal vacuuming, and temporary run cleanup.
  - 4) **Deep Clean:** Purges user Trash, browser caches, thumbnail caches, and legacy diagnostic runs.
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
- **Embedded Autonomous AI Copilot:**
  - Integrated with Goose AI using the specialized `sys-pilot-admin` skill.
  - Pre-packaged playbooks for complex multi-step workflows:
    - ⚡ **Forged Alliance Forever (FAF):** Automated setup of the `faf-linux` runner, Steam Linux Runtime, Proton GE, and `Game.prefs` synchronization.
    - 🎮 **Steam / Proton Readiness:** 32-bit graphics libraries, DXVK, futex2 / fsync, and wine prefix corruption recovery.
    - 🔊 **PipeWire Audio Triage:** Sinks, sample-rate jitter, and WirePlumber state recovery.
    - 🔧 **Surgical .pacnew Reconciler:** Safe diffing and non-destructive merging.

---

## 🚀 Quick Start

### 1. Launching SysPilot
```bash
# Launch the Graphical Dashboard & Tray Icon
./bin/syspilot

# Launch minimized to tray (used for autostart)
./bin/syspilot --tray

# Print instant terminal status (0 tokens, formatted scorecard)
./bin/syspilot --status

# Output machine-readable JSON triage
./bin/syspilot --json
```

### 2. Asking the AI Copilot from CLI
```bash
# Ask Copilot to diagnose a specific issue
./bin/syspilot --copilot "Why is my sound crackling on headphones?"

# Launch an interactive SRE Copilot terminal session
./bin/syspilot --copilot-shell
```

### 3. Enabling Autostart on Login
```bash
./bin/syspilot --enable-autostart
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
│   └── triage.py                # High-speed (<1s) zero-sudo telemetry collector
├── gui/
│   └── syspilot_gui.py          # Modern PyQt6 Tray & Dashboard application
├── copilot/
│   ├── recipe.yaml              # Goose SRE recipe definition
│   ├── skills/
│   │   └── sys-pilot-admin/     # Token-Lean Copilot system instructions
│   ├── playbooks/               # Battle-tested recipes (FAF, Proton, Audio, Pacnew)
│   └── quirks/                  # Hardware & driver quirks
├── autostart/
│   └── syspilot.desktop         # XDG autostart entry
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
