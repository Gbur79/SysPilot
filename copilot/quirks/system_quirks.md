# Arch Linux & Community Distro Environmental Quirks

This document catalogues upstream packaging quirks, bootloader configurations, and environmental factors across Arch Linux and derivatives (EndeavourOS, CachyOS, Manjaro).

---

## 1. Mirrorlist Management & Regional Routing
* **Recommended Engine:** Use `syspilot --mirrors` or `sys-health --mirrors`. It includes an Atomic Fallback Staging Gate (generates the candidate list in `/tmp`, verifies live HTTP 200/TTFB against `core.db`, and creates a verified `.bak` before applying).
* **Missing Country Mirrors:** Some country codes (e.g. Ireland `IE`) do not host official tier-1 Arch mirrors. Running tools like `reflector --country IE` results in an empty list error.
* **Safe Fallback Protocol:** The universal mirror benchmark automatically probes the 20 freshest HTTPS mirrors globally, evaluating low latency and high bandwidth across neighboring regional backbones.
* **Truncation Warning:** Never pipe unverified rank commands directly into `/etc/pacman.d/mirrorlist` without testing status codes first. Doing so can truncate the mirrorlist to 0 bytes and break pacman database operations.

---

## 2. Bootloader Topologies & ESP Layouts
* **Split Partition Topologies:**
  - Common desktop setup: `/boot` resides on the root filesystem (e.g. Btrfs subvolume or ext4), while `/efi` or `/boot/efi` is an independently mounted vfat ESP partition.
* **Dracut Machine-ID Quirk:**
  - Running a generic `dracut --regenerate-all --force` on certain configurations can fail if Dracut attempts to enforce BLS Type #1 UKI paths (`/boot/efi/<machine-id>/<kver>/`) when standard GRUB traditional images are expected in `/boot`.
  - **Surgical Best Practice:** Rebuild targeting specific installed kernel versions explicitly:
    ```bash
    sudo dracut --kver <KERNEL_VERSION> --force
    ```
  - Or use the official package hook trigger:
    ```bash
    sudo dracut-rebuild
    ```

---

## 3. Magic SysRq Emergency Recovery
* **Arch Linux / systemd Upstream Default:**
  - Standard Arch Linux configures `/proc/sys/kernel/sysrq` to `16` (enables sync) or `176` (sync + unmount + remount read-only).
  - This is an intentional upstream security default and should not be flagged as a critical error.
  - To enable full emergency key control (`REISUB` combo for frozen systems):
    ```bash
    sudo sysctl -w kernel.sysrq=1
    ```
