# NVIDIA GTX 970 (Maxwell GM204) & DKMS Quirks

## 1. Hardware & Driver Architecture
* **GPU:** MSI NVIDIA GeForce GTX 970 (4GB VRAM, GM204, architektura Maxwell).
* **Oficjalna gałąź sterownika:** `nvidia-580xx-dkms` (wraz z `nvidia-580xx-utils`, `nvidia-580xx-settings`, `lib32-nvidia-580xx-utils`).
* **Kernel driver in use:** `nvidia` (moduły: `nvidia`, `nvidia_modeset`, `nvidia_uvm`, `nvidia_drm`).

---

## 2. Krytyczne Zależności i Pułapki Sprzętowe

### A. Sterowniki Zakazane dla Maxwell:
* **`nvidia-open` / moduły open-source:** Architektura Maxwell (GM204) **nie posiada firmware GSP** wymaganego przez moduły open-source NVIDII (wsparcie zaczyna się od Turinga: RTX 20xx / GTX 16xx). Próba instalacji `nvidia-open` unieruchomi środowisko graficzne.
* **Standardowy pakiet `nvidia`:** Pakiet `nvidia` w oficjalnym repozytorium Arch Linux regularnie przechodzi na nowsze gałęzie upstreamu. Dla architektury Maxwell jedyną wspieraną gałęzią jest seria dedykowana legacy (`580xx` / dkms).

### B. Ograniczenia Nouveau (Sterownik Otwarty):
* Moduł `nouveau` **nie posiada mechanizmu dynamicznego przetaktowywania (reclocking)** dla Maxwell GM204 bez podpisanego firmware'u. Karta na `nouveau` działa na minimalnym taktowaniu bazowym (ok. 135 MHz), dając ~10% nominalnej wydajności 3D i brak obsługi współczesnych gier Steam/Proton.
* W konfiguracji dracut (`/etc/dracut.conf.d/disable-nouveau.conf`) moduł nouveau jest jawnie wykluczany (`omit_drivers+=" nouveau "`).

### C. Sesja Wyświetlania: X11 vs Wayland:
* **Rekomendacja:** **X11 (Xorg)** jest podstawowym, stabilnym środowiskiem dla tej stacji.
* **Wayland na Maxwell (580xx):** Mimo wsparcia dla explicit sync, starsze karty Maxwell na sterownikach własnościowych pod Waylandem nadal wykazują sporadyczny stuttering, problemy z wybudzaniem monitora po uśpieniu (DPMS) oraz anomalie z synchronizacją klatek w grach fullscreen.

### D. Zjawisko "Fliplock Jitter" w Xorg:
* W logu `/var/log/Xorg.0.log` mogą pojawiać się komunikaty `Failed to request fliplock`.
* **Interpretacja:** Wystąpienia te towarzyszą przełączaniu trybów wyświetlania (`Setting mode ... ForceCompositionPipeline=On`) oraz wybudzaniu monitora ze stanu uśpienia DPMS. Nie stanowi to usterki sprzętowej ani uszkodzenia GPU.
* **Obsługa w `sys-health`:**
  - Standardowe zdarzenia (związane z DPMS/handshake) są logowane **bezdźwięcznie (silent log note)** do pliku `system-health.log`.
  - W tabeli TUI wyświetla się czysty stan **`PASS ✔`**.
  - Dynamiczny próg alarmowy uwzględnia czas sesji (`30 + 10 * godziny_uptime`), alarmując `WARN ⚠` wyłącznie w razie rzeczywistego, patologicznego zapętlenia buforów (setki błędów w pętli).

---

## 3. Rygorystyczny Protokół DKMS
* Ponieważ sterownik budowany jest przez DKMS (`nvidia-580xx-dkms`), każda zmiana jądra (`linux`, `linux-lts`) wymaga weryfikacji stanu modułów:
  ```bash
  dkms status
  # Oczekiwany stan dla każdego jądra:
  # nvidia/580.xx.xx, <wersja-kernela>, x86_64: installed
  ```
* **Rozróżnienie faz:**
  1. *Faza DKMS:* Kompilacja kodu źródłowego modułu do pliku `.ko.zst` w `/usr/lib/modules/<kernel>/extra/`.
  2. *Faza Dracut:* Spakowanie modułów do obrazu initramfs (`/boot/initramfs-*.img`).
  *Błąd na etapie dracuta nie oznacza awarii sterownika NVIDIA i odwrotnie.*

---

## 4. Polecenia Weryfikacyjne (Post-Update Check)
```bash
uname -r
dkms status
nvidia-smi
lsmod | grep -E 'nvidia|nouveau'
lspci -k | grep -A 4 -E 'VGA|3D'
```
