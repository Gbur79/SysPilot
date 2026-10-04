# EndeavourOS / Arch System & Environmental Quirks

Ten dokument opisuje specyficzne anomalie, pułapki konfiguracyjne i cechy środowiska stacji roboczej Karola (Dublin, Ireland).

---

## 1. Zarządzanie Mirrorami & Sieć Geograficzna
* **Rekomendowane narzędzie:** Używaj `sys-health --mirrors`. Posiada wbudowany Atomic Staging Gate (generuje listę w `/tmp`, testuje min. 3 serwery, bada na żywo HTTP 200/TTFB na `core.db` i tworzy kopię `.bak`).
* **Brak mirrorów w Irlandii:** Oficjalna baza Arch Linux **nie zawiera serwerów z kodem kraju `IE`**. Próba wywołania `reflector --country IE` zwróci błąd pustej listy.
* **Optymalny routing sieciowy:** Z racji połączenia światłowodowego Digiweb FTTH przez węzeł INEX, najniższe opóźnienia oferują serwery w Wielkiej Brytanii, Holandii, Francji i Niemczech. Standardowy uniwersalny mechanizm `sys-health --mirrors` automatycznie testuje 20 najświeższych globalnych mirrorów HTTPS i wybiera 10 najszybszych.
* **Zakaz ręcznego `rate-mirrors`:** Pakiet `rate-mirrors` **nie jest preinstalowany** w systemie. Ręczne wywołanie polecenia typu `rate-mirrors arch | sudo tee /etc/pacman.d/mirrorlist` obetnie plik mirrorlist do 0 bajtów i zablokuje pacmana! (Silnik `sys-health --mirrors` sam wykrywa dostępne w systemie programy: `reflector` / `eos-rankmirrors` i chroni pliki przed wyzerowaniem).

---

## 2. Bootloader, EFI i Dracut Machine-ID Quirk
* **Układ partycji rozruchowych:**
  - `/boot` znajduje się na głównej partycji systemowej ext4 (`/dev/nvme0n1p2`).
  - `/boot/efi` to osobno montowana partycja ESP FAT32 (`/dev/nvme0n1p1`).
* **Historyczny błąd ścieżki `machine-id` w dracut:**
  - W przeszłości polecenie `dracut --regenerate-all --force` wywoływało błąd z powodu próby zapisu obrazów jądra do struktury `/boot/efi/<machine-id>/<kver>/`.
  - **Rozwiązanie / zasada:** Nigdy nie twórz sztucznych katalogów w EFI ani nie reinstaluj GRUB-a w reakcji na ten błąd. Zawsze regeneruj initramfs celując w **konkretne jądro**:
    ```bash
    sudo dracut --kver <wersja-jadra> --force
    ```

---

## 3. NetworkManager & Quirk "Metered Connection"
* **Objaw:** Drastyczny spadek prędkości pobierania (throttling), wysokie opóźnienia lub brak synchronizacji po aktualizacji systemu.
* **Przyczyna:** NetworkManager może automatycznie oznaczyć aktywne połączenie ethernetowe/WiFi jako taryfowane (`metered = yes`).
* **Weryfikacja CLI:**
  ```bash
  nmcli -t -f GENERAL.DEVICE,GENERAL.CONNECTION,GENERAL.METERED dev show
  ```
* **Naprawa:**
  ```bash
  sudo nmcli connection modify '<Nazwa-Połączenia>' connection.metered no
  ```

---

## 4. Topologia DNS i Osierocone Adresy VPN
* **Podstawowy resolver:** Router pfSense (`192.168.1.1`) z instancją NextDNS.
* **Osierocony DNS VPN:** Po nagłym lub niespójnym rozłączeniu tunelu VPN (np. AirVPN / WireGuard / OpenVPN) w pliku `/etc/resolv.conf` może pozostać adres wewnętrzny tunelu (np. `10.128.0.1`), uniemożliwiając rezolucję nazw w sieci lokalnej i internecie.
* **Weryfikacja:** `cat /etc/resolv.conf` oraz `resolvectl status`.

---

## 5. Blokada Bazy Pacmana (`db.lck`)
* Plik `/var/lib/pacman/db.lck` zabezpiecza bazę przed jednoczesnym zapisem.
* **Nigdy nie usuwaj pliku w ciemno!** Najpierw upewnij się, że żaden proces pacmana ani menedżera pakietów nie działa w tle:
  ```bash
  pgrep -l "pacman|yay"
  ```
* Usunięcie blokady dopuszczalne jest wyłącznie po twardym zawieszeniu/reboocie stacji, gdy proces nie istnieje.
