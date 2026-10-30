# Plan Przebudowy i Decouplingu SysPilot (SRE Roadmap)
**Właściciel Projektu:** Karol (Dublin, Ireland)  
**Nadzór Architektoniczny:** Audytor Sol 5.6 (`eos-sre-auditor`)  
**Data inicjalizacji:** 2026-10-18  
**Status projektu:** ⏸️ **FREEZE NA CZAS TESTÓW (2–3 DNI WERYFIKACJI LOKALNEJ)**  

---

## 1. Aktualny Stan Architektury (Gdzie jesteśmy)

SysPilot został przekształcony w nowoczesną, hybrydową architekturę **Ultra-Lean Daemon + On-Demand Graphical Dashboard + Terminal AI Copilot**:

```text
 ┌─────────────────────────────────────────────────────────────┐
 │                  TRAY SENTINEL (Rust)                       │
 │  • Działa 24/7 w tle w zasobniku systemowym (KDE Plasma)     │
 │  • Zużycie pamięci: ~4.9 - 5.3 MB RSS, 0.0% CPU              │
 │  • Pasywny nasłuch inotify na ~/.local/state/.../summary.json│
 │  • D-Bus Feral GameMode ClientCount (zamrażanie w grach)     │
 │  • Ikony: Tarcza (ALL_CLEAR / WARN / FAIL) lub Gamepad (Gry) │
 └──────────────────────────────┬──────────────────────────────┘
                                │
                 LEWY KLIK LUB  │ (spawn on-demand)
                 MENU TRAYA     ▼
 ┌─────────────────────────────────────────────────────────────┐
 │             SYSPILOT DASHBOARD (PyQt6 On-Demand)            │
 │  • Otwiera się na żądanie w ułamku sekundy                  │
 │  • Single-instance IPC przez QLocalServer/QLocalSocket       │
 │  • Zwięzłe sekcje: System Updates, Third-Party, Health      │
 │  • Wskaźnik Baloo & Journal Rate Limit (ochrona I/O i dysku)│
 │  • Podręczne przyciski akcji (Upgrade, Prune Orphans)       │
 │  • Przycisk [ 🤖 ASK AI COPILOT ]                           │
 │  • PO ZAMKNIĘCIU [X]: proces natychmiast zwalnia 100% RAM!  │
 └──────────────────────────────┬──────────────────────────────┘
                                │
                                ▼
 ┌─────────────────────────────────────────────────────────────┐
 │       CANONICAL ENGINE & TELEMETRY (sys-health v2.48)       │
 │  • Single source of truth w Projects/eos-cleaner/sys-health │
 │  • Generuje zwięzłą telemetrię summary.json (Schema 2.0)    │
 └─────────────────────────────────────────────────────────────┘
```

---

## 2. Status Faz i Harmonogram

### 🟢 FAZA 1: Decoupling i Usunięcie Dublowania Silnika (ZAKOŃCZONE ✔)
- [x] **Audyt dywergencji kodu:** Potwierdzono, że baza v2.47/v2.48 jest w 100% zsynchronizowana.
- [x] **Usunięcie duplikatu silnika:** Usunięto 485 KB stary plik `Projects/sysPilot/bin/sys-health.sh`.
- [x] **Wdrożenie cienkiego routera CLI (`bin/syspilot`):** 
  - Dynamiczne wykrywanie i uruchamianie kanonicznego `sys-health` z PATH / systemd.
  - Szybki odczyt cache (`-s`, `--status`, `-j`) z `~/.local/state/system-health/summary.json`.
  - Przekazywanie poleceń diagnostycznych i naprawczych do silnika.
  - Opcja `--gui` uruchamiająca dashboard graficzny.
  - Opcja `--copilot` oraz `--copilot-shell` ze wstępnie wstrzykiwaną telemetrią (0 zbędnych tokenów).

### 🟢 FAZA 2: Ultra-Lekki Sentinel i Tray w Rust (`syspilot-tray`) (ZAKOŃCZONE ✔)
- [x] **Natywny StatusNotifierItem (KDE Plasma Wayland):** Zaimplementowano w `rust-tray` z biblioteką `ksni`.
- [x] **Event-driven (Zero Polling):** Monitorowanie `summary.json` przez `notify` (inotify) oraz D-Bus Feral GameMode.
- [x] **Pomiary zużycia:** RAM spadł z 255 MB do **4.9 MB RSS**, 0.0% CPU w spoczynku.
- [x] **Narzędzie deweloperskie:** Stworzono `dev-tools/build-and-install.sh` (clippy, testy, release build, instalacja do `~/.local/bin/syspilot-tray`).

### 🟢 FAZA 2.5: Reconciliacja UX Dashboardu i On-Demand GUI (ZAKOŃCZONE ✔)
- [x] **Zastąpienie terminalowego popupu graficznym Dashboardem:** Lewy klik w tray otwiera dopracowane okno GUI zamiast surowego Konsole z 7 linijkami tekstu.
- [x] **Single-Instance IPC:** Obsługa podnoszenia okna (raise/activate) przy ponownym kliknięciu ikony.
- [x] **Skrócone i czytelne nazwy sekcji:** System Updates, Third-Party Updates, System Health & Hygiene.
- [x] **Integracja nowego testu PATCH-041:** Monitorowanie stanu Baloo LMDB oraz journal rate-limiting w czasie rzeczywistym.
- [x] **Cykl życia pamięci:** Zamknięcie okna `[X]` całkowicie kończy proces Pythona i Qt.

### 🟡 FAZA 3: Publiczna Segregacja AI i Szablony Open-Source (W TRAKCIE)
- [x] **Prywatność i segregacja (Sanitization):** Całkowicie wyczyszczono publiczny kod repozytorium z nazw prywatnych agentów oraz specyfikacji stacji deweloperskiej.
- [x] **Prywatny lokalny override:** Utworzono `~/.config/syspilot/config.env` (uprawnienia 0600, ignorowany przez git), w którym zdefiniowano lokalną ścieżkę do prywatnej receptury użytkownika.
- [x] **Publiczny szablon Goose w `packaging/goose/`:**
  - `copilot-admin.yaml`: Zabezpieczona receptura (tryb `approve`, limit 15 tur, wyświetlanie kosztu, bez zbędnych wywołań).
  - `copilot-admin/SKILL.md`: Uniwersalny skill SRE uczący model dynamicznego wykrywania sprzętu, czytania `summary.json` (0 tokenów) oraz przestrzegania zasady Reversibility First.
- [x] **Zaktualizowano `packaging/PKGBUILD`:** Przygotowano instalację publicznych reguł do `/usr/share/syspilot/goose/`.
- [ ] **Testy długodystansowe (2-3 dni):** Sprawdzenie stabilności na żywym systemie.

---

## 3. Rejestr Napotkanych Błędów i Rozwiązania (Post-Mortem)

| Identyfikator | Problem / Objaw | Przyczyna źródłowa | Zastosowane rozwiązanie |
|---|---|---|---|
| **ERR-01** | `syspilot: command not found` przy akcjach menu traya | Brak binarki `syspilot` w globalnym PATH środowiska graficznego. | Dodano funkcję `resolve_syspilot()` w `src/terminal.rs` (sprawdzanie PATH, `~/.local/bin` oraz katalogu rodzeństwa) oraz utworzono symlink `~/.local/bin/syspilot`. |
| **ERR-02** | Lewy klik w ikonę traya otwierał surowy terminal z 7 linijkami tekstu (`SysPilot_left_click.png`) | W PoC Fazy 2 podpięto `activate()` pod `syspilot -s` w terminalu zamiast pod dashboard graficzny. | Wdrożono architekturę On-Demand GUI: `activate()` wywołuje `spawn_detached_gui()`, które uruchamia dashboard PyQt6. Okno zamyka się krzyżykiem `[X]` i zwalnia 100% RAM-u. |
| **ERR-03** | Lewy klik oraz menu kontekstowe „Open Dashboard” nie reagowały (`Exit code: 1`) | W `bin/syspilot` ścieżka `SCRIPT_DIR` była pobierana przez `dirname "${BASH_SOURCE[0]}"`. Przy uruchomieniu przez symlink w `~/.local/bin/syspilot` skrypt szukał GUI w `~/.local/gui/syspilot_gui.py` zamiast w repozytorium. | Użyto `readlink -f -- "${BASH_SOURCE[0]}"` w `bin/syspilot`, co poprawnie rozwiązuje ścieżkę bez względu na symlinki. |
| **ERR-04** | Sekcja **THIRD-PARTY UPDATES** stale pokazywała `TRIAGE PENDING ℹ` (`Third Party Updates.png`) | Funkcja `check_standalone_software()` w `core/triage.py` miała zahardkodowaną ścieżkę do usuniętego `sysPilot/bin/sys-health.sh`. Gdy plik usunięto, funkcja po cichu zwracała pusty stan `checked: False`. | Zaktualizowano `core/triage.py`, aby dynamicznie lokalizowała kanoniczny silnik systemowy `sys-health` (`shutil.which` oraz `~/.local/bin/sys-health`). Dane natychmiast wskoczyły na `UP TO DATE ✔`. |
| **ERR-05** | Ryzyko wycieku prywatnej konfiguracji na GitHub | Nazwa prywatnego skilla i specyfika sprzętowa były bezpośrednio używane w repozytorium `sysPilot`. | 1) Usunięto odniesienia do prywatnego skilla z publicznego kodu.<br>2) Wprowadzono plik `~/.config/syspilot/config.env` jako lokalny override.<br>3) Utworzono czysty, publiczny szablon `copilot-admin` w `packaging/goose/`. |

---

## 4. Wytyczne do Testów (Najbliższe 2–3 dni)

Podczas codziennego użytkowania stacji roboczej (praca, multimedia, granie w FAF/Steam) weryfikujemy:
1. **Stabilność traya w tle:** Czy ikona `syspilot-tray` nie znika z zasobnika KDE, a zużycie RAM-u utrzymuje się w okolicach ~5 MB.
2. **Reakcję na GameMode:** Czy podczas włączenia gry (Steam/FAF) ikona zmienia się na gamepad (`input-gaming`), a diagnostyka jest zamrażana.
3. **Ergonomię Dashboardu:** Czy po lewym kliknięciu okno otwiera się szybko i czytelnie prezentuje statusy.
4. **Zwalnianie pamięci:** Czy po kliknięciu `[X]` proces Pythona całkowicie znika z `ps aux | grep syspilot_gui`.
5. **Spójność z `sys-health`:** Czy dane w Dashboardzie (pakiety, sieroty, Baloo) pokrywają się z ręcznym wywołaniem `sys-health --audit`.

Po zakończeniu fazy testów przejdziemy do finalnego szlifu i przygotowania repozytorium pod publikację Open-Source.

---

## 5. Rejestr Uwag i Zaobserwowanych Problemów z Testów (Observed Issues)

| Identyfikator | Data | Zgłaszający | Komponent | Status | Opis zgłoszenia |
|---|---|---|---|---|---|
| **ISSUE-001** | 2026-10-18 | Karol | `rust-tray`, `syspilot_gui.py`, `bin/syspilot` | `[RESOLVED in PATCH-010]` | **Brak wyboru agenta przy "Ask AI Copilot":** Przycisk uruchamia na sztywno predefiniowany ogólny szablon agenta (`copilot-admin`). Rozwiązano przez wprowadzenie resolvera `~/.config/syspilot/config.env` (`SYSPILOT_COPILOT_CMD`, `COPILOT_LABEL`). |

