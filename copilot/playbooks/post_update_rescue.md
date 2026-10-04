# Playbook: Post-Update Rescue & Recovery

Step-by-step emergency triage and recovery procedure following an interrupted or broken system update (`pacman -Syu` / `yay`): transaction locks, black screen on boot, kernel/initramfs desynchronization, corrupted packages, or display manager failure.

---

## Phase 1: Rapid Telemetry & State Inspection
Before running any modifying commands, inspect local runtime telemetry:
```bash
# 1. Read compact triage state
cat ~/.local/state/syspilot/status.json 2>/dev/null || syspilot -s

# 2. Check recent pacman log transactions
tail -n 45 /var/log/pacman.log
```
*Key indicators to check:* `kernel`, `initramfs`, `dkms`, `gpu_runtime`, `db.lck`, `failed_services`.

---

## Phase 2: Interrupted Transaction & Stale Pacman Lock
If an update was abruptly interrupted (power loss, system freeze, or display crash):
1. **Verify if another pacman process is still running:**
   ```bash
   pgrep -l "pacman|yay|paru"
   ```
   If no package manager is running, remove the stale database lock:
   ```bash
   sudo rm -f /var/lib/pacman/db.lck
   ```

2. **Re-synchronize databases and finish incomplete transactions:**
   ```bash
   sudo pacman -Syu
   ```

---

## Phase 3: Kernel, Initramfs & DKMS Re-alignment
If the system boots to a black screen, blinking cursor, or initramfs rescue prompt:
1. **Verify matching kernel modules exist for the running/installed kernel:**
   ```bash
   ls -la /usr/lib/modules/
   ```

2. **Rebuild DKMS modules (e.g. proprietary NVIDIA or virtualbox):**
   ```bash
   sudo dkms status
   sudo dkms autoinstall
   ```

3. **Rebuild initramfs images:**
   - If using Dracut:
     ```bash
     sudo dracut-rebuild
     ```
   - If using Mkinitcpio:
     ```bash
     sudo mkinitcpio -P
     ```

4. **Re-generate bootloader configuration:**
   - If using GRUB:
     ```bash
     sudo grub-mkconfig -o /boot/grub/grub.cfg
     ```
   - If using systemd-boot:
     ```bash
     sudo bootctl update
     ```

---

## Phase 4: Display Manager & Desktop Session Triage
If the system boots to a TTY or display server crashes on startup:
1. Check display manager status (e.g., SDDM, GDM, LightDM):
   ```bash
   systemctl status display-manager.service --no-pager
   ```
2. Inspect recent graphical errors:
   ```bash
   journalctl -u display-manager.service -b 0 -p 3 --no-pager
   ```
3. Verify GPU driver module loading:
   ```bash
   lspci -k | grep -EA3 'VGA|3D'
   ```
