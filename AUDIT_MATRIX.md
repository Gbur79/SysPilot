# AUDIT_MATRIX — SysPilot Certification & Governance Matrix

**Principal authority:** SRE Auditor Sol (`gpt-5.6-sol`)  
**Project:** SysPilot  
**Review date:** 2026-11-01  
**Governance blueprint:** `BP-ARGON-SPRINT-03-001`  
**Approval:** `ARGON-20261101-SPRINT-03-PASS`

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
| SYS-009 | Rust StatusNotifierItem Tray | `crates/syspilot-tray/` | CERTIFIED [Argon] | Medium | Zero-waste sentinel ~5.7 MB RSS, zbus 5.19, ksni |
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
