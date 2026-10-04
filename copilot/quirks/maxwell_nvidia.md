# Hardware Quirks: NVIDIA Maxwell (GM204 / GTX 970) & DKMS Management

## 1. Hardware & Driver Architecture
* **Target Architecture:** NVIDIA GeForce GTX 970 (4GB VRAM: 3.5GB fast + 0.5GB segmented, GM204 Maxwell).
* **Official Driver Branch:** `nvidia-580xx-dkms` (along with `nvidia-580xx-utils`, `nvidia-580xx-settings`, and `lib32-nvidia-580xx-utils`).
* **Kernel Drivers in Use:** Proprietary `nvidia` (modules: `nvidia`, `nvidia_modeset`, `nvidia_uvm`, `nvidia_drm`).

---

## 2. Critical Hardware Constraints & Upstream Pitfalls

### A. Driver Packages Incompatible with Maxwell:
* **`nvidia-open` (Open-Source GPU Kernel Modules):**  
  Maxwell architecture (GM204) **lacks the GSP firmware processor** required by modern open-source NVIDIA kernel modules (GSP support begins with Turing: RTX 20xx / GTX 16xx). Attempting to install `nvidia-open` will prevent the display server from initializing.
* **Standard `nvidia` package:**  
  The mainline `nvidia` package in official Arch Linux repositories frequently transitions to newer driver branches that drop legacy architecture support. Maxwell requires the legacy DKMS branch (`580xx`).

### B. Nouveau Limitations (Open-Source Fallback):
* The open-source `nouveau` driver **lacks dynamic reclocking support** for Maxwell GM204 without signed firmware. As a result, GPUs running on `nouveau` remain locked at low base boot clocks (~135 MHz), providing roughly ~10% nominal 3D performance and failing modern Steam/Proton workloads.
* In Dracut or Mkinitcpio configs, nouveau should be explicitly blacklisted or omitted when proprietary drivers are deployed.

### C. Display Server Compatibility: X11 vs Wayland:
* **Recommendation:** **X11 (Xorg)** remains the baseline stable, predictable display server for Maxwell on legacy proprietary drivers.
* **Wayland on Maxwell (580xx):** Despite recent explicit sync additions, older Maxwell cards running proprietary drivers on Wayland can exhibit intermittent frame drops, DPMS display wakeup failures upon resume from suspend, and fullscreen game pacing jitter.

### D. "Fliplock Jitter" in Xorg Logs:
* In `/var/log/Xorg.0.log`, harmless messages such as `Failed to request fliplock` may appear. These represent display-pipe synchronization negotiation under KWin and can be safely ignored unless accompanied by hard GPU lockup errors (`NVRM: Xid`).
