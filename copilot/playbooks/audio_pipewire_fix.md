# Playbook: Audio, PipeWire & WirePlumber Fix

Diagnostic and recovery procedure for desktop audio issues (no sound, stuttering, missing audio sinks, or sound server hang) under PipeWire.

---

## 1. Golden Rule: User Session vs Root
> [!IMPORTANT]
> On modern Arch Linux and derivatives, the entire audio stack (**PipeWire**, **WirePlumber**, **pipewire-pulse**) runs strictly as a **user session service (`systemd --user`)**, NOT as a root daemon!  
> **Never run:** `sudo systemctl restart pipewire` (this fails and corrupts user socket permissions).

---

## 2. Phase 1: Inspect Audio Service State
Check the status of user audio units in the current session:
```bash
systemctl --user status pipewire wireplumber pipewire-pulse --no-pager
```
Check if any audio unit entered a failed state:
```bash
systemctl --user --failed
```

---

## 3. Phase 2: Inspect Active Audio Sinks
Verify available audio output endpoints:
```bash
wpctl status
```
Inspect current default sink volume and mute status:
```bash
wpctl get-volume @DEFAULT_AUDIO_SINK@
```
If muted:
```bash
wpctl set-mute @DEFAULT_AUDIO_SINK@ 0
```

---

## 4. Phase 3: Surgical Session Service Restart
If the audio daemon is frozen or disconnected:
```bash
systemctl --user restart pipewire pipewire-pulse wireplumber
```
Verify PipeWire socket response:
```bash
pactl info
```

---

## 5. Phase 4: Reset Corrupted WirePlumber State Cache
If audio sinks disappear after a system upgrade or sleep/resume cycle:
```bash
systemctl --user stop wireplumber pipewire pipewire-pulse
rm -rf ~/.local/state/wireplumber/*
systemctl --user start pipewire pipewire-pulse wireplumber
```
