# Playbook: Gaming & Steam Performance Triage (Linux Gaming Readiness)

Comprehensive diagnostics and surgical repair procedures for gaming, Steam, Proton, and wineprefix performance on Arch Linux & derivatives.

---

## 1. Cardinal Philosophy: Zero Guesswork Diagnostics
Before altering graphics drivers, altering kernel parameters, or wiping game files:
1. Run the local gaming readiness check (`syspilot -g` or `sys-health --gaming`).
2. Verify multilib and 32-bit Vulkan ICD loader support.
3. Check GameMode daemon state.

---

## 2. Phase 1: 32-Bit Graphics Stack Verification
Proton and older DirectX/Vulkan games require 32-bit driver parity. Missing 32-bit libraries cause immediate crashes on launch.

Check installed Vulkan packages:
```bash
pacman -Qs vulkan
```
Verify matching 32-bit driver according to GPU architecture:
- **NVIDIA:** `lib32-nvidia-utils`
- **AMD:** `lib32-vulkan-radeon` (or `lib32-amdvlk`)
- **Intel:** `lib32-vulkan-intel`

Ensure the 32-bit Vulkan loader is present:
```bash
pacman -Q lib32-vulkan-icd-loader
```

---

## 3. Phase 2: Kernel Synchronization & Memory Limits
Modern Proton requires high memory-mapping counts (`vm.max_map_count`) and fast synchronization (`fsync` / `futex2`).

Verify memory map limit (must be >= 1048576):
```bash
cat /proc/sys/vm/max_map_count
```
If lower, enforce modern gaming value:
```bash
sudo sysctl -w vm.max_map_count=1048576
```

Verify split-lock mitigate setting (disabling avoids severe stutter in CPU-heavy games):
```bash
cat /proc/sys/kernel/split_lock_mitigate 2>/dev/null
```
If enabled (1), set to 0 for gaming sessions:
```bash
sudo sysctl -w kernel.split_lock_mitigate=0
```

---

## 4. Phase 3: Corrupted Proton Prefix Recovery
If a game fails to start after an update or Proton version change:
1. Identify the Steam AppID (e.g., from store URL).
2. Stop Steam:
   ```bash
   pkill -TERM steam
   ```
3. Backup or delete only the compatdata prefix (`~/.local/share/Steam/steamapps/compatdata/<APPID>/pfx`).
4. Re-launch Steam and run the game to rebuild a pristine prefix automatically.
