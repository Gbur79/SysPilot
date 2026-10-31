# ✈️ SysPilot

> **The Token-Lean SRE Desktop Copilot & Sentinel for Arch Linux & Derivatives**  
> *Ultra-lightweight 24/7 Rust Sentinel (~3.2 MB RAM) • Decoupled on-demand GUI • Zero token waste • Native D-Bus.*

---

<img width="902" height="751" alt="image" src="https://github.com/user-attachments/assets/c2720ce8-8bef-4d11-83d2-cac23b243281" />

<img width="904" height="783" alt="image" src="https://github.com/user-attachments/assets/1739eb87-c309-481f-9f3a-261410d3d579" />


## 🌟 Overview

**SysPilot** bridges the gap between passive Linux command-line diagnostics and everyday desktop operations. Instead of waiting for the system to break or running heavy scripts, **SysPilot** employs a decoupled two-tier architecture:

1. **Ultra-Lightweight 24/7 Rust Sentinel (`syspilot-tray`):** Runs continuously in the background consuming only **~3.2 MB RAM and 0.0% CPU**. Communicates directly with the system and user session bus via native D-Bus (`zbus 5.19`, zero `busctl` fork/exec), tracks pending kernel reboots, pacman database locks, disk capacity, and queries pending updates with automatic GameMode freeze.
2. **On-Demand Graphical Dashboard (`syspilot --gui`):** Native Slint 1.18 desktop dashboard with kernel-level single-instance enforcement (`libc::flock`), dynamic Hardware Platform & GPU telemetry, 1-click guarded system upgrades, orphan cleanup, mirror ranking, and an interactive **AI Copilot** console.

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

By having the local Rust sentinel and triage engine perform 100% of the heavy lifting for free, the AI Copilot receives only the exact surgical slice of telemetry needed to resolve the fault.

---

## 🛡️ Core Features

- **Ultralight 24/7 Rust Sentinel (`syspilot-tray`):**
  - Native StatusNotifierItem (SNI) implementation (`ksni 0.3`) for KDE Plasma Wayland and X11 desktops (~3.2 MB RSS).
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
- **Native Slint Desktop Cockpit (`syspilot-gui`):**
  - Ultra-fast native GUI built with Slint 1.18 and dual renderers (FemtoVG OpenGL + software CPU rasterizer fallback).
  - Single instance locking via non-blocking `libc::flock`.
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

The SysPilot native Slint GUI is organized into 3 streamlined, high-density tabs:

1. **✈ Dashboard:** Real-time operational cockpit categorized into 3 synchronized telemetry cards:
   - **System Health & Hygiene:** Kernel & modules sync, root storage (`/`), host firewall shield, failed systemd units, and dynamic Hardware Platform & GPU telemetry.
   - **System Updates:** Core system packages (kernel/drivers), official repository updates, AUR packages, and security advisories (CVE tracking).
   - **Third-Party & Peripherals:** Hardware firmware (LVFS / `fwupd`) and orphan package detection, paired with quick action triggers (`RUN GUARDED UPGRADE`, `DIAGNOSTIC AUDIT`, `PRUNE ORPHANS`).
2. **🛠 System Maintenance:** 7 atomic, battle-tested maintenance cards:
   - 1) **Orphan Package Triage & Prune:** Interactive 3-tier orphan purger preserving optional dependencies (`optdepends`).
   - 2) **Refresh & Rank Regional Mirrors:** Benchmarks and ranks fastest regional mirrors with atomic backup.
   - 3) **Safe Maintenance & Cache Trimming:** Trims pacman cache (retains last 2), vacuums journal logs (>14 days), and cleans old run logs.
   - 4) **Deep Clean:** Purges user Trash, browser caches, and thumbnail caches.
   - 5) **Security Advisories (CVE Triage):** Deep CVE inspection powered by `arch-audit`.
   - 6) **Hardware Firmware Upgrade (LVFS):** Upgrades firmware for NVMe, UEFI, and attached peripherals via `fwupdmgr`.
   - 7) **Pre-Cache Packages:** Interactively pre-downloads large packages ahead of maintenance without locking pacman.
3. **🤖 AI Copilot:** Embedded SRE diagnostic console featuring:
   - 1-click battle-tested playbooks: **FAF Client Repair**, **Steam / Proton Fix**, **Reconcile .pacnew**, and **PipeWire Audio Fix**.
   - Pluggable multi-agent CLI adapter: Seamlessly routes queries to Google Antigravity CLI (`agy`), Goose, or custom local runners.
   - Live interactive diagnostic console and terminal launcher.

---

## 🔨 Installation & Building from Source

### Prerequisites
On Arch Linux / EndeavourOS:
```bash
sudo pacman -S --needed base-devel rust cargo fontconfig freetype2
```

### Build & Deploy
```bash
git clone https://github.com/Gbur79/SysPilot.git
cd SysPilot

# Build release binaries with locked dependencies
cargo build --release --locked

# Copy binaries to your local bin directory
cp target/release/syspilot-tray target/release/syspilot-gui ~/.local/bin/

# Alternatively, use the automated installer to compile and restart the service:
./dev-tools/build-and-install.sh
```

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
*(Manages `~/.config/autostart/syspilot.desktop` pointing to `syspilot --tray`).*

---

## 📦 Directory Structure

```text
sysPilot/
├── Cargo.toml                   # Cargo Workspace root (syspilot-core, syspilot-tray, syspilot-gui)
├── bin/
│   ├── syspilot                 # Unified master CLI & GUI runner
│   └── syspilot-faf-repair      # Deterministic FAF runner & shortcut repair tool
├── crates/
│   ├── syspilot-core/           # Headless triage engine, CVE, LVFS, firewall, terminal & copilot runners
│   ├── syspilot-tray/           # High-performance 24/7 Rust StatusNotifierItem Tray Sentinel (~3.2 MB RAM)
│   └── syspilot-gui/            # Native Slint Desktop Cockpit (3-tab HUD, Maintenance Suite & AI Copilot)
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
│   └── syspilot.desktop         # XDG autostart entry for background tray sentinel
├── packaging/
│   ├── PKGBUILD                 # Pure Rust Arch Linux / AUR package specification (v2.0.0)
│   └── syspilot.desktop         # Application menu desktop launcher (Exec=syspilot --gui)
└── systemd/
    └── syspilot-tray.service    # User systemd service for 24/7 Rust Sentinel
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
