# SysPilot Rust Tray & Desktop Integration: Observed Issues & Feedback

Ten dokument rejestruje błędy, uwagi operacyjne i propozycje usprawnień zaobserwowane podczas testów `syspilot-tray` (Rust SNI) oraz integracji z ekosystemem SysPilot.

---

## Rejestr Zgłoszeń

### [ISSUE-001] "Ask AI Copilot" uruchamia na sztywno predefiniowanego agenta zamiast agenta skonfigurowanego przez użytkownika
* **Data zgłoszenia:** 2026-10-18
* **Zgłaszający:** Karol (Product Owner & Workstation Engineer)
* **Status:** `[LOGGED / IN TESTING]` — *Wstrzymano wdrożenie zmian do zakończenia pełnego cyklu testów lokalnych.*
* **Komponenty dotknięte:**
  - `rust-tray/src/tray.rs` (pozycja menu `"Ask Goose SRE Copilot"` -> wywołanie `--copilot-shell`)
  - `gui/syspilot_gui.py` (przycisk `[ 🤖 ASK AI COPILOT ]` oraz zakładka AI Copilot)
  - `bin/syspilot` (router CLI, funkcja `copilot()`)
* **Priorytet:** MEDIUM / HIGH (User Experience & Personalizacja AI)

#### Opis problemu (Observed Behavior)
Podczas testowania wprowadzonych zmian po kliknięciu przycisku **"Ask AI copilot"** (zarówno w Dashboardzie GUI, jak i w menu zasobnika Rust Tray):
1. Uruchamiany jest predefiniowany szablon agenta (domyślny ogólny przepis `copilot-admin.yaml` lub `recipe.yaml`),
2. Użytkownik nie ma możliwości wyboru ani wskazania własnego, dedykowanego agenta (np. prywatnego agenta wyspecjalizowanego pod konkretną stację roboczą, sprzęt, jądro, sieć i playbooki).

#### Uzasadnienie (Business & Engineering Rationale)
- Predefiniowany, publiczny agent (`copilot-admin`) jest dobrym rozwiązaniem domyślnym (*out-of-the-box*) dla nowych użytkowników lub instalacji open-source, którzy nie posiadają jeszcze skonfigurowanego własnego agenta.
- Zaawansowani użytkownicy i inżynierowie posiadający dedykowane agenty potrzebują, aby kliknięcie przycisku uruchamiało ich spersonalizowanego copilot-a z pełnym kontekstem środowiska roboczego.
- Użytkownicy powinni mieć możliwość wyboru / konfiguracji docelowego agenta.

#### Wdrożone Rozwiązanie Architektoniczne (PATCH-010)
1. **Konfiguracja w `~/.config/syspilot/config.env`:**
   - Parametry wskazujące komendę agenta lub recepturę (`SYSPILOT_COPILOT_CMD`, `SYSPILOT_GOOSE_RECIPE`, `COPILOT_LABEL`).
2. **Kaskada detekcji z fallbackiem:**
   - Jeśli zdefiniowano `SYSPILOT_COPILOT_CMD` -> uruchom skonfigurowanego agenta w terminalu.
   - W przeciwnym razie -> uruchom predefiniowanego publicznego `copilot-admin`.
3. **Selektor w GUI / menu Traya:**
   - Rozwijana lista (dropdown) w ustawieniach Copilota z wykrytymi skillami / agentami Goose (`goose skills list`).
   - Opcjonalne podmenu w trayu, jeśli użytkownik skonfigurował więcej niż jednego agenta.
