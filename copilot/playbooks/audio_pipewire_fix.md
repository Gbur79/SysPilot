# Playbook: Audio, PipeWire & WirePlumber Fix

Procedura diagnostyki i naprawy dźwięku w środowisku KDE Plasma (brak dźwięku, zacięcia, niewykrywanie wyjść audio lub zawieszenie serwera dźwięku).

---

## 1. Złota Zasada Audio: Sesja Użytkownika vs Root
> [!IMPORTANT]
> Na współczesnym EndeavourOS / Arch Linux stack audio (**PipeWire**, **WirePlumber**, **pipewire-pulse**) działa jako usługa **sesji użytkownika (`systemd --user`)**, a NIE jako daemon systemowy roota!  
> **Nigdy nie wykonuj:** `sudo systemctl restart pipewire` (to nie zadziała i może zablokować uprawnienia socketów).

---

## 2. Faza 1: Inspekcja Stanu Usług Audio
Sprawdź status jednostek w sesji bieżącego użytkownika:
```bash
systemctl --user status pipewire wireplumber pipewire-pulse --no-pager
```
Sprawdź, czy któraś z usług nie weszła w stan awarii (`failed`):
```bash
systemctl --user --failed
```

---

## 3. Faza 2: Weryfikacja Wyjść Audio i WirePlumber (`wpctl`)
Narzędzie `wpctl` służy do bezpośredniego podglądu urządzeń i routingu w WirePlumber:
1. **Wylistuj dostępne urządzenia i profile (Sinks / Sources):**
   ```bash
   wpctl status
   ```
2. **Sprawdź domyślne urządzenie wyjściowe (oznaczone gwiazdką `*` w sekcji Sinks):**
   - Upewnij się, że dźwięk nie został skierowany na niewłaściwe wyjście (np. wyjście monitora HDMI karty NVIDIA zamiast karty dźwiękowej płyty głównej lub słuchawek USB).
3. **Sprawdź głośność i wyciszenie (mute):**
   ```bash
   wpctl get-volume @DEFAULT_AUDIO_SINK@
   # Jeśli widnieje [MUTED], odcisz:
   wpctl set-mute @DEFAULT_AUDIO_SINK@ 0
   ```

---

## 4. Faza 3: Deterministyczny Restart Stacku PipeWire
W przypadku zawieszenia buforów dźwięku lub braku wykrywania nowo podłączonego urządzenia:
1. **Restart jednostek użytkownika:**
   ```bash
   systemctl --user restart pipewire pipewire-pulse wireplumber
   ```
2. **Weryfikacja logów pod kątem błędów ALSA / zablokowanych urządzeń:**
   ```bash
   journalctl --user -u pipewire -u wireplumber -n 30 --no-pager
   ```

---

## 5. Faza 4: Weryfikacja Działania Dźwięku
Odtwórz testowy sygnał dźwiękowy w terminalu:
```bash
pw-play /usr/share/sounds/freedesktop/stereo/complete.oga 2>/dev/null || aplay /usr/share/sounds/alsa/Front_Center.wav 2>/dev/null
```
