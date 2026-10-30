# Playbook: Gaming & Steam Performance Triage (Windows Migrant Guide)

Procedura diagnostyki i optymalizacji stacji roboczej dla graczy korzystających ze Steam / Proton / Wine na EndeavourOS (szczególnie po migracji z systemu Windows).

---

## 1. Architektura Wspólnego Mianownika dla Gier na Linuksie
Większość gier ze Steama to natywne aplikacje Windows uruchamiane przez warstwę translacji **Valve Proton (Wine + DXVK + VKD3D-Proton)**.
W przeciwieństwie do Windowsa, gdzie instalator DirectX i sterowniki GeForce instalują wszystko automatycznie, na Linuksie wymagana jest spójność 5 kluczowych filarów:

1. **Repozytorium `[multilib]` i biblioteki 32-bitowe:** (większość silników gier wymaga 32-bitowego stosu Vulkan).
2. **Dedykowany sterownik GPU z akceleracją 3D:** (dla NVIDIA: `nvidia-utils` lub odpowiedni dkms; dla AMD: `vulkan-radeon`).
3. **Limity pamięci wirtualnej kernela:** (`vm.max_map_count` oraz `nofile`).
4. **Zarządca procesora (CPU Governor) & GameMode:** unikanie profilu oszczędzania energii w trakcie rozgrywki.
5. **Sesja wyświetlania:** poprawna synchronizacja klatek (Wayland explicit sync lub stabilny X11 w zależności od generacji GPU).

---

## 2. Faza 1: "Gra w ogóle nie startuje" (Crash natychmiast po kliknięciu 'Graj')

### Symptom:
Przycisk w Steam zmienia się na ułamek sekundy na „Uruchamianie”, po czym wraca do stanu „Graj” bez żadnego okna błędu.

### Najczęstsza Przyczyna:
Brak 32-bitowego loadera Vulkana lub 32-bitowych bibliotek sterownika karty graficznej.

### Szybka Diagnoza Telemetryczna:
```bash
sys-health --gaming
# Lub odczyt z JSON:
cat ~/.local/state/system-health/summary.json | jq .gaming
```

### Rozwiązanie:
1. Upewnij się, że w `/etc/pacman.conf` odkomentowana jest sekcja:
   ```ini
   [multilib]
   Include = /etc/pacman.d/mirrorlist
   ```
2. Zainstaluj brakujące 32-bitowe komponenty stosu graficznego odpowiednie dla swojego GPU:
   ```bash
   # NVIDIA (wersja bieżąca lub legacy branch dkms):
   sudo pacman -S --needed lib32-vulkan-icd-loader lib32-nvidia-utils
   # AMD:
   sudo pacman -S --needed lib32-vulkan-icd-loader lib32-vulkan-radeon
   # Intel:
   sudo pacman -S --needed lib32-vulkan-icd-loader lib32-vulkan-intel
   ```
3. Weryfikacja: `vulkaninfo --summary` musi wykrywać dedykowane GPU.

---

## 3. Faza 2: "Gra działa, ale mam 5-15 FPS i potworny lag"

### Symptom:
Gra uruchamia się, ale animacja menu tnie, a w grze klatkarz nie przekracza kilkunastu FPS.

### Najczęstsza Przyczyna:
1. **Działanie na otwartym sterowniku `nouveau`:** Na architekturze Maxwell (GTX 970) nouveau nie posiada podpisanego firmware do przetaktowywania zegarów (reclocking). Karta działa na bazowym taktowaniu rdzenia (~135 MHz zamiast ~1200+ MHz).
2. **Działanie na programowym rendererze CPU (`llvmpipe`):** Vulkan nie wykrywa karty dedykowanej.

### Rozwiązanie:
1. Sprawdź aktywny sterownik:
   ```bash
   lspci -k | grep -A 3 -E 'VGA|3D'
   ```
   *Wymagane:* `Kernel driver in use: nvidia`
2. Jeśli karta działa na `nouveau`, włącz moduły NVIDIA i upewnij się, że nouveau jest zablokowane w `/etc/dracut.conf.d/disable-nouveau.conf`.
3. Sprawdź stan modułów: `dkms status`.

---

## 4. Faza 3: Crashe po 10-30 minutach w nowszych grach (UE5, Hogwarts Legacy)

### Symptom:
Gra startuje i działa płynnie, ale w losowym momencie wyłącza się do pulpitu z błędem alokacji pamięci lub braku zasobów.

### Przyczyna:
Domyślny limit mapowań pamięci w Linuksie (`vm.max_map_count`) jest zbyt niski dla współczesnych gier 64-bitowych z setkami tysięcy tekstur i wątków w Protonie.

### Rozwiązanie:
1. Sprawdź bieżącą wartość:
   ```bash
   cat /proc/sys/vm/max_map_count
   ```
   *Wartość optymalna:* `1048576` (lub wyższa).
2. Jeśli wartość wynosi `65530` lub `262144`, ustaw limit na stałe w `/etc/sysctl.d/80-game-compatibility.conf`:
   ```ini
   vm.max_map_count = 1048576
   ```
3. Zastosuj natychmiast:
   ```bash
   sudo sysctl --system
   ```

---

## 5. Faza 4: Mikro-przycięcia (Stuttering) i Zarządca Częstotliwości CPU

### Profil CPU Governor:
Domyślny profil jądra (`schedutil` lub `powersave`) potrafi zbyt wolno reagować na nagłe skoki obciążenia w grach, powodując frametime spikes.

### Optymalne Rozwiązania:
1. **Opcja A (Automatyczna - GameMode):**
   Zainstaluj narzędzie Feral GameMode:
   ```bash
   sudo pacman -S --needed gamemode lib32-gamemode
   ```
   W opcjach uruchamiania gry na Steamie dodaj:
   ```text
   gamemoderun %command%
   ```
   GameMode automatycznie przestawia procesor na profil `performance` tylko na czas działania gry, a po wyjściu przywraca oszczędzanie energii.

2. **Opcja B (Ręczne wymuszenie profilu performance):**
   ```bash
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   ```

---

## 6. Faza 5: Proton GE (GloriousEggroll) i Opcje Uruchamiania Steama

### Dlaczego Valve Proton czasem nie wystarcza?
Oficjalny Proton ze względu na prawa licencyjne i patenty multimedialne (kodeki wideo w przerywnikach filmowych gier, np. formaty Media Foundation / WMF) nie zawsze może odtworzyć cutscenki. GE-Proton posiada wbudowane brakujące kodeki.

### Zarządzanie GE-Proton:
* Narzędzie GUI zainstalowane w systemie: `protonup-qt` (uruchom z menu programów lub terminala).
* Zainstalowane wersje trafiają do katalogu:
  `~/.local/share/Steam/compatibilitytools.d/`
* Wybór w Steam: *Właściwości gry → Zgodność → Wymuś użycie określonego narzędzia zgodności Steam Play → wybierz GE-Proton*.

---

## 7. Faza 6: Szarpanie Obrazu przy Stałych 60 FPS (Beat-Frequency Jitter & Fliplock Contention w KDE Plasma X11)

### Symptom:
Licznik FPS w grze (np. RVGL, SuperTuxKart, emulatory, gry natywne OpenGL/SDL2) pokazuje stabilne, zablokowane 59–60 FPS, ale w trakcie ruchu kamery/pojazdu obraz wyraźnie szarpie, mikro-przycina i traci płynność.

### Architektura Problemu (Potrójna Synchronizacja):
Na stacji roboczej z GTX 970 i łańcuchem DisplayPort MST (zablokowanym na 60 Hz) działają równolegle:
1. `ForceCompositionPipeline=On` w `/etc/X11/xorg.conf` (wymóg dla eliminacji tearingu na podwójnym 1440p),
2. Kompozytor KDE Plasma (`kwin_x11`),
3. Pętla renderowania i V-Sync samej gry.

### Przyczyna A: Fliplock Contention (KWin + V-Sync gry)
Gdy gra fullscreen renderuje z V-Sync, a kompozytor KDE KWin pozostaje aktywny, obie warstwy rywalizują o bezpośredni bufor ramki (*page flip*). W logu `/var/log/Xorg.0.log` skutkuje to ostrzeżeniem `Failed to request fliplock`, a ramki docierają do ekranu z nieregularnym opóźnieniem (*frame pacing jitter*).

**Rozwiązanie:** Wymuszenie zawieszenia kompozytora KWin dla gry:
* **RVGL:** W `/opt/rvgl/profiles/rvgl.ini` ustaw:
  ```ini
  Compositor = 1
  ```
  *(Aktywuje to przekazywanie atomu X11 `_NET_WM_BYPASS_COMPOSITOR` do KWin).*
* **Gry SDL2 (ogólne):** Uruchamiaj ze zmienną środowiskową:
  ```bash
  SDL_VIDEO_X11_NET_WM_BYPASS_COMPOSITOR=1 %command%
  ```
* **Krótkie obejście ręczne (KDE X11):** Naciśnij **`Alt + Shift + F12`** przed uruchomieniem gry, aby tymczasowo wyłączyć kompozytor KWin (ten sam skrót przywraca go po zakończeniu grania).

---

### Przyczyna B: Beat-Frequency Jitter (Kaganiec CPU bez V-Sync)
Gdy gra ma wyłączony sprzętowy V-Sync, ale ustawiony sztywny programowy limit FPS równy częstotliwości monitora (np. `max_fps="60"` w SuperTuxKart), wewnętrzny timer procesora generuje klatki w niedoskonałym takcie (np. 59.92 Hz lub 60.05 Hz). W zderzeniu z idealnym taktem 16.666 ms `ForceCompositionPipeline` dochodzi do zjawiska dudnienia fazowego: co kilkanaście klatek bufor spóźnia się o ułamek milisekundy i monitor wyświetla powtórzoną ramkę.

**Rozwiązanie (SuperTuxKart):**
W `~/.config/supertuxkart/config-0.10/config.xml`:
1. Włącz sprzętowy V-Sync:
   ```xml
   swap-interval-vsync="1"
   ```
2. Odblokuj kaganiec CPU (podnieś próg do 120 FPS lub wyłącz), by to V-Sync zarządzał tempem renderowania:
   ```xml
   max_fps="120"
   ```

---

## 8. Weryfikacja Końcowa
Po wprowadzeniu jakiejkolwiek optymalizacji potwierdź poprawność środowiska:
```bash
sys-health --gaming
```
Oczekiwany rezultat: **GAMING READINESS: ALL CLEAR ✔**.
