# AUDIT_MATRIX — SysPilot Certification & Governance Matrix

**Principal authority:** SRE Auditor Sol (`gpt-5.6-sol`)  
**Project:** SysPilot  
**Review date:** 2026-11-01  
**Governance blueprint:** `BP-ARGON-SPRINT-04-001`  
**Approval:** `ARGON-20261101-SPRINT-04-PASS`

---

## 1. Certification Matrix

| ID | Component | Canonical path | Status | Blast radius | Required verification |
|:---|:---|:---|:---:|:---:|:---|
| SYS-001 | Public governance backlog | `DEVELOPMENT_PROPOSALS.md` | CERTIFIED [Sol] | Low | Exactly one Clerical Inbox and one Rejected Decisions block |
| SYS-002 | Decoupling roadmap | `DECOUPLING_PLAN.md` | HARDENED [Legacy] | Medium | Targeted roadmap review before engine changes |
| SYS-003 | Runtime staging tree | `/home/gbur/Projects/sysPilot_Karol` | HARDENED [Legacy] | High | Regression suite and explicit promotion gate |
| SYS-004 | Legacy Rust SNI tray | `rust-tray/` | RETIRED [Argon] | Low | Replaced by `crates/syspilot-tray/` |
| SYS-005 | PyQt6 GUI | `gui/` | RETIRED [Argon] | Medium | Replaced by `crates/syspilot-gui/` (Slint 1.18) |
| SYS-006 | CLI entrypoint router | `bin/syspilot` | CERTIFIED [Argon] | High | POSIX array-safe multi-agent router with fallback to native binary |
| SYS-007 | Legacy Triage engine | `core/triage.py` | RETIRED [Argon] | High | Replaced by `crates/syspilot-core/` |
| SYS-008 | Pure Rust Triage Core | `crates/syspilot-core/` | CERTIFIED [Argon] | High | Resilient Serde Schema 1.0/2.0, Sprint 1, 2 & 3 100% (Subprocess Reaper, Network/Pacman timeouts, Telemetry clock-skew & atomic sync, 34 tests) |
| SYS-009 | Rust StatusNotifierItem Tray | `crates/syspilot-tray/` | CERTIFIED [Argon] | Medium | Zero-waste sentinel ~5.7 MB RSS, Sprint 4 Hardened (Lazy D-Bus reconnect, inotify leak prevention, triage deduplication Arc<AtomicBool>, 2s shutdown timeout, 16 tests) |
| SYS-010 | Slint Native Desktop GUI | `crates/syspilot-gui/` | CERTIFIED [Argon] | High | 3-tab cockpit, Maintenance suite, AI Copilot, SRE playbooks, dual renderers |
| SYS-011 | Packaging & XDG Integration | `packaging/` | CERTIFIED [Argon] | High | Pure Rust PKGBUILD v2.0.0, dedicated GUI & tray desktop entries, optdepends |
| GOV-001 | Clerical fast-path | matrix registry + `project_inbox.py` | CERTIFIED [Sol] | Medium | `validate_project_inbox.py`: PASS 12/12 |

---

## 2. GOV-001 Security Invariants

1. `/home/gbur/SystemDocs/.agents/agent-matrix.json` is the only project registry.
2. Clerical mode performs no source, Git, repository, journal or system telemetry scan.
3. A project resolves to exactly one owner-controlled, non-group/world-writable root.
4. Canonical backlog and matrix are regular owner-controlled files with one hardlink.
5. `eos-cleaner` writes to `pending-patches.md`, never through the compatibility symlink.
6. Exactly one stable owner-only lock serializes cooperating writers per project.
7. Input is at most 4096 bytes and contains no reserved marker or recognized secret.
8. Writes use same-filesystem temporary staging, file fsync, atomic replace and directory fsync.
9. `RECORDED` is emitted only after post-write verification.
10. `COMMIT_UNCERTAIN` is never retried automatically.
11. Inbox observations are not approved patches and grant no mutation authority.
12. Formal proposals require a targeted read of the bounded Rejected Decisions block.

---

## 3. Known Residual Risks

- `flock` is advisory; noncooperating writers are mitigated by owner-only roots and a
  pre-commit canonical snapshot check, not eliminated cryptographically.
- Secret detection is defense-in-depth and cannot replace user discipline or a dedicated
  DLP engine.
- Atomic rename protects readers from partial content; durability still depends on the
  underlying filesystem and storage honoring fsync.
- The fast-path is local-user governance, not a privilege boundary against the same UID
  or root.

---

## 4. Acceptance Gate

```text
python3 dev-tools/validate_project_inbox.py
EXPECTED: PASS 12/12
./dev-tools/verify-rust.sh -q
EXPECTED: RUST_VERIFY: PASS | TESTS: Total: 72/72 | CLIPPY: CLEAN | FMT: CLEAN | LINT: CLEAN
```

---

## 5. 3-Axis Module-by-Module SRE Audit Matrix (v2.1.1+)

Every module in `SysPilot` is evaluated across **3 mandatory SRE axes**:
1. **Axis 1 — Bugs & Mutation Resilience (`cargo mutants --in-place -p syspilot-core -f <file> -- --lib`):** Zero survived mutants on critical diagnostic logic; zero edge-case panics or silent failures.
2. **Axis 2 — Resource & CPU Optimization (Zero-Subprocess & Heap Hygiene):** Eliminate unnecessary `Command::new` subprocess forks in favor of pure-Rust `/proc`, `/sys`, `/etc`, `/var/lib/pacman/local` reads; enforce `libc::malloc_trim(0)` after background allocations; minimize background threads (`ksni` runtime unification).
3. **Axis 3 — 5-Archetype Universalism (`MockRig` + `sub_path`):** Every filesystem probe must accept `root: &Path` (`*_at`), use `sys_root::sub_path`, and pass across all 5 hardware/distro archetypes (`arch_amd_radeon`, `endeavouros_nvidia_dkms`, `cachyos_intel_hybrid_laptop`, `broken_reboot_state`, `headless_minimal_vm`).

| Module ID | File Path | Status | Axis 1 (Mutants / Bugs) | Axis 2 (Optimization / Subprocesses) | Axis 3 (Universalism / `_at(root)`) | Priority & Planned Action |
|:---|:---|:---:|:---|:---|:---|:---|
| `MOD-CORE-01` | `crates/syspilot-core/src/sys_root.rs` | 🟢 **CERTIFIED [Argon]** | 2/2 caught (100%) | Pure Rust (`0` forks) | Canonical `sub_path` guard | Done (`PATCH-026`) |
| `MOD-CORE-02` | `crates/syspilot-core/src/exec.rs` | 🟢 **CERTIFIED [Argon]** | Covered (`thread_local!`) | Whitelist gate | N/A | Done (`PATCH-026`) |
| `MOD-CORE-03` | `crates/syspilot-core/src/reboot_safety.rs` | 🟢 **CERTIFIED [Argon]** | Adversarial tested | Pure Rust (`0` forks) | 5 archetypes + `sub_path` | Done (`PATCH-026`) |
| `MOD-CORE-04` | `crates/syspilot-core/src/gaming.rs` | 🟢 **CERTIFIED [Argon]** | Adversarial tested | Pure Rust (`0` forks) | 5 archetypes + `sub_path` | Done (`PATCH-026`) |
| `MOD-CORE-05` | `crates/syspilot-core/src/sentinels.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Pure Rust (`0` forks) | Parameterized `*_at(root)` | Done (Stage 3 Audit) |
| `MOD-CORE-06` | `crates/syspilot-core/src/firewall.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Pure Rust (`0` forks; `systemctl` eliminated) | `check_host_firewall_at(root)` + 5 archetypes | Done (`Stage 1 Audit`) |
| `MOD-CORE-07` | `crates/syspilot-core/src/release.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Pure Rust `detect_install_origin_at` (`0` `pacman` forks) | `detect_install_origin_at(root, ...)` | Done (`Stage 1 Audit`) |
| `MOD-CORE-08` | `crates/syspilot-core/src/services.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Bounded `systemctl` with transient filter | `user_systemd_available_at(root, xdg)` | Done (`Stage 2 Audit`) |
| `MOD-CORE-09` | `crates/syspilot-core/src/orphans.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Bounded `pacman` + pure-Rust local DB | `list_local_pacman_db_entries_at(root)` | Done (`Stage 2 Audit`) |
| `MOD-CORE-10` | `crates/syspilot-core/src/firmware.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Pure-Rust EFI NVRAM + bounded `fwupdmgr` | `is_secure_boot_enabled_at(root)` | Done (`Stage 2 Audit`) |
| `MOD-CORE-11` | `crates/syspilot-core/src/security.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Shared `parse_timeout_ms` + `arch-audit` | Graceful offline & missing-binary fallback | Done (`Stage 2 Audit`) |
| `MOD-CORE-12` | `crates/syspilot-core/src/triage.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | `find` eliminated (`0` forks for `.pacnew`) | `find_pacnew_files_at(root)` | Done (Stage 3 Audit) |
| `MOD-CORE-13` | `crates/syspilot-core/src/models.rs` | 🟢 **CERTIFIED [Argon]** | `0` survived mutants (`cargo-mutants`) | Zero-fork PCI/sysfs GPU probe | `detect_gpu_at(root)` + `detect_os_at(root)` | Done (Stage 3 Audit) |
| `MOD-CORE-14` | `crates/syspilot-core/src/copilot.rs` | 🔴 **REDESIGN SLATED (`PATCH-027`)** | CLI probe tested | Forks `agy`/`goose` | To be replaced/streamlined with Zero-Subprocess AI Clipboard & Runbooks | **Tab 03 (`PATCH-027`)** |
| `MOD-TRAY-01` | `crates/syspilot-tray/` | 🟢 **HARDENED [Argon]** | `0` survived mutants (`cargo-mutants`) | `malloc_trim(0)` (`700 kB` `Private_Dirty`); **Opportunity:** `ksni` spawns extra `async-io` thread alongside `tokio` | Zero-root guard enforced | Done (Stage 3 Audit) |
| `MOD-GUI-01` | `crates/syspilot-gui/` | 🟢 **CERTIFIED [Argon]** | 10 headless + watcher tests | On-demand Slint 1.18 | Dual-Skin (`LCARS` + `Classic`) parity | Tab 03 (`PATCH-027`) |
