# Playbook: Wayland AppImage & Flatpak Graphics Triage

A surgical guide to resolving DMA-BUF protocol errors, blank video players, and crashes when running AppImages or Flatpaks on Wayland compositors (KWin Wayland, Hyprland, Sway, GNOME).

---

## 1. Anatomy of the Failure
- **The Error Signature:**
  ```text
  [destroyed object]: error 7: importing the supplied dmabufs failed
  The Wayland connection experienced a fatal error: Error of protocol
  ```
- **Root Cause:**
  - Standalone containers (AppImages, Flatpaks) bundle their own graphics libraries (Qt, GTK, internal Mesa or ffmpeg).
  - When communicating over Wayland via DMA-BUF (direct memory access buffer sharing) for hardware decoding or surface rendering, version mismatches between bundled container libraries and the host compositor kernel/Mesa driver cause protocol rejections.
  - Video players (e.g., Kdenlive preview monitor, MPV, VLC) show a black/grey screen or terminate with a fatal protocol abort.

---

## 2. Surgical Triage Hierarchy

### Tier 1: Prefer Official Arch Repositories (Native Host Mesa)
The simplest, most stable resolution on rolling-release Arch Linux is to install native packages compiled directly against the system's active Mesa and Qt stack:
```bash
# E.g. for Kdenlive:
sudo pacman -S --needed kdenlive breeze-icons mediainfo
```
Native packages eliminate DMA-BUF protocol mismatches because they share the exact host DRI/Mesa drivers.

---

### Tier 2: Force XWayland Fallback (For AppImages & Standalone Binaries)
If you must use an AppImage or standalone binary that crashes on Wayland, force it to run under XWayland rather than native Wayland:

#### For Qt Applications (Kdenlive, FreeCAD, OBS, etc.):
```bash
QT_QPA_PLATFORM=xcb ./application.AppImage
```

#### For GTK Applications (Inkscape, GIMP, etc.):
```bash
GDK_BACKEND=x11 ./application.AppImage
```

#### For Electron Applications:
```bash
./application.AppImage --ozone-platform=x11
```

*Tip:* You can create a persistent desktop launcher override in `~/.local/share/applications/` specifying the `QT_QPA_PLATFORM=xcb` environment prefix.

---

### Tier 3: Flatpak Permissions & Socket Overrides
If running inside Flatpak and experiencing video preview failures or missing acceleration:

1. **Verify GPU access:**
   ```bash
   flatpak override --user --device=dri <app.id.here>
   ```

2. **Allow X11 fallback socket:**
   ```bash
   flatpak override --user --socket=x11 --socket=fallback-x11 <app.id.here>
   ```

3. **Force X11 mode within Flatpak:**
   ```bash
   flatpak run --env=QT_QPA_PLATFORM=xcb <app.id.here>
   ```

---

## 3. Verification & Diagnostic Commands
To check whether an application is running natively under Wayland or routed through XWayland:
```bash
# Inspect window protocol
xlsclients
```
If the application appears in `xlsclients`, it is running under XWayland and avoids DMA-BUF protocol aborts.
