# Playbook: Bootloader, Dracut & EFI Recovery Guide

Surgical guide for diagnosing and repairing kernel, initramfs, and bootloader discrepancies on Arch Linux & EndeavourOS without blind reinstallations.

---

## 1. Safety Guardrails & Reversibility
- **Never reboot before verification:** If an initramfs or kernel update failed, do not reboot until you confirm valid kernel images and initramfs files exist on the active boot/ESP partition.
- **Dynamic Mount Discovery:** Check `/boot` and `/efi` mountpoints using `findmnt`. Never assume fixed paths.
- **Fallback Kernel Integrity:** Always verify that at least one functional LTS or alternate kernel exists before modifying the primary kernel.

---

## 2. Dynamic Boot & Initramfs Diagnostic Phase
Inspect currently installed kernels:
```bash
pacman -Q | grep -E '^linux(-lts|-zen|-hardened|-cachyos)? '
```
Verify matching modules in `/usr/lib/modules`:
```bash
ls -la /usr/lib/modules/
```
Check active ESP and boot mountpoints:
```bash
findmnt /boot
findmnt /boot/efi || findmnt /efi
```

---

## 3. Initramfs Regeneration Protocol

### Dracut Engine:
To rebuild initramfs for all installed kernels using Dracut:
```bash
sudo dracut-rebuild
```
Or target a specific kernel:
```bash
sudo dracut --force --kver <KERNEL_VERSION>
```

### Mkinitcpio Engine:
If the system uses mkinitcpio:
```bash
sudo mkinitcpio -P
```

---

## 4. Bootloader Sync Phase

### GRUB:
```bash
sudo grub-mkconfig -o /boot/grub/grub.cfg
```

### systemd-boot:
```bash
bootctl status
sudo bootctl update
```
Verify Type #1 entries or Type #2 UKIs:
```bash
ls -la /boot/loader/entries/ || ls -la /efi/loader/entries/
```

---

## 5. Verification Gate
Inspect the generated initramfs image sizes:
```bash
ls -lh /boot/*.img /boot/efi/EFI/*/ /efi/EFI/*/ 2>/dev/null
```
If file size is greater than 10MB and permissions are intact, the system is primed for safe reboot.
