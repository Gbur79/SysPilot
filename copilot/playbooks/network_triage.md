# Playbook: Network Triage & Performance Troubleshooting

Procedura diagnozy problemów z siecią LAN, łączem światłowodowym, opóźnieniami oraz degradacją przepustowości.

---

## 1. Kontekst Topologiczny i Sprzętowy
* **Karta w PC:** Supermicro AOC-X550 STGS-i1T (sterownik `ixgbe`).
* **Negocjacja linku:** Powinna wynosić **2500 Mb/s (2.5 Gbps)** Full Duplex.
* **Ścieżka kablowa:** PC (`192.168.1.50`) → TP-Link TL-SG108-M2 → Netgear MS108UP → pfSense (`192.168.1.1`).
* **Brama domyślna:** pfSense Plus / CE (`192.168.1.1`).
* **Dostawca WAN:** Digiweb FTTH (PPPoE VLAN 10, 1 Gbps Down / 100 Mbps Up).
* **Zasada QoS:** Traffic shaping (limitery FQ_CODEL) działa wyłącznie na routerze pfSense (900M / 90M). Nigdy nie konfiguruj QoS ani limitów na karcie w PC!

---

## 2. Faza 1: Warstwa Fizyczna i Stan Karty Sieciowej (L1/L2)
1. **Zidentyfikuj aktywny interfejs i stan linku:**
   ```bash
   IFACE=$(ip -4 route show default | awk '{print $5}')
   cat /sys/class/net/$IFACE/operstate
   cat /sys/class/net/$IFACE/speed
   ```
   *Oczekiwana prędkość:* `2500` (lub co najmniej `1000`).  
   *Alarm:* Jeśli prędkość wynosi `100` lub `10`, doszło do degradacji negocjacji łącza (uszkodzony kabel kat. 5e/6, zabrudzony port lub błąd autonegocjacji switcha TP-Link).
2. **Sprawdź liczniki błędów sprzętowych karty:**
   ```bash
   cat /sys/class/net/$IFACE/statistics/rx_crc_errors
   cat /sys/class/net/$IFACE/statistics/rx_errors
   cat /sys/class/net/$IFACE/statistics/tx_errors
   ```
   *Wartości powyżej 0 oznaczają zakłócenia elektryczne na kablu lub wadliwy wtyk RJ-45.*

---

## 3. Faza 2: Łączność z Bramą pfSense i Opóźnienia (L3)
1. **Sprawdź trasę domyślną:**
   ```bash
   ip route show default
   # Oczekiwane: default via 192.168.1.1 dev <IFACE> proto ...
   ```
2. **Pomiar pingu do routera:**
   ```bash
   ping -c 5 192.168.1.1
   # Oczekiwany RTT: < 0.4 ms (stabilne, bez packet loss)
   ```

---

## 4. Faza 3: NetworkManager Quirk "Metered Connection"
Jeśli przepustowość w grach lub pobieraniu drastycznie spadła po aktualizacji systemu:
1. **Sprawdź status taryfowania:**
   ```bash
   nmcli -t -f GENERAL.DEVICE,GENERAL.CONNECTION,GENERAL.METERED dev show
   ```
2. **Wyłącz flagę taryfowania (jeśli wskazuje `yes` lub `guess`):**
   ```bash
   CONN_NAME=$(nmcli -t -f GENERAL.CONNECTION dev show "$IFACE" | head -n 1)
   sudo nmcli connection modify "$CONN_NAME" connection.metered no
   sudo nmcli connection up "$CONN_NAME"
   ```

---

## 5. Faza 4: Diagnostyka DNS i Osierocone Wpisy VPN
1. **Sprawdź konfigurację resolvera:**
   ```bash
   cat /etc/resolv.conf
   ```
2. **Wykrywanie wycieku / osieroconego DNS:**
   - Adres resolvera powinien wskazywać na `192.168.1.1` (pfSense).
   - Jeśli widnieje adres z puli VPN (np. `10.128.0.1`), doszło do zablokowania DNS po awaryjnym zamknięciu tunelu VPN.
3. **Restart usług sieciowych w celu odświeżenia:**
   ```bash
   sudo systemctl restart NetworkManager
   ```

---

## 6. Faza 5: Weryfikacja przez `sys-health`
Uruchom dedykowany audyt sekcji sieciowej:
```bash
sys-health --audit
```
Upewnij się, że flaga `network` i `dns` nie pojawiają się w `summary.json`.
