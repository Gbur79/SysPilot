# Playbook: Safe .pacnew Configuration Triage & Merging

A surgical, non-destructive guide to triaging and merging `.pacnew` files on Arch Linux / EndeavourOS.

---

## 1. Safety Guardrails
- **NEVER overwrite a configuration blindly** (`mv file.pacnew file` is STRICTLY FORBIDDEN).
- **Inspect diff first:** Always run `diff -u file file.pacnew` or `DIFFPROG=diff pacdiff -s` to view exact upstream modifications.
- **Root caution:** Many `.pacnew` files (like `/etc/passwd.pacnew`, `/etc/shadow.pacnew`, `/etc/group.pacnew`) contain default users/groups and MUST NOT be merged directly without manual entry comparison.
- **Common low-risk files:**
  - `/etc/security/pam_env.conf.pacnew`: Upstream comments/variables.
  - `/etc/default/useradd.pacnew`: Default shell changes.
  - `/etc/makepkg.conf.pacnew`: Updated compiler flags (keep custom `MAKEFLAGS="-j$(nproc)"`).

---

## 2. Dynamic Discovery
To find all active `.pacnew` files:
```bash
find /etc -maxdepth 4 -name "*.pacnew" 2>/dev/null
```
Or use the official Arch utility:
```bash
pacdiff -o
```

---

## 3. Surgical Triage Protocol

### Scenario A: Identical or trivial comment changes
If `diff -u -w -B /etc/file /etc/file.pacnew` only shows comment or whitespace changes:
```bash
sudo rm /etc/file.pacnew
```

### Scenario B: New syntax / deprecated options
If upstream changed syntax (e.g. `/etc/systemd/resolved.conf.pacnew` or `/etc/pacman.conf.pacnew`):
1. Review the diff.
2. Port only the new configuration parameters into your active file.
3. Remove the `.pacnew` once reconciled:
   ```bash
   sudo rm /etc/file.pacnew
   ```

### Scenario C: Interactive 3-way merge tools
If the user prefers graphical or terminal diff:
- `DIFFPROG=meld pacdiff` (Graphical GUI merge via Meld)
- `DIFFPROG=vimdiff pacdiff` (Terminal 2-way diff)
- `DIFFPROG="diff -u --color=always" pacdiff` (Quick read-only overview)

---

## 4. Verification
After merging, verify the active service or package syntax before restarting:
- For pacman: `pacman -V`
- For systemd units: `systemd-analyze verify /etc/systemd/...`
- For sddm: `sddm --example-config`
