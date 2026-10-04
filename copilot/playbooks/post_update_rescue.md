# Playbook: Post-Update Rescue & Recovery

Procedura ratunkowa krok-po-kroku w przypadku awarii po aktualizacji systemu (`pacman -Syu` / `yay`): błędy transakcji, czarny ekran, desynchronizacja jądra, uszkodzone pakiety lub pętla logowania.

---

## Faza 1: Szybki Triage & Diagnoza Telemetryczna
Przed wykonaniem inwazyjnych poleceń odczytaj stan telemetrii:
```bash
# 1. Odczyt skrótu JSON z sys-health
cat ~/.local/state/system-health/summary.json 2>/dev/null || sys-health --json

# 2. Sprawdzenie ostatnich wpisów z pacmana
tail -n 40 /var/log/pacman.log
```
*Zwróć uwagę na flagi:* `kernel`, `initramfs`, `dkms`, `gpu_runtime`, `pacman_lock`, `package_integrity`.

---

## Faza 2: Przerwana Transakcja i Osierocona Blokada Pacmana
Jeśli aktualizacja została przerwana (brak prądu, zawieszenie Xorg/Wayland):
1. **Weryfikacja procesów:**
   ```bash
   pgrep -l "pacman|yay"
   ```
2. **Usunięcie blokady (wyłącznie gdy brak aktywnych procesów):**
   ```bash
   sudo rm -f /var/lib/pacman/db.lck
   ```
3. **Wznowienie i dokończenie transakcji:**
   ```bash
   sudo pacman -Syu
   ```

---

## Faza 3: Błędy Kluczy PGP / Keyring (`invalid or corrupted package`)
Gdy aktualizacja zatrzymuje się na błędzie walidacji podpisów:
1. **Zaktualizuj wyłącznie pakiety z bazą kluczy:**
   ```bash
   sudo pacman -Sy archlinux-keyring endeavouros-keyring
   ```
2. **Następnie wykonaj pełną aktualizację:**
   ```bash
   sudo pacman -Su
   ```
*(Nigdy nie wyłączaj weryfikacji kluczy `SigLevel = Never` w `/etc/pacman.conf`).*

---

## Faza 4: Desynchronizacja Kernela, Modułów i DKMS (Czarny Ekran)
Najczęstsza przyczyna czarnego ekranu na GTX 970 to rozbieżność między uruchomionym jądrem a modułami NVIDIA.
1. **Sprawdź wersje jądra i modułów:**
   ```bash
   uname -r
   ls -ld /usr/lib/modules/*
   dkms status
   ```
2. **Weryfikacja obecności nagłówków jądra:**
   Upewnij się, że zainstalowane są nagłówki odpowiadające zainstalowanym jądrom:
   - dla `linux` → `linux-headers`
   - dla `linux-lts` → `linux-lts-headers`
3. **Ręczne przebudowanie modułu NVIDIA w DKMS (jeśli status nie jest 'installed'):**
   ```bash
   sudo dkms install nvidia-580xx/<wersja> -k <wersja-kernela>
   ```
4. **Regeneracja initramfs dla danego jądra:**
   ```bash
   sudo dracut --kver <wersja-kernela> --force
   ```

---

## Faza 5: Rollback Pakietu z Lokalnego Cache (`pacman -U`)
Jeśli konkretna nowa wersja pakietu (np. sterownik, biblioteka glibc, kwin) powoduje regresję:
1. **Znajdź poprzednią wersję w cache:**
   ```bash
   ls -lt /var/cache/pacman/pkg/<nazwa-pakietu>*
   ```
2. **Zainstaluj wersję poprzednią:**
   ```bash
   sudo pacman -U /var/cache/pacman/pkg/<nazwa-pakietu>-<stara-wersja>.pkg.tar.zst
   ```

---

## Faza 6: Weryfikacja Końcowa
Potwierdź usunięcie usterki za pomocą audytu:
```bash
sys-health --audit
```
Oczekiwany kod wyjścia: `0` (ALL_CLEAR) lub `2` (REVIEW_WARNINGS bez błędów krytycznych).
