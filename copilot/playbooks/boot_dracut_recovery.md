# Playbook: Boot, Dracut & EFI Recovery

Procedura diagnostyki i bezpiecznej naprawy procesu rozruchu, obrazów initramfs oraz konfiguracji EFI/GRUB.

---

## Faza 1: Weryfikacja Punktów Montowania i Miejsca na Dysku
Przed jakąkolwiek ingerencją w rozruch sprawdź układ partycji:
```bash
findmnt /boot
findmnt /boot/efi
df -h /boot /boot/efi
```
*Zasada:*
- `/boot` musi znajdować się na głównym systemie plików (ext4).
- `/boot/efi` musi być zamontowane jako partycja ESP (`vfat`).
- Wolne miejsce na `/boot/efi` nie może być mniejsze niż 30 MB.

---

## Faza 2: Inspekcja Konfiguracji Dracuta
Upewnij się, że pliki konfiguracyjne w `/etc/dracut.conf.d/` nie zostały uszkodzone ani usunięte:
```bash
cat /etc/dracut.conf.d/*.conf
```
*Kluczowe wpisy dla stacji roboczej:*
- `disable-nouveau.conf`: `omit_drivers+=" nouveau "`
- `enable-nvidia.conf`: `force_drivers+=" nvidia nvidia_modeset nvidia_uvm nvidia_drm "`
- `eos-defaults.conf`: kompresja `zstd`, wykluczenie zbędnych modułów sieciowych

---

## Faza 3: Bezpieczna, Celowana Regeneracja Initramfs
Nigdy nie stosuj ślepego `dracut --regenerate-all --force`, jeśli nie masz pewności co do stanu wszystkich zainstalowanych jąder.

### 1. Ustal wersję jądra do naprawy:
```bash
# Dla aktualnie działającego jądra:
KVER=$(uname -r)

# Lub sprawdź zainstalowane kernele w systemie:
ls -1 /usr/lib/modules/
```

### 2. Wygeneruj initramfs bezpośrednio do pliku w `/boot`:
```bash
# Dla jądra LTS:
sudo dracut --kver "$KVER" --force /boot/initramfs-linux-lts.img "$KVER"

# Lub standardowe celowane wywołanie:
sudo dracut --kver "$KVER" --force
```

---

## Faza 4: Weryfikacja Poprawności Obrazu Rozruchowego
Przed wykonaniem restartu **bezwzględnie zweryfikuj** wygenerowany plik:
1. **Sprawdź rozmiar i czas modyfikacji (mtime):**
   ```bash
   ls -lh /boot/initramfs*
   ```
   *(Obraz o rozmiarze kilkuset bajtów lub 0 B oznacza awarię dracuta! Prawidłowy obraz ma zazwyczaj 30–80 MB)*.
2. **Upewnij się, że moduły NVIDIA znalazły się wewnątrz initramfs:**
   ```bash
   lsinitrd /boot/initramfs-linux-lts.img | grep -E 'nvidia\.ko|nvidia_modeset\.ko'
   ```

---

## Faza 5: Weryfikacja Wpisów Bootloadera (GRUB & UEFI NVRAM)
1. **Sprawdź stan wpisów rozruchowych płyty głównej:**
   ```bash
   sudo efibootmgr -v
   ```
2. **Odświeżenie konfiguracji menu GRUB (jeśli dodano nowe jądro):**
   ```bash
   sudo grub-mkconfig -o /boot/grub/grub.cfg
   ```
> [!CAUTION]
> **Nigdy nie wykonuj `grub-install`** jako pierwszego kroku naprawy błędu jądra! Używaj `grub-mkconfig` do regeneracji pliku menu, chyba że wpis NVRAM płyty głównej został faktycznie skasowany.
