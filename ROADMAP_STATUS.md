# SysPilot Modernization: Living Tracker & SRE Runbook

**Właściciel Projektu:** Karol (Dublin, Ireland)  
**Nadzór Architektoniczny:** SRE Auditor (Auditor Argon Pro) & SysPilot SRE Team  
**Ostatnia aktualizacja:** 2026-10-31T19:07:00Z  
**Status projektu:** 🟢 **FAZA 1, FAZA 2, FAZA 3 I FAZA 4 ZAKOŃCZONE ✔ (TESTY I INSTALACJA ZATWIERDZONE)**  
**Lokalizacja kanoniczna w repozytorium:** `Projects/sysPilot/ROADMAP_STATUS.md`

---

## 1. Cel Projektu i Żelazne Inwarianty

Przekształcenie `SysPilot` w zintegrowany, lekki i niezawodny pakiet dla Arch Linux / EndeavourOS:
1. **Ultralekki Sentinel 24/7 (Rust):** Daemon `syspilot-tray` w KDE Plasma Wayland zużywający **~1.2 MB RAM i 0.0% CPU**, z natywnym D-Bus (`zbus`) i asynchronicznym sprawdzaniem aktualizacji (Core, Official, AUR, Third-Party).
2. **Architektura Luźnego Sprzężenia (Decoupled UI):** Czyste oddzielenie Silnika Sentinela od Dashboardu UI. Przyszłe zmiany interfejsu (np. lifting stylów, Slint/Tauri/QML) nie wymagają modyfikacji ani rekompilacji kodu w Rust.
3. **Żelazna Segregacja Tożsamości:** Kategoryczny zakaz używania prefiksów i nazw prywatnych agentów w publicznym repozytorium (oficjalna tożsamość: **`SysPilot AI Copilot`**). Prywatne profile konfigurowane są wyłącznie przez lokalny `~/.config/syspilot/config.env`.
4. **100% Uniwersalizm Sprzętowy:** Dynamiczny odczyt GPU i CPU z `summary.json` oraz `nvidia-smi`/`lspci` bez hardkodowania parametrów GTX 970 / DP MST.

---

## 2. Rejestr Postępów i Status Zadań (Roadmap Checklist)

### 🟢 FAZA 1: Szybkie Poprawki UX & IPC (ZAKOŃCZONE ✔)
- [x] **Rozwiązanie Double Window Bug (BUG-01):**
  - [x] Debounce 1.5s w `rust-tray/src/tray.rs` (`activate()`).
  - [x] Atomowa blokada `fcntl.flock` na pliku `.gui.lock` w `gui/syspilot_gui.py` (`run_gui_single_instance`).
  - [x] Wczesny start `QLocalServer` przed tworzeniem ciężkiego okna Qt.
- [x] **Likwidacja opóźnienia i desynchronizacji (BUG-02):**
  - [x] Usunięcie sztucznego opóźnienia 1000 ms (`singleShot`) w `gui/syspilot_gui.py` (skrócono do 100 ms).
  - [x] Ujednolicenie logiki `effective_status` w Pythonie z Rustem (eliminacja lagu Amber vs Green).
- [x] **Weryfikacja:** Test kompilacji Rust (`cargo test` 5/5 pass, `cargo clippy` clean), `py_compile`, instalacja binarki, test jednoczesnego uruchomienia 2 procesów (tylko 1 proces pozostał aktywny).

---

### 🟢 FAZA 2: Silnik Rust 24/7 & Natywny D-Bus (ZAKOŃCZONE ✔)
- [x] **Moduł wartowników `sentinels.rs`:**
  - [x] `check_reboot_pending()` (detekcja wymiany jądra w `/usr/lib/modules/<version>`).
  - [x] `check_pacman_lock()` (inteligentna detekcja osieroconej blokady `/var/lib/pacman/db.lck` względem aktywnych procesów ALPM w `/proc`).
  - [x] `get_disk_stats()` (bezpośrednie wywołanie `libc::statvfs` dla `/` oraz `/home`, 0 fork/exec).
- [x] **Moduł detekcji aktualizacji `updater.rs`:**
  - [x] `check_official_updates()` (izolowana baza w `/tmp/checkup-db`, bezpieczny auto-heal blokad).
  - [x] Detekcja pakietów Core (`linux`, `systemd`, `nvidia`, `grub`, `dracut`, `glibc`, `pipewire`).
  - [x] Asynchroniczne sprawdzanie AUR (`yay -Qua` / `paru -Qua`) oraz narzędzi third-party (`sys-health --software --json`).
  - [x] GameMode Freeze: zamrożenie skanów dysku i sieci w trakcie aktywnej rozgrywki.
- [x] **Natywny D-Bus (`zbus 5.19`) w `gamemode.rs`:**
  - [x] Eliminacja okresowego spawnowania `busctl` (oszczędność ~28 800 procesów na dobę).
- [x] **Spłaszczenie menu kontekstowego i dynamiczny ToolTip:**
  - [x] 5 ergonomicznych akcji: Dashboard, Diagnostic, Upgrade, Copilot, Exit.
  - [x] Wieloliniowy ToolTip informujący o statusie, pakietach Core, restarcie i dysku.
- [x] **Weryfikacja:**
  - [x] 10/10 testów jednostkowych w `src/tests.rs` zaliczonych (`cargo test --locked`).
  - [x] `cargo clippy --locked --all-targets -- -D warnings` czysty (0 ostrzeżeń).
  - [x] Zbudowano wersję zoptymalizowaną (`cargo build --release --locked`), zainstalowano do `~/.local/bin/syspilot-tray`.
  - [x] Usługa `syspilot-tray.service` aktywna i zweryfikowana na magistrali sesyjnej D-Bus (1.2 MB RAM).

---

### 🟢 FAZA 3: UI Dashboard & Uniwersalny AI Copilot (ZAKOŃCZONE ✔)
- [x] **Dynamiczny wiersz Hardware & GPU:**
  - [x] Dodano wiersz `Hardware Platform & GPU` w sekcji *System Health & Hygiene*.
  - [x] Dynamiczne wykrywanie CPU (`/proc/cpuinfo`) oraz GPU (`nvidia-smi` / `lspci` / `summary.json`).
  - [x] Zweryfikowano u Karola: `PASS ✔ (NVIDIA GeForce GTX 970 [580.196.07] • Intel Core i7-4790K)`.
- [x] **Uniwersalny, odsprzężony AI Copilot:**
  - [x] Wprowadzono resolver `~/.config/syspilot/config.env` (`SYSPILOT_COPILOT_CMD`, `COPILOT_LABEL`, `SYSPILOT_GOOSE_RECIPE`).
  - [x] `bin/syspilot --copilot-shell` oraz przycisk w UI transparentnie honorują konfigurację lokalną użytkownika.
  - [x] Usunięto sztywne nazwy silników z widoków UI i nagłówków zakładek na rzecz neutralnego `SysPilot AI Copilot`.
- [x] **Weryfikacja:** `py_compile` czysty, inicjalizacja okna w trybie offscreen zakończona sukcesem.

---

### 🟢 FAZA 4: Certyfikacja Końcowa, Live Watcher & Stabilizacja (ZAKOŃCZONE ✔)
- [x] **Live Telemetry Watcher & Auto-Refresh:**
  - [x] Monitorowanie czasu modyfikacji `status.json` w `syspilot-gui` (1 Hz).
  - [x] Usunięcie błędu thread-affinity Slinta (`BP-ARGON-20261031-SYS-010-WATCHER`) poprzez `upgrade_in_event_loop` i `Arc<AtomicBool>`.
  - [x] Automatyczna synchronizacja otwartego okna do stanu `FLIGHT READY ✔` natychmiast po ukończeniu guarded upgrade w terminalu.
- [x] **Certyfikacja i testy:** 32/32 testów PASS, Clippy clean, wydanie v2.0.1 skompilowane i zainstalowane w `~/.local/bin/`.

---

### 🟡 FAZA 5: Proaktywny Audyt Architektoniczny SRE (6-Sprint SRE Audit Plan)
*Kanoniczny dokument planu audytu:* [SRE_AUDIT_PLAN.md](./SRE_AUDIT_PLAN.md)  
*Status ogólny fazy:* **W TRAKCIE (Sprint 1 zaliczony, Sprint 2 oczekuje na realizację)**

- [x] **Sprint 1: Procesy & Subprocess Reaper (`terminal.rs`):**
  - [x] Slot underflow immunity (`fetch_update` + `saturating_sub` w `ACTIVE_TERMINALS`).
  - [x] Zero-idle CPU wakeups w reaperze (blokowanie na kanale `rx.recv()` zamiast gorącego sleepa).
  - [x] Level-triggered ALPM change detection (`pacman_db_mtime`, `summary_mtime` w `sentinels.rs`).
  - [x] 3 nowe testy jednostkowe w `crates/syspilot-core/src/tests.rs` (łącznie 36/36 testów w workspace).
  - [x] Certyfikacja SRE: zatwierdzono w `BP-ARGON-SPRINT-01-TERMINAL-001` (PATCH-019 / PATCH-020).
- [ ] **Sprint 2: Narzędzia Zewnętrzne & Odporność Sieci (`updater.rs`, `sentinels.rs`, `firmware.rs`, `orphans.rs`):**
  - [ ] Wprowadzenie twardych barier timeoutów dla `checkupdates`, `yay`, `arch-audit`, `fwupdmgr`.
  - [ ] Graceful fallback i tryb offline przy awarii mirrora lub DNS (zero zawieszeń UI/demona).
  - [ ] Testy jednostkowe timeoutów i obsługa symulowanego braku sieci.
- [ ] **Sprint 3: Telemetria & Trwałość Stanu (`models.rs`, `triage.rs`):**
  - [ ] Fuzzing parsera telemetrycznego z użyciem `proptest` (odporność na ucięte/zniekształcone JSON-y).
  - [ ] Atomowy zapis i obsługa anomalii zegara systemowego (clock skew).
- [ ] **Sprint 4: Demon Zasobnika 24/7 & D-Bus (`syspilot-tray`):**
  - [ ] Weryfikacja wycieków deskryptorów inotify w wielotygodniowym uptime.
  - [ ] Odporność na restart demona D-Bus / ponowne łączenie z GameMode.
  - [ ] Czysty shutdown (`SIGTERM`).
- [ ] **Sprint 5: Interfejs Slint GUI & Pętla Zdarzeń (`syspilot-gui`):**
  - [ ] Weryfikacja pokrewieństwa wątków (thread-affinity) we wszystkich callbackach UI.
  - [ ] Rozszerzenie testów bezgłowych (`watcher_smoke.rs`) o symulację kliknięć i blokadę podwójnego kliknięcia Copilota.
- [ ] **Sprint 6: Integracja CLI & Systemd Packaging (`bin/syspilot`, `packaging/PKGBUILD`):**
  - [ ] Asercje standardu POSIX dla przekazywania argumentów CLI.
  - [ ] Weryfikacja czystości pakietowania i integracji z XDG autostart.

---

## 4. Procedura Awaryjnego Wznowienia Projektu (Quick Resume Runbook)

W przypadku restartu sesji, zaniku zasilania lub powrotu do projektu w nowym oknie:
1. Odczytaj ten plik: `Projects/sysPilot/ROADMAP_STATUS.md`.
2. Sprawdź bieżący stan repozytorium: `git status` oraz `git diff`.
3. Sprawdź, na którym punkcie w Sekcji 2 zatrzymały się prace (pole z `[ ]`).
4. Uruchom testy bazowe: `cd Projects/sysPilot/rust-tray && cargo test`.
5. Kontynuuj implementację od bieżącej fazy.
