# ✈️ SysPilot

> **The Token-Lean SRE Desktop Copilot & Sentinel for Arch Linux & Derivatives**  
> *Ultra-lightweight 24/7 Rust Sentinel (~1.2 MB RAM) • Decoupled on-demand GUI • Zero token waste • Native D-Bus.*

---

<img width="1160" height="962" alt="image" src="https://github.com/user-attachments/assets/a0e65fa0-afdf-4382-9878-9ef52fd3e310" />


## 🌟 Overview

**SysPilot** bridges the gap between passive Linux command-line diagnostics and everyday desktop operations. Instead of waiting for the system to break or running heavy scripts, **SysPilot** employs a decoupled two-tier architecture:

1. **Ultra-Lightweight 24/7 Rust Sentinel (`syspilot-tray`):** Runs continuously in the background consuming only **~1.2 MB RAM and 0.0% CPU**. Communicates directly with the system and user session bus via native D-Bus (`zbus 5.19`, zero `busctl` fork/exec), tracks pending kernel reboots, pacman database locks, disk capacity, and queries pending updates with automatic GameMode freeze.
2. **On-Demand Graphical Dashboard (`syspilot --gui`):** Rich PyQt6 desktop dashboard with single-instance enforcement (`fcntl.flock` + `QLocalServer`), dynamic Hardware Platform & GPU telemetry, 1-click guarded system upgrades, orphan cleanup, mirror ranking, and an interactive **AI Copilot** console.

Unlike generic AI assistants that hallucinate or blindly execute destructive shell commands, SysPilot pairs an execution runner with the specialized **`copilot-admin` SRE skill** (built upon battle-tested token-lean Linux systems engineering).

---

## 💡 The Token-Lean & Decoupled Philosophy

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
                  │     STEP 2: SysPilot AI Copilot               │
                  │    (Surgical context: ~400 - 900 tokens)      │
                  │    Model receives pre-filtered root causes    │
                  │       Cost: $0.0001 | Latency: ~1.5s          │
                  └───────────────────────────────────────────────┘
```

By having the local Rust sentinel and Python triage engine perform 100% of the heavy lifting for free, the AI Copilot receives only the exact surgical slice of telemetry needed to resolve the fault.

---

## 🛡️ Core Features

- **Ultralight 24/7 Rust Sentinel (`rust-tray`):**
  - Native StatusNotifierItem (SNI) implementation (`ksni 0.3`) for KDE Plasma Wayland and X11 desktops.
  - Native D-Bus client (`zbus 5.19`) for zero-fork GameMode queries.
  - Immediate detection of un-rebooted kernel upgrades by monitoring `/usr/lib/modules/<version>`.
  - Non-blocking disk space queries using `libc::statvfs` directly without invoking external shell binaries.
  - Intelligent pacman lock monitor: distinguishes active package manager operations from orphaned `/var/lib/pacman/db.lck` files.
- **Asynchronous Package Update Intelligence:**
  - Evaluates pending official repository updates (`checkupdates` isolated in `/tmp`) and AUR packages (`yay`/`paru`).
  - Distinguishes **Core System Updates** (kernel, systemd, glibc, dracut, grub, mesa, nvidia) from regular packages.
  - Auto-heals stale lockfiles in temporary checkupdate databases.
  - Suspends all background update and disk checks during active gaming sessions (GameMode Freeze).
  - **On-Demand Package Pre-Caching (`[⚡ PRE-CACHE PACKAGES]`):** Interactively pre-downloads multi-gigabyte package archives into `/var/cache/pacman/pkg` prior to initiating the full upgrade, minimizing system lock times.
- **Security & Vulnerability Intelligence (`arch-audit`):**
  - Instant, non-blocking CVE tracking parsing official Arch Linux Security Tracker data (`security.archlinux.org`).
  - Distinguishes benign utility updates from High/Critical remote code execution or privilege escalation zero-days.
  - Automatically escalates Rust Tray Sentinel status and tooltip when actionable security advisories are detected, paired with an interactive `[🔍 INSPECT CVE]` terminal audit.
- **Hardware Firmware & Peripherals Telemetry (`fwupd` / LVFS):**
  - Full visibility into firmware status for NVMe/SATA SSDs, wireless dongles (Logitech Unifying), TPMs, and motherboard UEFI.
  - Interactive, guarded `[⚡ UPGRADE FIRMWARE]` action running LVFS updates in an isolated terminal.
  - **Universal NVRAM Resilience:** Smart hardware-quirk classification that bypasses oversized UEFI certificates (KEK/dbx) on platforms with legacy BIOS NVRAM buffer limits when Secure Boot is disabled, eliminating false-positive warning loops.
- **Endpoint Host Firewall Inspection (Shield):**
  - High-speed (<5ms) multi-backend inspector verifying `firewalld`, `ufw`, or `nftables` state and active zones.
  - Follows the SRE Universalism Standard: machines relying on upstream perimeter routers (e.g. pfSense) report neutral `INFO ℹ` without false alerts.
- **Decoupled Single-Instance Dashboard (`syspilot_gui.py`):**
  - Atomic startup lock (`fcntl.flock`) and early `QLocalServer` binding eliminate duplicate windows upon rapid double-clicks.
  - Dynamic **Hardware Platform & GPU** telemetry row detecting CPU architecture, GPU model, and active driver stack without hardcoded assumptions.
  - Responsive status synchronization: zero lag between tray amber warnings and dashboard components.
- **Universal & Configurable AI Copilot:**
  - Seamlessly configured via `~/.config/syspilot/config.env` (`SYSPILOT_COPILOT_CMD`, `SYSPILOT_GOOSE_RECIPE`, `COPILOT_LABEL`).
  - Supports private agent profiles or native terminal runners without modifying public codebase.
  - 30-second layman graphical onboarding wizard for Google Gemini Flash, OpenAI, Claude, or local Ollama.
- **Dedicated System Maintenance Suite:**
  - 1) **Orphan Package Triage & Prune:** Interactive 3-tier orphan purger preserving optional dependencies (`optdepends`).
  - 2) **Refresh & Rank Regional Mirrors:** Benchmarks and ranks fastest regional mirrors with atomic fallback safety.
  - 3) **Safe Maintenance:** Trims pacman cache (retains last 2), vacuums journal logs, and cleans temporary artifacts.
  - 4) **Deep Clean:** Purges user Trash, browser caches, thumbnail caches, and legacy diagnostic runs.
- **Deterministic 1-Click FAF (Forged Alliance Forever) Setup (`syspilot --faf-repair`):**
  - Verifies 32-bit Vulkan ICDs, syncs `Game.prefs` across Steam libraries, and generates dynamic launcher wrappers.

---

## 🖥️ Graphical Dashboard Layout

The SysPilot graphical interface is organized into 5 focused tabs:

1. **✈ Dashboard:** Lean operational overview. Shows Official Updates (with Security Advisories & CVE risk scoring, Pre-caching, Guarded Upgrades), Standalone & 3rd-Party App Updates (AUR, Flatpak, UV, Goose, Hardware Firmware via `fwupd`), and System Health & Hygiene (Host Firewall Shield, Reboot, Systemd units, Baloo & Journal, Orphans, .pacnew, Disk storage, Gaming readiness, and Hardware Platform & GPU).
2. **🛠 System Maintenance:** One-click safe maintenance tools (Orphan package triage, Mirror ranking, Safe Cache Trimming, Deep Clean, and `.pacnew` reconciler).
3. **🤖 AI Copilot:** Embedded AI troubleshooting console with 1-click playbook buttons (FAF Setup, Proton launch fix, PipeWire audio repair, `.pacnew` diffing) and the 30-second onboarding wizard.
4. **🧠 SRE Skill & Persona:** Full transparency view. Inspect pre-gathered 0-token telemetry, customize persistent user directives, and browse loaded playbooks.
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

# Setup or repair Forged Alliance Forever (FAF) & fix desktop shortcuts
syspilot --faf-repair
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
│   ├── syspilot-faf-repair      # Deterministic FAF runner & shortcut repair tool
│   └── syspilot-sentinel        # CLI background monitoring daemon
├── rust-tray/                   # High-performance 24/7 Rust StatusNotifierItem Tray Sentinel
│   ├── Cargo.toml               # Native zbus 5.19 & ksni 0.3 dependencies
│   ├── ARCHITECTURE.md          # Full architectural breakdown & SRE contracts
│   └── src/
│       ├── main.rs              # Event loop & timers
│       ├── sentinels.rs         # Reboot, pacman lock, libc statvfs probes
│       ├── updater.rs           # Async checkupdates, AUR, and software updates
│       ├── gamemode.rs          # Native zbus D-Bus GameMode proxy
│       ├── tray.rs              # Ergonomic 5-action SNI menu & multi-line tooltip
│       └── tests.rs             # Hermetic unit tests (cargo test --locked)
├── core/
│   ├── triage.py                # High-speed (<1s) zero-sudo telemetry collector
│   └── copilot_config.py        # Copilot configuration & ~/.config/syspilot/config.env resolver
├── gui/
│   └── syspilot_gui.py          # Decoupled PyQt6 Dashboard (single-instance flock)
├── copilot/
│   ├── recipe.yaml              # Public AI SRE recipe definition
│   ├── skills/
│   │   └── sys-pilot-admin/     # Token-Lean Copilot system instructions (SKILL.md)
│   ├── playbooks/               # Battle-tested recipes (FAF, Proton, Audio, Pacnew)
│   └── quirks/                  # Generic hardware & architecture quirks
├── dev-tools/
│   ├── build-and-install.sh     # One-step test, compile & install helper
│   └── verify-rust.sh           # SRE hermetic verification script
├── autostart/
│   └── syspilot.desktop         # XDG autostart entry
├── packaging/
│   └── PKGBUILD                 # Arch Linux / AUR package specification (v1.1.0)
└── systemd/
    ├── syspilot-tray.service    # User systemd service for 24/7 Rust Sentinel
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
4. Generates dynamic desktop and application menu shortcuts pointing to the runner wrapper (`~/faf-linux/run`), preventing version-freeze and preserving automatic background updates (`syspilot --faf-repair`).
5. Completes in seconds, saving hours of manual debugging and ensuring game night with family is never cancelled.

---

## 📄 License & Community Independence

SysPilot is open-source software licensed under the **Apache License 2.0**.
*SysPilot is an independent community project and is not officially affiliated with EndeavourOS, Arch Linux, or Block Inc. (Goose).*
