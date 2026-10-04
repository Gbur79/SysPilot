# Playbook: Gaming & Steam Performance Triage (Windows Migrant Guide)

Procedura diagnostyki i optymalizacji stacji roboczej dla graczy korzystających ze Steam / Proton / Wine na EndeavourOS (szczególnie po migracji z systemu Windows).

---

## 1. Architektura Wspólnego Mianownika dla Gier na Linuksie
Większość gier ze Steama to natywne aplikacje Windows uruchamiane przez warstwę translacji **Valve Proton (Wine + DXVK + VKD3D-Proton)**.
W przeciwieństwie do Windowsa, gdzie instalator DirectX i sterowniki GeForce instalują wszystko automatycznie, na Linuksie wymagana jest spójność 5 kluczowych filarów:

1. **Repozytorium `[multilib]` i biblioteki 32-bitowe:** (większość silników gier wymaga 32-bitowego stosu Vulkan).
2. **Dedykowany sterownik GPU z akceleracją 3D:** (dla GTX 970: seria własnościowa `580xx-dkms`).
3. **Limity pamięci wirtualnej kernela:** (`vm.max_map_count` oraz `nofile`).
4. **Zarządca procesora (CPU Governor) & GameMode:** unikanie profilu oszczędzania energii w trakcie rozgrywki.
5. **Sesja wyświetlania:** X11 zamiast Wayland dla architektury Maxwell (GTX 970).

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

### Rozwiązanie (dla NVIDIA GTX 970 / Maxwell):
1. Upewnij się, że w `/etc/pacman.conf` odkomentowana jest sekcja:
   ```ini
   [multilib]
   Include = /etc/pacman.d/mirrorlist
   ```
2. Zainstaluj brakujące 32-bitowe komponenty stosu graficznego:
   ```bash
   sudo pacman -S --needed lib32-vulkan-icd-loader lib32-nvidia-580xx-utils
   ```
3. Weryfikacja: `vulkaninfo --summary` musi wykrywać urządzenie GPU0: NVIDIA GeForce GTX 970.

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

## 7. Weryfikacja Końcowa
Po wprowadzeniu jakiejkolwiek optymalizacji potwierdź poprawność środowiska:
```bash
sys-health --gaming
```
Oczekiwany rezultat: **GAMING READINESS: ALL CLEAR ✔**.
