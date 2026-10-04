# Playbook: Systemd Failed Units Diagnostic & Safe Recovery

A surgical, non-destructive guide to diagnosing and clearing failed systemd units (services, timers, mounts) on Arch Linux / EndeavourOS.

---

## 1. Safety Guardrails & Architecture
- **Fail-latched behavior:** Systemd latches any crashed or error-exited unit into a persistent `failed` state. It will not clear until explicitly reset or rebooted.
- **Inspect before resetting:** Never run `systemctl reset-failed` blindly. Always read the unit journal first to determine why it failed.
- **Dry-run maintenance jobs:** Maintenance utilities like `systemd-tmpfiles-clean` support `--dry-run --verbose`. Use dry-run before altering filesystem permissions.
- **Root vs User units:** Remember that `systemctl --failed` only lists system-wide units. User-level units require `systemctl --user --failed`.

---

## 2. Dynamic Discovery

### Check for active failures:
```bash
systemctl --failed --no-legend --no-pager
systemctl --user --failed --no-legend --no-pager
```

### Inspect the failure journal for a specific unit:
```bash
journalctl -u <unit_name> -b --no-pager -n 50
```
*(For user units, add `--user`: `journalctl --user -u <unit_name> -b --no-pager -n 50`)*

---

## 3. Surgical Triage Protocols by Unit Type

### Case A: `systemd-tmpfiles-clean.service`
Triggered daily by `systemd-tmpfiles-clean.timer` to purge stale temporary files in `/tmp` and `/var/tmp`.

1. **Inspect error entries in journal:**
   ```bash
   journalctl -u systemd-tmpfiles-clean.service -b --no-pager
   ```
2. **Execute dry-run to identify the offending path:**
   ```bash
   sudo systemd-tmpfiles --clean --dry-run --verbose
   ```
3. **Common culprits:**
   - **Container / VM sockets:** Containers (Podman, Docker) or Flatpak/Wine runtimes creating locked or unreadable subdirectories in `/tmp`. Check ownership: `ls -ld /tmp/* /var/tmp/*`.
   - **Broken configuration:** Broken or deprecated directives left in `/etc/tmpfiles.d/` pointing to deleted users or non-existent paths.
4. **Recovery & verification:**
   ```bash
   sudo systemctl reset-failed systemd-tmpfiles-clean.service
   sudo systemctl start systemd-tmpfiles-clean.service
   systemctl status systemd-tmpfiles-clean.service
   ```
   *(Ensure status reports `code=exited, status=0/SUCCESS`).*

---

### Case B: `systemd-coredump@.service` or Journal Failures
Triggered when an application crashes to capture stack traces.
1. **List recent core dumps:**
   ```bash
   coredumpctl list -n 10
   ```
2. **Clear stalled coredump unit:**
   ```bash
   sudo systemctl reset-failed systemd-coredump*.service
   ```

---

### Case C: Failed Storage or Network Mounts (`*.mount`)
Occurs when an external USB drive, NFS share, or secondary partition failed to mount during boot.
1. **Check mount configuration:**
   ```bash
   findmnt --verify
   cat /etc/fstab
   ```
2. **Safely clear mount failure if partition was intentionally detached:**
   ```bash
   sudo systemctl reset-failed <mount-unit-name>
   ```

---

## 4. Resetting & Final Verification
Once the root cause is resolved or confirmed transient:
```bash
# System units
sudo systemctl reset-failed <unit_name>

# User units
systemctl --user reset-failed <unit_name>

# Verify system is clean
systemctl --failed
```
Run `syspilot -s` to verify that `Failed Units: 0` is reported.
