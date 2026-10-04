# Playbook: Forged Alliance Forever (FAF) Setup & Troubleshooting Guide

Procedura instalacji, konfiguracji i rozwiązywania problemów z Forged Alliance Forever (FAF) oraz Supreme Commander: Forged Alliance na EndeavourOS / Arch Linux.

---

## 1. Architektura i Wybór Metody Instalacji
- **NIE używaj pakietu AUR (`downlords-faf-client`):** Pakiet z AUR instaluje tylko binarkę Javy klienta – nie konfiguruje izolowanego prefiksu Wine, kontenera Steam Runtime (`pressure-vessel`), patchy DXVK ani skryptów uruchomieniowych.
- **Rekomendowana metoda społeczności:** Oficjalny zestaw skryptów runnera: [FAForever/faf-linux](https://github.com/FAForever/faf-linux) w katalogu `~/faf-linux`.
- **Wymagania wstępne w systemie:**
  - Zainstalowane Supreme Commander: Forged Alliance na Steamie (AppID: `9420`).
  - Aktywny multilib i 32-bitowy stos graficzny (`sys-health --gaming`).
  - Narzędzia systemowe: `bubblewrap`, `curl`, `jq`, `git`.

---

## 2. Instalacja Krok po Kroku (Clean Install)

### Krok 1: Wymuszenie profilu w Steam
Uruchom raz grę Supreme Commander: Forged Alliance bezpośrednio ze Steama (Proton), wejdź do menu i wyjdź.
Tworzy to plik konfiguracji `Game.prefs` w:
`~/.local/share/Steam/steamapps/compatdata/9420/pfx/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance/Game.prefs`

### Krok 2: Klonowanie i wstępna konfiguracja `faf-linux`
```bash
git clone https://github.com/FAForever/faf-linux ~/faf-linux
cd ~/faf-linux
./setup.sh
```
*Uwaga:* Skrypt automatycznie pobiera Java Temurin 25, Steam Linux Runtime 4, Proton GE oraz DXVK.

### Krok 3: Pierwsze logowanie
Uruchom klienta:
```bash
~/faf-linux/run
```
Zaloguj się na swoje konto FAF przez przeglądarkę, a po udanym logowaniu zamknij klienta.

### Krok 4: Skopiowanie profilu `Game.prefs` do prefiksu FAF
Skopiuj działający profil gry ze Steama do prefiksu `faf-linux`:
```bash
mkdir -p "$HOME/faf-linux/prefix/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance"
cp -r "$HOME/.local/share/Steam/steamapps/compatdata/9420/pfx/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance/Game.prefs" \
      "$HOME/faf-linux/prefix/drive_c/users/steamuser/AppData/Local/Gas Powered Games/Supreme Commander Forged Alliance/"
```

### Krok 5: Zastosowanie ścieżek i krytyczne poprawki
1. Uruchom skrypt konfigurujący ścieżki w kliencie:
   ```bash
   cd ~/faf-linux && ./set-client-paths.sh
   ```
2. **Krytyczna poprawka `relativeGamePaths`:**
   Domyślnie klient potrafi ustawić `relativeGamePaths: true`, co powoduje generowanie uciętej ścieżki `/.steam/...` w `fa_path.lua`. Należy wymusić ścieżki bezwzględne:
   ```bash
   jq '.forgedAlliance.relativeGamePaths = false' ~/.faforever/client.prefs > ~/.faforever/client.prefs.tmp && mv ~/.faforever/client.prefs.tmp ~/.faforever/client.prefs
   ```
3. Upewnij się, że w `~/.faforever/fa_path.lua` zmienna `fa_path` zawiera pełną ścieżkę do gry:
   ```lua
   fa_path = "/home/gbur/.local/share/Steam/steamapps/common/Supreme Commander Forged Alliance"
   ```

### Krok 6: Rejestracja skrótu w KDE Plasma
```bash
cd ~/faf-linux && ./install-shortcut.sh
```

---

## 3. Typowe Błędy i Diagnoza (Troubleshooting)

### A. Gra nie startuje (Exit code 126 w `~/.faforever/logs/client.log`)
- **Przyczyna:** Klient FAF próbuje uruchomić windowsowe `ForgedAlliance.exe` bezpośrednio przez Linuksa zamiast przekazać je do Protona/Wine.
- **Rozwiązanie:** Sprawdź `~/.faforever/client.prefs`. Pole `executableDecorator` musi mieć postać:
  ```json
  "executableDecorator": "\"/home/gbur/faf-linux/launchwrapper\" \"%s\""
  ```
  Jeśli go brak, uruchom ponownie: `cd ~/faf-linux && ./set-client-paths.sh`.

### B. Serwer wymaga nowszej wersji klienta (np. Update z 2026.7.0 do 2026.7.1)
Nie instaluj pakietu z zewnątrz. Zaktualizuj komponent w `faf-linux`:
```bash
cd ~/faf-linux
git pull
./update-component.sh faf-client <wersja>   # np. 2026.7.1
```

### C. Czarny ekran po uruchomieniu gry (NVIDIA Maxwell / GTX 970)
Włącz wirtualny pulpit w prefiksie Wine:
```bash
~/faf-linux/launchwrapper-env wine winecfg
```
W zakładce **Grafika (Graphics)** zaznacz **Emuluj wirtualny pulpit (Emulate a virtual desktop)** i ustaw rozdzielczość Twojego monitora (np. 1920x1080).
