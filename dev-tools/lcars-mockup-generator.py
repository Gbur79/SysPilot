"""
SysPilot LCARS UI Mockup Generator (Headless FigCraft CLI)
Complies with Michael Okuda 1987 Standards, SRE Universalism, and Karol's Frameless OLED Deck Specification.
"""

import json
import subprocess
import sys
from pathlib import Path

def generate_lcars_theme(
    mode="yellow",
    bottom_style="segmented_blocks",
    pod_style="frameless_tracks",
    out_dir=Path("/home/gbur/Projects/sysPilot")
):
    out_dir.mkdir(parents=True, exist_ok=True)

    configs = {
        "yellow": {
            "name": "Condition Yellow",
            "frame": "#ff9900",
            "text_frame": "#3d1c00",
            "sub": "#ffaa66",
            "cond_text": "CONDITION YELLOW ⚠",
            "cond_color": "#f59e0b",
            "stat_color": "#f59e0b",
            "prefix": f"syspilot_v9_{pod_style}",
        },
        "green": {
            "name": "Condition Green (Nominal)",
            "frame": "#9999ff",
            "text_frame": "#0d1a38",
            "sub": "#38bdf8",
            "cond_text": "CONDITION GREEN ✔",
            "cond_color": "#22c55e",
            "stat_color": "#f59e0b",
            "prefix": f"syspilot_v9_nominal_{pod_style}",
        },
        "red": {
            "name": "Red Alert",
            "frame": "#ef4444",
            "text_frame": "#380b0b",
            "sub": "#fca5a5",
            "cond_text": "RED ALERT ⚡",
            "cond_color": "#ef4444",
            "stat_color": "#ef4444",
            "prefix": f"syspilot_v9_red_{pod_style}",
        },
    }

    cfg = configs.get(mode, configs["yellow"])
    commands = []
    next_id = 2

    def add_cmd(cmd, **params):
        nonlocal next_id
        cid = next_id
        commands.append({"command": cmd, "params": params})
        next_id += 1
        return cid

    c_frame = cfg["frame"]
    c_text_frame = cfg["text_frame"]
    c_sub = cfg["sub"]
    c_peach = "#ffaa66"
    c_lilac = "#cc99cc"
    c_cyan = "#38bdf8"
    c_green = "#22c55e"
    c_row_bg = "#0e1828"
    c_border = "#1a2538"

    # Canvas: 1040 x 740, pitch black OLED (#000000)
    root = add_cmd(
        "frame.create",
        name=f"SysPilot True Arches {cfg['name']}",
        width=1040,
        height=740,
        x=0,
        y=0,
        props={"fill": "#000000", "layout": None},
    )

    # 1. TOP L-ELBOW (Convex outer arch R=28, Concave fillet arch R=20)
    top_elbow_path = "M 28 0 L 220 0 L 220 44 L 148 44 A 20 20 0 0 0 128 64 L 128 120 L 0 120 L 0 28 A 28 28 0 0 1 28 0 Z"
    add_cmd(
        "node.create",
        parent=root,
        type="vector",
        name="Top Left L-Elbow (ARCH)",
        path=top_elbow_path,
        x=12,
        y=12,
        width=220,
        height=120,
        props={"fill": c_frame},
    )
    add_cmd(
        "node.create",
        parent=root,
        type="text",
        name="Elbow Text",
        text="441-A",
        x=26,
        y=26,
        props={"fill": c_text_frame, "fontSize": 14, "fontFamily": "Antonio"},
    )

    # 2. TOP HEADER ROW (Cutout Title + Trailing Rail)
    title_box = add_cmd(
        "frame.create",
        parent=root,
        name="Title Box (Cutout)",
        x=240,
        y=12,
        width=310,
        height=44,
        props={
            "layout": "horizontal",
            "gap": 10,
            "crossAlign": "center",
            "fill": "none",
            "sizingH": "hug",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=title_box,
        type="text",
        name="Sys Title",
        text="SYS PILOT",
        props={"fill": c_frame, "fontSize": 26, "fontFamily": "Antonio"},
    )
    add_cmd(
        "node.create",
        parent=title_box,
        type="text",
        name="Sys OS",
        text="ENDEAVOUR-OS // 7.2.14-ZEN",
        props={"fill": c_sub, "fontSize": 16, "fontFamily": "Antonio"},
    )

    trailing_top_rail = add_cmd(
        "frame.create",
        parent=root,
        name="Trailing Top Rail",
        x=558,
        y=12,
        width=470,
        height=44,
        props={
            "fill": c_frame,
            "cornerRadius": [0, 22, 22, 0],
            "layout": "horizontal",
            "padding": [0, 16, 0, 16],
            "mainAlign": "spaceBetween",
            "crossAlign": "center",
            "sizingH": "fixed",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=trailing_top_rail,
        type="text",
        name="Stardate",
        text="STARDATE 2026.311",
        props={"fill": c_text_frame, "fontSize": 14, "fontFamily": "Antonio"},
    )

    pill_cond = add_cmd(
        "frame.create",
        parent=trailing_top_rail,
        name="Condition Pill",
        height=26,
        props={
            "fill": "#000000",
            "cornerRadius": 13,
            "layout": "horizontal",
            "padding": [3, 14, 3, 14],
            "crossAlign": "center",
            "sizingH": "hug",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=pill_cond,
        type="text",
        name="CondTxt",
        text=cfg["cond_text"],
        props={"fill": cfg["cond_color"], "fontSize": 12, "fontFamily": "Antonio"},
    )

    # 3. BOTTOM L-ELBOW (Convex outer arch R=28, Concave fillet arch R=20)
    bottom_elbow_path = "M 0 0 L 128 0 L 128 56 A 20 20 0 0 0 148 76 L 220 76 L 220 120 L 28 120 A 28 28 0 0 1 0 92 Z"
    add_cmd(
        "node.create",
        parent=root,
        type="vector",
        name="Bottom Left L-Elbow (ARCH)",
        path=bottom_elbow_path,
        x=12,
        y=608,
        width=220,
        height=120,
        props={"fill": c_frame},
    )
    add_cmd(
        "node.create",
        parent=root,
        type="text",
        name="Bottom Code",
        text="SEC-01",
        x=26,
        y=694,
        props={"fill": c_text_frame, "fontSize": 14, "fontFamily": "Antonio"},
    )

    # 4. BOTTOM ACTION RAIL (Segmented Blocks)
    b1 = add_cmd(
        "frame.create",
        parent=root,
        name="Btn Upgrade",
        x=236,
        y=684,
        width=150,
        height=44,
        props={
            "fill": c_peach,
            "cornerRadius": 0,
            "layout": "horizontal",
            "padding": [0, 10, 0, 10],
            "mainAlign": "center",
            "crossAlign": "center",
            "sizingH": "fixed",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=b1,
        type="text",
        name="T1",
        text="ENGAGE UPGRADE",
        props={"fill": "#000000", "fontSize": 13, "fontFamily": "Antonio"},
    )

    b2 = add_cmd(
        "frame.create",
        parent=root,
        name="Btn Diagnostic",
        x=390,
        y=684,
        width=150,
        height=44,
        props={
            "fill": c_lilac,
            "cornerRadius": 0,
            "layout": "horizontal",
            "padding": [0, 10, 0, 10],
            "mainAlign": "center",
            "crossAlign": "center",
            "sizingH": "fixed",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=b2,
        type="text",
        name="T2",
        text="DIAGNOSTIC AUDIT",
        props={"fill": "#000000", "fontSize": 13, "fontFamily": "Antonio"},
    )

    b3 = add_cmd(
        "frame.create",
        parent=root,
        name="Btn Hygiene",
        x=544,
        y=684,
        width=140,
        height=44,
        props={
            "fill": c_cyan,
            "cornerRadius": 0,
            "layout": "horizontal",
            "padding": [0, 10, 0, 10],
            "mainAlign": "center",
            "crossAlign": "center",
            "sizingH": "fixed",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=b3,
        type="text",
        name="T3",
        text="SYSTEM HYGIENE",
        props={"fill": "#000000", "fontSize": 13, "fontFamily": "Antonio"},
    )

    tr = add_cmd(
        "frame.create",
        parent=root,
        name="Trailing Bottom Rail",
        x=688,
        y=684,
        width=340,
        height=44,
        props={
            "fill": c_frame,
            "cornerRadius": [0, 22, 22, 0],
            "layout": "horizontal",
            "padding": [0, 16, 0, 16],
            "mainAlign": "spaceBetween",
            "crossAlign": "center",
            "sizingH": "fixed",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=tr,
        type="text",
        name="Tag",
        text="SYS-01 // TELEMETRY",
        props={"fill": c_text_frame, "fontSize": 13, "fontFamily": "Antonio"},
    )
    add_cmd(
        "node.create",
        parent=tr,
        type="text",
        name="Tag2",
        text="COPILOT ONLINE",
        props={"fill": c_text_frame, "fontSize": 13, "fontFamily": "Antonio"},
    )

    # 5. LEFT SPINE NAVIGATION (Vertically Centered, No Emojis)
    tabs_box = add_cmd(
        "frame.create",
        parent=root,
        name="Left Centered Tabs",
        x=12,
        y=239,
        width=128,
        height=264,
        props={
            "layout": "vertical",
            "gap": 4,
            "sizingH": "fixed",
            "sizingV": "hug",
            "fill": "none",
        },
    )

    n1 = add_cmd(
        "frame.create",
        parent=tabs_box,
        name="Nav 01 Dashboard",
        height=46,
        props={
            "fill": c_peach,
            "cornerRadius": 0,
            "layout": "vertical",
            "padding": [4, 8, 4, 8],
            "mainAlign": "center",
            "sizingH": "fill",
            "sizingV": "fixed",
        },
    )
    add_cmd(
        "node.create",
        parent=n1,
        type="text",
        name="N1",
        text="01  DASHBOARD ▶",
        props={"fill": "#000000", "fontSize": 13, "fontFamily": "Antonio"},
    )

    for num_lbl, col, txt in [
        ("02", c_lilac, "02  MAINTAIN"),
        ("03", "#9999ff", "03  AI COPILOT"),
        ("04", c_cyan, "04  REFRESH DATA"),
    ]:
        nb = add_cmd(
            "frame.create",
            parent=tabs_box,
            name=f"Nav {num_lbl}",
            height=42,
            props={
                "fill": col,
                "cornerRadius": 0,
                "layout": "vertical",
                "padding": [4, 8, 4, 8],
                "mainAlign": "center",
                "sizingH": "fill",
                "sizingV": "fixed",
            },
        )
        add_cmd(
            "node.create",
            parent=nb,
            type="text",
            name="T",
            text=txt,
            props={"fill": "#000000", "fontSize": 12, "fontFamily": "Antonio"},
        )

    for num, col in [
        ("441-9886", "#3d240c"),
        ("02-65459", "#2e1633"),
        ("05-22435", "#14223b"),
    ]:
        ch = add_cmd(
            "frame.create",
            parent=tabs_box,
            name=f"Chiclet {num}",
            height=22,
            props={
                "fill": col,
                "cornerRadius": 0,
                "layout": "horizontal",
                "padding": [2, 8, 2, 8],
                "crossAlign": "center",
                "sizingH": "fill",
                "sizingV": "fixed",
            },
        )
        add_cmd(
            "node.create",
            parent=ch,
            type="text",
            name="T",
            text=num,
            props={"fill": "#aaaaaa", "fontSize": 11, "fontFamily": "Antonio"},
        )

    # 6. CENTRAL PODS DECK (FRAMELESS OLED - NO BLUE BOXES)
    deck = add_cmd(
        "frame.create",
        parent=root,
        name="Central Pods Deck",
        x=150,
        y=66,
        width=878,
        height=608,
        props={
            "layout": "vertical",
            "gap": 16,
            "sizingH": "fixed",
            "sizingV": "fixed",
            "fill": "none",
        },
    )

    def make_pod(pod_id_str, title, accent_color, rows):
        # Frameless group: no blue background, no border
        pod = add_cmd(
            "frame.create",
            parent=deck,
            name=f"Pod {pod_id_str}: {title}",
            props={
                "fill": "none",
                "layout": "vertical",
                "padding": [0, 0, 0, 0],
                "gap": 6,
                "sizingH": "fill",
                "sizingV": "hug",
            },
        )

        ph = add_cmd(
            "frame.create",
            parent=pod,
            name="Pod Header",
            height=24,
            props={
                "layout": "horizontal",
                "gap": 8,
                "crossAlign": "center",
                "sizingH": "fill",
                "sizingV": "fixed",
                "fill": "none",
            },
        )
        add_cmd(
            "frame.create",
            parent=ph,
            name="Pod Cap",
            width=16,
            height=16,
            props={"fill": accent_color, "cornerRadius": [8, 0, 0, 8]},
        )
        add_cmd(
            "node.create",
            parent=ph,
            type="text",
            name="Pod Title",
            text=f"{pod_id_str} // {title}".upper(),
            props={"fill": accent_color, "fontSize": 14, "fontFamily": "Antonio"},
        )
        add_cmd(
            "frame.create",
            parent=ph,
            name="Accent Line",
            height=2,
            props={"fill": accent_color, "sizingH": "fill", "sizingV": "fixed"},
        )

        for lbl, val, stat, is_warn in rows:
            row = add_cmd(
                "frame.create",
                parent=pod,
                name=f"Row: {lbl}",
                height=26,
                props={
                    "layout": "horizontal",
                    "gap": 4,
                    "crossAlign": "center",
                    "sizingH": "fill",
                    "sizingV": "fixed",
                    "fill": "none",
                },
            )

            # Left Cap: [ARCH ... 90]
            c1 = add_cmd(
                "frame.create",
                parent=row,
                name="Label Cap [ARCH-90]",
                width=200,
                height=24,
                props={
                    "fill": accent_color,
                    "cornerRadius": [12, 0, 0, 12],
                    "layout": "horizontal",
                    "padding": [2, 6, 2, 12],
                    "crossAlign": "center",
                    "sizingH": "fixed",
                    "sizingV": "fixed",
                },
            )
            add_cmd(
                "node.create",
                parent=c1,
                type="text",
                name="Lbl",
                text=lbl,
                props={"fill": "#000000", "fontSize": 12, "fontFamily": "Antonio"},
            )

            # Mid Segment
            if pod_style == "frameless_tracks":
                c2 = add_cmd(
                    "frame.create",
                    parent=row,
                    name="Value Mid [90-90]",
                    height=24,
                    props={
                        "fill": c_row_bg,
                        "stroke": c_border,
                        "strokeWeight": 1,
                        "cornerRadius": 0,
                        "layout": "horizontal",
                        "padding": [2, 10, 2, 10],
                        "crossAlign": "center",
                        "sizingH": "fill",
                        "sizingV": "fixed",
                    },
                )
            else:  # pure_oled
                c2 = add_cmd(
                    "frame.create",
                    parent=row,
                    name="Value Mid [90-90]",
                    height=24,
                    props={
                        "fill": "none",
                        "cornerRadius": 0,
                        "layout": "horizontal",
                        "padding": [2, 10, 2, 10],
                        "crossAlign": "center",
                        "sizingH": "fill",
                        "sizingV": "fixed",
                    },
                )

            add_cmd(
                "node.create",
                parent=c2,
                type="text",
                name="Val",
                text=val,
                props={
                    "fill": "#93c5fd" if not is_warn else "#fde047",
                    "fontSize": 13,
                    "fontFamily": "Antonio",
                },
            )

            # Right Cap: [90 ... ARCH]
            c3 = add_cmd(
                "frame.create",
                parent=row,
                name="Status Cap [90-ARCH]",
                width=80,
                height=24,
                props={
                    "fill": c_green if not is_warn else cfg["stat_color"],
                    "cornerRadius": [0, 12, 12, 0],
                    "layout": "horizontal",
                    "padding": [2, 6, 2, 6],
                    "mainAlign": "center",
                    "crossAlign": "center",
                    "sizingH": "fixed",
                    "sizingV": "fixed",
                },
            )
            add_cmd(
                "node.create",
                parent=c3,
                type="text",
                name="Stat",
                text=stat,
                props={"fill": "#000000", "fontSize": 12, "fontFamily": "Antonio"},
            )

    make_pod(
        "POD 01",
        "SYSTEM HEALTH & HYGIENE",
        c_peach,
        [
            ("KERNEL & MODULES", "RUNNING LINUX 7.2.14-ZEN1-1-ZEN (64-BIT)", "PASS ✔", False),
            ("ROOT STORAGE (/)", "34% USED • 300.8 GB UNALLOCATED ON NVME", "PASS ✔", False),
            ("HOST FIREWALL", "FIREWALLD ACTIVE • ZONE: HOME (DIGIWEB PPPOE STEALTH)", "PASS ✔", False),
            ("FAILED SYSTEM SERVICES", "0 FAILED SYSTEMD UNITS", "PASS ✔", False),
            ("HARDWARE PLATFORM & GPU", "GEFORCE GTX 970 (MAXWELL GM204) • 580XX-DKMS 60HZ", "PASS ✔", False),
        ],
    )

    make_pod(
        "POD 02",
        "SYSTEM UPDATES & REPOSITORIES",
        c_lilac,
        [
            ("CORE SYSTEM (KERNEL/DRV)", "0 CORE SYSTEM UPDATES PENDING", "PASS ✔", False),
            ("OFFICIAL REPOSITORIES", "0 OFFICIAL PACKAGES PENDING", "PASS ✔", False),
            (
                "AUR PACKAGES",
                "2 PACKAGES AUDITED (AUR STREAMS CLEAN)",
                "WARN ⚠" if mode != "red" else "CRIT ⚠",
                True,
            ),
            (
                "SECURITY ADVISORIES (CVE)",
                "NO UNPATCHED CVE VULNERABILITIES"
                if mode != "red"
                else "KERNEL SEC ADVISORY DETECTED",
                "PASS ✔" if mode != "red" else "ALERT ⚡",
                mode == "red",
            ),
        ],
    )

    make_pod(
        "POD 03",
        "THIRD-PARTY & PERIPHERALS",
        c_cyan,
        [
            ("HARDWARE FIRMWARE (LVFS)", "FIRMWARE SIGNATURES VERIFIED • LVFS CLEAN", "PASS ✔", False),
            ("ORPHAN PACKAGES", "0 ORPHANS DETECTED • PACMAN CACHE TRIMMED", "PASS ✔", False),
        ],
    )

    out_fig = out_dir / f"{cfg['prefix']}.figcraft"
    out_png = out_dir / f"{cfg['prefix']}.png"
    tmp_json = Path(f"/tmp/{cfg['prefix']}.json")

    with open(tmp_json, "w") as f:
        json.dump(commands, f)

    subprocess.run(["figcraft-cli", "run", str(tmp_json), "--out", str(out_fig)], check=True)
    subprocess.run(["figcraft-cli", "render", str(out_fig), "-o", str(out_png)], check=True)
    subprocess.run(["cp", str(out_png), f"/home/gbur/.gemini/antigravity/brain/f9ecbd36-bf2d-48a3-bcfa-77aa7143f0b8/{cfg['prefix']}.png"], check=True)
    print(f"Generated {out_png}")

if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "yellow"
    b_style = sys.argv[2] if len(sys.argv) > 2 else "segmented_blocks"
    p_style = sys.argv[3] if len(sys.argv) > 3 else "frameless_tracks"
    generate_lcars_theme(mode, b_style, p_style)
