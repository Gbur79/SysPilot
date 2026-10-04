# Playbook: Network Triage & Performance Troubleshooting

Universal diagnostic procedure for LAN/WAN connectivity, gateway latency, DNS resolution, and link-speed degradation on Arch Linux and derivatives.

---

## 1. Safety Guardrails & Diagnostic Hierarchy
- **Zero Local Traffic Shaping Assumption:** Never assume local QoS or bandwidth limits on desktop NICs. Modern traffic shaping (e.g. FQ_CODEL limiters) typically resides on upstream edge routers (such as pfSense).
- **Read State Before Modification:** Interrogate link speeds, operational state, and carrier statistics before altering network manager services (`NetworkManager`, `systemd-networkd`).
- **Dynamic Interface Discovery:** Query runtime routes via `ip route` instead of hardcoding static interface names like `eth0` or `enp5s0`.

---

## 2. Phase 1: Physical Link & NIC Layer (L1/L2)
1. **Identify the active default route interface and link state:**
   ```bash
   IFACE=$(ip -4 route show default | awk '{print $5}' | head -n1)
   echo "Active Interface: $IFACE"
   cat "/sys/class/net/$IFACE/operstate"
   cat "/sys/class/net/$IFACE/speed" 2>/dev/null || ethtool "$IFACE" 2>/dev/null | grep -i speed
   ```
   *Expected speed:* High-speed negotiation (`1000` or `2500` Mbps Full Duplex).  
   *Warning:* A speed of `100` or `10` indicates physical link degradation (damaged Cat5e/6 cable, oxidized RJ45 termination, or switch autonegotiation mismatch).

2. **Inspect interface carrier errors and drops:**
   ```bash
   ip -s link show dev "$IFACE"
   ```
   *Check:* `RX errors`, `TX errors`, `dropped` counters.

---

## 3. Phase 2: Gateway Reachability & Local Latency (L3)
1. **Determine the default gateway IP:**
   ```bash
   GW=$(ip -4 route show default | awk '{print $3}' | head -n1)
   echo "Default Gateway: $GW"
   ```

2. **Measure ICMP round-trip time and jitter:**
   ```bash
   ping -c 5 -i 0.2 "$GW"
   ```
   *Baseline:* Latency should be sub-millisecond (<1.0 ms) across switched wired LAN.  
   *Jitter indicator:* If RTT spikes (>5 ms) or shows packet loss, suspect switch port buffering, duplex mismatch, or bad cabling.

---

## 4. Phase 3: DNS Resolution & Upstream WAN
1. **Test upstream DNS resolution latency:**
   ```bash
   time dig +noall +answer archlinux.org
   ```

2. **Verify active system resolvers:**
   ```bash
   resolvectl status 2>/dev/null || cat /etc/resolv.conf
   ```

3. **Check external ping and traceroute:**
   ```bash
   ping -c 4 1.1.1.1
   ```

---

## 5. Phase 4: Network Service Recovery
If NetworkManager or systemd-resolved becomes unresponsive:
```bash
# Restart NetworkManager
sudo systemctl restart NetworkManager

# Flush local systemd-resolved DNS cache
sudo resolvectl flush-caches 2>/dev/null || true
```
