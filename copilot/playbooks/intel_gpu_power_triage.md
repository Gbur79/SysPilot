# Playbook: Intel iGPU Load Spikes & Video Acceleration Triage

A surgical guide to diagnosing Intel Integrated Graphics (HD, UHD, Iris Xe, Arc) load spikes, frequency scaling, and hardware video acceleration on Arch Linux / EndeavourOS.

---

## 1. The Core Architecture: The "100% iGPU" Paradox
- **Dynamic Clock Scaling:** Intel iGPUs throttle down to their minimum base clock (often 300 MHz or 350 MHz) when idle to conserve battery and thermals.
- **The Percentage Paradox:** If an iGPU is clocked at 300 MHz and a background browser tab or desktop animation demands 300 MHz of compute, system monitors may report **"100% GPU Usage"** even though power draw is minuscule (~1-2 Watts).
- **Actual Load vs Busy Engine:** High percentage is only a problem if accompanied by high GPU frequency, high package temperatures (>75°C), or fan ramp-ups.

---

## 2. Dynamic Discovery & Diagnostics

### Step 1: Install official Intel GPU diagnostic tooling
```bash
sudo pacman -S --needed intel-gpu-tools
```

### Step 2: Live engine inspection with `intel_gpu_top`
```bash
sudo intel_gpu_top
```
**What to look for in `intel_gpu_top`:**
- **Freq MHz (req / act):** If actual frequency is near base (e.g. 300–450 MHz) while Render engine is busy, the GPU is merely idling efficiently.
- **Engines breakdown:**
  - `Render/3D`: Desktop compositing (KWin, Mutter), games, WebGL.
  - `Video`: Hardware video decoding/encoding (VA-API).
  - `VideoEnhance`: Post-processing or upscaling.
- **Power (Watts):** Real GPU package power consumption.

### Step 3: Check active power profiles in KDE Plasma / GNOME
```bash
powerprofilesctl get
```
*(Switching between `balanced` and `performance` alters GPU clock floor governor).*

---

## 3. Hardware Video Acceleration (VA-API) Triage

Software video decoding in browsers (Chromium/Firefox) forces heavy CPU and iGPU rendering load. Proper VA-API drivers offload this to dedicated fixed-function silicon.

### Driver Matrix:
| Architecture | Generation | Required Driver | Package |
| :--- | :--- | :--- | :--- |
| **Broadwell through modern (Gen 8+)** | Core 5th gen up to Ultra/Arc | `iHD` | `intel-media-driver` |
| **Legacy Intel (Haswell and older, Gen 4–7)** | Core 4th gen and older | `i965` | `libva-intel-driver` |

### Step 1: Verify VA-API capabilities
```bash
sudo pacman -S --needed libva-utils
vainfo
```
- Ensure the output displays supported profile codecs (`VAProfileH264...`, `VAProfileVP9...`, `VAProfileAV1...` with `VAEntrypointVLD`).
- If `vainfo` fails or errors out with `vaInitialize failed`:
  ```bash
  # For 5th Gen (Broadwell) and newer:
  sudo pacman -S --needed intel-media-driver
  
  # For 4th Gen (Haswell) and older:
  sudo pacman -S --needed libva-intel-driver
  ```

### Step 2: Browser Hardware Acceleration
In Firefox or Chromium, navigate to video playback:
- **Firefox:** Check `about:support` -> *Graphics* -> *Compositing: WebRender*, *Hardware H264 Decoding: supported*.
- **Chromium / Brave:** Check `chrome://gpu` -> *Video Decode: Hardware accelerated*.

---

## 4. Verification
1. Run `sudo intel_gpu_top` while streaming a 1080p or 4K 60fps video.
2. Confirm that the **Video** engine is active (>20%) while **Render/3D** stays low (<15%).
3. Package temperatures remain cool and thermals stable.
