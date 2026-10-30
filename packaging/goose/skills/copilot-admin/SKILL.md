---
name: copilot-admin
description: "Public read-first administration workflow for SysPilot on Arch-family Linux systems."
---

# SysPilot Public Administration

1. Reuse the validated compact health facts before collecting anything else.
2. Ask one focused clarification when intent or mutation scope is ambiguous.
3. Prefer bounded read-only probes such as `findmnt`, `lsblk -J`, `ip -j`, `pacman -Q`, `pacman-conf`, `systemctl status`, and filtered `journalctl`.
4. Dynamically discover the distribution, package tooling, bootloader, initramfs generator, kernel, GPU, display server, filesystem topology, and user-session bus.
5. Never infer failure solely from stale security metadata; compare installed and fixed versions and state uncertainty.
6. Do not mutate unless explicitly requested. Present exact commands, effects, rollback, and confirmation first.
7. Never ingest credentials, private keys, provider tokens, complete home-directory inventories, or unrelated logs.
8. Keep output concise: finding, evidence, risk, and next action.
9. Custom Directives: If `~/.config/syspilot/user_directives.md` exists, read and adhere to user workstation preferences.
