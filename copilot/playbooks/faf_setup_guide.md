# Playbook: Forged Alliance Forever (FAF) Setup & Troubleshooting Guide

Step-by-step setup, configuration, and troubleshooting guide for Forged Alliance Forever (FAF) and Supreme Commander: Forged Alliance on Arch Linux & EndeavourOS.

---

## 1. Architecture & Runner Strategy
- **DO NOT install the AUR package (`downlords-faf-client`):** The AUR package merely ships the Java client frontend. It does not configure an isolated Wine prefix, the Steam Runtime (`pressure-vessel`) container, DXVK patches, or the native launcher scripts.
- **Official Recommended Community Solution:** The native runner suite: [FAForever/faf-linux](https://github.com/FAForever/faf-linux) located in `~/faf-linux`.
- **System Prerequisites:**
  - Supreme Commander: Forged Alliance installed on Steam (AppID: `9420`).
  - Multilib repository and 32-bit graphics stack enabled (`syspilot -s` or `sys-health --gaming`).
  - Core system utilities installed: `bubblewrap`, `curl`, `jq`, `git`.

---

## 2. Step-by-Step Clean Setup

### Step 1: Force First-Time Profile Creation in Steam
Launch Supreme Commander: Forged Alliance once directly from Steam (using standard Proton), enter the main menu, and exit.
This creates the initial `Game.prefs` file in:
`~/.local/share/Steam/steamapps/compatdata/9420/pfx/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance/Game.prefs`

### Step 2: Clone and Initialize `faf-linux`
```bash
git clone https://github.com/FAForever/faf-linux ~/faf-linux
cd ~/faf-linux
./setup.sh
```
*Note:* The setup script automatically provisions Java Temurin 25, Steam Linux Runtime 4, Proton GE, and DXVK.

### Step 3: First Login Authentication
Launch the client once:
```bash
~/faf-linux/run
```
Authenticate your FAF account via your default web browser, and once successfully logged in, close the client.

### Step 4: Synchronize `Game.prefs` into the FAF Prefix
Copy your working Steam game preferences into the isolated `faf-linux` prefix:
```bash
mkdir -p "$HOME/faf-linux/prefix/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance"
cp -r "$HOME/.local/share/Steam/steamapps/compatdata/9420/pfx/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance/Game.prefs" \
      "$HOME/faf-linux/prefix/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance/"
```

### Step 5: Configure Client Execution Paths
Execute the automated path setup script:
```bash
cd ~/faf-linux && ./set-client-paths.sh
```

---

## 3. Common Failure Modes & Quick Fixes

### Scenario A: Game launches to black screen or crashes on launch
- **Cause:** Missing 32-bit graphics libraries.
- **Fix:** Ensure `lib32-vulkan-icd-loader` and your GPU's 32-bit driver (`lib32-nvidia-utils` or `lib32-vulkan-radeon`) are installed:
  ```bash
  sudo pacman -S --needed lib32-vulkan-icd-loader
  ```

### Scenario B: "Game.prefs could not be found or written"
- **Cause:** Discrepancy between Steam user prefix path and FAF client path.
- **Fix:** Re-run Step 4 above to ensure `Game.prefs` is in the FAF prefix, and verify write permissions:
  ```bash
  chmod -R u+rw "$HOME/faf-linux/prefix"
  ```
