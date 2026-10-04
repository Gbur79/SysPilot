#!/usr/bin/env python3
"""
SysPilot Desktop Dashboard & System Tray
Autonomous Token-Lean SRE Copilot for Arch Linux & derivatives.
"""

import os
import sys
import json
import shutil
import subprocess
import threading
from typing import Dict, Any, List, Optional

from PyQt6.QtCore import Qt, QTimer, pyqtSignal, QObject, QUrl
from PyQt6.QtGui import QIcon, QPixmap, QPainter, QColor, QFont, QPen, QBrush, QDesktopServices
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QTabWidget, QProgressBar, QTextEdit,
    QLineEdit, QFrame, QScrollArea, QSystemTrayIcon, QMenu,
    QCheckBox, QComboBox, QMessageBox, QRadioButton, QButtonGroup,
    QStackedWidget, QDialog
)

# Ensure project root is in sys.path
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(SCRIPT_DIR)
sys.path.insert(0, PROJECT_ROOT)

from core.triage import run_triage, is_gamemode_active, STATUS_FILE
from core.copilot_config import (
    is_goose_installed, is_copilot_ready, get_configured_providers,
    configure_provider, find_goose_binary, get_skill_info,
    save_custom_directives, list_available_playbooks, get_playbook_content
)

AUTOSTART_DIR = os.path.expanduser("~/.config/autostart")
AUTOSTART_FILE = os.path.join(AUTOSTART_DIR, "syspilot.desktop")


def get_terminal_cmd(command_to_run: str, title: str = "SysPilot") -> List[str]:
    """Find available terminal and return command list to execute in terminal."""
    terminals = [
        ("konsole", ["konsole", "-e", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("alacritty", ["alacritty", "-e", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("kitty", ["kitty", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("xfce4-terminal", ["xfce4-terminal", "-e", f"bash -c \"{command_to_run}; echo ''; read -p 'Press Enter to close...'\""]),
        ("gnome-terminal", ["gnome-terminal", "--", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("xterm", ["xterm", "-T", title, "-e", "bash", "-c", f"{command_to_run}; read -p 'Press Enter to close...'"])
    ]
    for term, cmd in terminals:
        if shutil.which(term):
            return cmd
    return ["xterm", "-e", "bash", "-c", f"{command_to_run}; read -p 'Press Enter to close...'"]


def make_status_icon(color_hex: str, symbol: str = "") -> QIcon:
    """Generate dynamic sharp vector-like status icon in memory."""
    pix = QPixmap(32, 32)
    pix.fill(Qt.GlobalColor.transparent)
    p = QPainter(pix)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)

    # Outer circle
    p.setBrush(QBrush(QColor(color_hex)))
    p.setPen(Qt.PenStyle.NoPen)
    p.drawEllipse(3, 3, 26, 26)

    # Symbol if provided
    if symbol:
        p.setPen(QPen(QColor("#ffffff")))
        font = QFont("Sans", 11, QFont.Weight.Bold)
        p.setFont(font)
        p.drawText(pix.rect(), Qt.AlignmentFlag.AlignCenter, symbol)

    p.end()
    return QIcon(pix)


DARK_STYLESHEET = """
QMainWindow {
    background-color: #0b1120;
    color: #f8fafc;
}
QWidget {
    background-color: #0b1120;
    color: #f8fafc;
    font-family: 'Segoe UI', 'Ubuntu', 'Cantarell', sans-serif;
    font-size: 13px;
}
QTabWidget::pane {
    border: 1px solid #1e293b;
    background-color: #0f172a;
    border-radius: 8px;
    padding: 6px;
}
QTabBar::tab {
    background-color: #1e293b;
    color: #94a3b8;
    padding: 9px 18px;
    margin-right: 4px;
    border-top-left-radius: 6px;
    border-top-right-radius: 6px;
    font-weight: bold;
}
QTabBar::tab:selected {
    background-color: #2563eb;
    color: #ffffff;
}
QTabBar::tab:hover:!selected {
    background-color: #334155;
    color: #e2e8f0;
}
QFrame.card {
    background-color: #1e293b;
    border: 1px solid #334155;
    border-radius: 8px;
    padding: 12px;
}
QLabel.sectionTitle {
    font-size: 15px;
    font-weight: bold;
    color: #38bdf8;
    margin-bottom: 4px;
}
QPushButton {
    background-color: #2563eb;
    color: #ffffff;
    border: none;
    border-radius: 6px;
    padding: 8px 16px;
    font-weight: bold;
}
QPushButton:hover {
    background-color: #1d4ed8;
}
QPushButton:pressed {
    background-color: #1e40af;
}
QPushButton.secondary {
    background-color: #334155;
    color: #e2e8f0;
}
QPushButton.secondary:hover {
    background-color: #475569;
}
QPushButton.success {
    background-color: #059669;
}
QPushButton.success:hover {
    background-color: #047857;
}
QPushButton.warning {
    background-color: #d97706;
}
QPushButton.warning:hover {
    background-color: #b45309;
}
QPushButton.purple {
    background-color: #7c3aed;
}
QPushButton.purple:hover {
    background-color: #6d28d9;
}
QProgressBar {
    border: 1px solid #334155;
    border-radius: 5px;
    text-align: center;
    background-color: #020617;
    color: #ffffff;
    font-weight: bold;
}
QProgressBar::chunk {
    background-color: #3b82f6;
    border-radius: 4px;
}
QTextEdit, QLineEdit {
    background-color: #020617;
    border: 1px solid #334155;
    border-radius: 6px;
    color: #f8fafc;
    padding: 8px;
    font-family: 'Hack', 'Fira Code', 'JetBrains Mono', 'Consolas', monospace;
}
QTextEdit:focus, QLineEdit:focus {
    border: 1px solid #38bdf8;
}
QScrollBar:vertical {
    border: none;
    background: #0b1120;
    width: 8px;
    border-radius: 4px;
}
QScrollBar::handle:vertical {
    background: #334155;
    border-radius: 4px;
}
"""


class CopilotWorker(QObject):
    """Background worker for running Goose copilot commands without freezing GUI."""
    output_signal = pyqtSignal(str)
    finished_signal = pyqtSignal()

    def __init__(self, query: str):
        super().__init__()
        self.query = query

    def run(self):
        recipe_path = os.path.join(PROJECT_ROOT, "copilot", "recipe.yaml")
        goose_bin = find_goose_binary()

        if not goose_bin:
            self.output_signal.emit("[Error] Goose CLI is not found on your system.\nPlease install goose via AUR or goose.ai.\n")
            self.finished_signal.emit()
            return

        self.output_signal.emit(f"🚀 Calling SysPilot Copilot with query:\n\"{self.query}\"\n(Token-Lean mode: reading local telemetry & playbooks...)\n\n")

        try:
            cmd = [
                goose_bin, "run",
                "--recipe", recipe_path,
                "--params", f"user_query={self.query}",
                "-q"
            ]
            process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                cwd=PROJECT_ROOT
            )
            for line in process.stdout:
                self.output_signal.emit(line)
            process.wait()
            self.output_signal.emit("\n✔ Copilot analysis complete.\n")
        except Exception as e:
            self.output_signal.emit(f"\n[Error executing Copilot]: {str(e)}\n")

        self.finished_signal.emit()


class SysPilotWindow(QMainWindow):
    """Main Dashboard Window."""

    def __init__(self, tray_app):
        super().__init__()
        self.tray_app = tray_app
        self.setWindowTitle("SysPilot — Autonomous SRE Desktop Copilot")
        self.resize(900, 740)
        self.setStyleSheet(DARK_STYLESHEET)

        self.central_widget = QWidget()
        self.setCentralWidget(self.central_widget)
        self.main_layout = QVBoxLayout(self.central_widget)

        # Header Status Banner
        self.header_frame = QFrame()
        self.header_frame.setProperty("class", "card")
        self.header_frame.setStyleSheet("background-color: #1e293b; border-radius: 8px; padding: 12px;")
        header_layout = QHBoxLayout(self.header_frame)

        self.status_icon_label = QLabel("🟢")
        self.status_icon_label.setStyleSheet("font-size: 28px;")
        header_layout.addWidget(self.status_icon_label)

        status_text_layout = QVBoxLayout()
        self.status_title = QLabel("System Status: Flight Ready")
        self.status_title.setStyleSheet("font-size: 18px; font-weight: bold; color: #10b981;")
        self.status_sub = QLabel("All core diagnostics pass. System is primed and stable.")
        self.status_sub.setStyleSheet("color: #94a3b8; font-size: 12px;")
        status_text_layout.addWidget(self.status_title)
        status_text_layout.addWidget(self.status_sub)
        header_layout.addLayout(status_text_layout, stretch=1)

        self.refresh_btn = QPushButton("↻ Refresh Triage")
        self.refresh_btn.setProperty("class", "secondary")
        self.refresh_btn.clicked.connect(self.trigger_refresh)
        header_layout.addWidget(self.refresh_btn)

        self.main_layout.addWidget(self.header_frame)

        # Main Navigation Tabs
        self.tabs = QTabWidget()
        self.main_layout.addWidget(self.tabs)

        # Tab 1: Lean Dashboard
        self.dashboard_tab = QWidget()
        self.setup_lean_dashboard_tab()
        self.tabs.addTab(self.dashboard_tab, "✈ Dashboard")

        # Tab 2: System Maintenance
        self.maintenance_tab = QWidget()
        self.setup_maintenance_tab()
        self.tabs.addTab(self.maintenance_tab, "🛠 System Maintenance")

        # Tab 3: Copilot AI (With Layman Onboarding Wizard)
        self.copilot_tab = QWidget()
        self.setup_copilot_tab()
        self.tabs.addTab(self.copilot_tab, "🤖 AI Copilot (Goose SRE)")

        # Tab 4: SRE Skill & Persona Blueprint
        self.skill_tab = QWidget()
        self.setup_skill_blueprint_tab()
        self.tabs.addTab(self.skill_tab, "🧠 SRE Skill & Persona")

        # Tab 5: Settings & Autostart
        self.settings_tab = QWidget()
        self.setup_settings_tab()
        self.tabs.addTab(self.settings_tab, "⚙ Settings & Autostart")

        # Load initial data
        self.update_ui_from_state()

    # --------------------------------------------------------------------------
    # TAB 1: LEAN DASHBOARD
    # --------------------------------------------------------------------------
    def setup_lean_dashboard_tab(self):
        layout = QVBoxLayout(self.dashboard_tab)

        # Card 1: Official System Updates
        self.card_updates = QFrame()
        self.card_updates.setProperty("class", "card")
        u_layout = QVBoxLayout(self.card_updates)

        u_title = QLabel("📦 Official Repository & Core System Updates")
        u_title.setProperty("class", "sectionTitle")
        u_layout.addWidget(u_title)

        self.updates_lbl = QLabel("0 pending updates (0 core system packages)")
        self.updates_lbl.setStyleSheet("font-size: 14px; font-weight: bold;")
        u_layout.addWidget(self.updates_lbl)

        self.core_pkgs_lbl = QLabel("All core packages (kernel, systemd, drivers, bootloader) are up to date.")
        self.core_pkgs_lbl.setStyleSheet("color: #94a3b8; font-size: 12px;")
        u_layout.addWidget(self.core_pkgs_lbl)

        u_btn_box = QHBoxLayout()
        self.btn_guarded_upgrade = QPushButton("⚡ Guarded System Upgrade")
        self.btn_guarded_upgrade.setProperty("class", "success")
        self.btn_guarded_upgrade.clicked.connect(self.run_guarded_upgrade_terminal)
        u_btn_box.addWidget(self.btn_guarded_upgrade)

        self.btn_quick_audit = QPushButton("🔍 Full Diagnostic Audit")
        self.btn_quick_audit.setProperty("class", "secondary")
        self.btn_quick_audit.clicked.connect(self.run_audit_terminal)
        u_btn_box.addWidget(self.btn_quick_audit)

        u_layout.addLayout(u_btn_box)
        layout.addWidget(self.card_updates)

        # Card 2: Standalone & Third-Party Apps Update Check
        self.card_software = QFrame()
        self.card_software.setProperty("class", "card")
        s_layout = QVBoxLayout(self.card_software)

        s_title = QLabel("🚀 Standalone & Third-Party Apps Update Check")
        s_title.setProperty("class", "sectionTitle")
        s_layout.addWidget(s_title)

        self.software_lbl = QLabel("AUR: 0 pending | Flatpak: Not installed | Goose: 1.53.0 (up to date) | UV: 0.12.23 (up to date)")
        self.software_lbl.setStyleSheet("color: #cbd5e1; font-size: 13px;")
        s_layout.addWidget(self.software_lbl)

        s_btn_box = QHBoxLayout()
        self.btn_check_apps = QPushButton("📦 Standalone & 3rd-Party Update Triage")
        self.btn_check_apps.setProperty("class", "purple")
        self.btn_check_apps.clicked.connect(self.run_software_terminal)
        s_btn_box.addWidget(self.btn_check_apps)

        self.btn_update_goose = QPushButton("⚡ Update Goose AI Agent")
        self.btn_update_goose.setProperty("class", "success")
        self.btn_update_goose.setVisible(False)
        self.btn_update_goose.clicked.connect(self.run_update_goose_terminal)
        s_btn_box.addWidget(self.btn_update_goose)
        s_layout.addLayout(s_btn_box)

        layout.addWidget(self.card_software)

        # Card 3: Core Health & Storage Overview
        self.card_health = QFrame()
        self.card_health.setProperty("class", "card")
        h_layout = QVBoxLayout(self.card_health)

        h_title = QLabel("🛡 System Health & Disk State")
        h_title.setProperty("class", "sectionTitle")
        h_layout.addWidget(h_title)

        self.services_lbl = QLabel("✔ Systemd Units: All system and user units operational.")
        self.services_lbl.setStyleSheet("color: #10b981; font-weight: bold;")
        h_layout.addWidget(self.services_lbl)

        # Root Disk Bar
        h_layout.addWidget(QLabel("Root Partition Usage (/):"))
        self.disk_bar = QProgressBar()
        self.disk_bar.setValue(23)
        self.disk_bar.setFormat("%v% used")
        h_layout.addWidget(self.disk_bar)

        self.disk_sub_lbl = QLabel("334.6 GB available")
        self.disk_sub_lbl.setStyleSheet("color: #94a3b8; font-size: 11px;")
        h_layout.addWidget(self.disk_sub_lbl)

        self.gaming_lbl = QLabel("🎮 Gaming Mode: Inactive (Normal desktop state)")
        self.gaming_lbl.setStyleSheet("color: #94a3b8; font-size: 12px; margin-top: 4px;")
        h_layout.addWidget(self.gaming_lbl)

        layout.addWidget(self.card_health)
        layout.addStretch()

    # --------------------------------------------------------------------------
    # TAB 2: SYSTEM MAINTENANCE
    # --------------------------------------------------------------------------
    def setup_maintenance_tab(self):
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        content = QWidget()
        layout = QVBoxLayout(content)

        # Header info
        m_intro = QLabel("<b>Universal SRE Maintenance Suite:</b> Safe, atomic maintenance utilities that maintain system stability without breaking package dependencies or user data.")
        m_intro.setStyleSheet("background-color: #1e293b; border-left: 4px solid #10b981; padding: 10px; border-radius: 4px;")
        layout.addWidget(m_intro)

        # 1. Orphan Package Triage & Prune
        c1 = QFrame()
        c1.setProperty("class", "card")
        l1 = QVBoxLayout(c1)
        t1 = QLabel("1) 🗑 Orphan Package Triage & Prune")
        t1.setProperty("class", "sectionTitle")
        l1.addWidget(t1)
        d1 = QLabel("Interactive 3-tier orphan package resolver. Safely purges truly abandoned dependencies without breaking optional dependencies (optdepends).")
        d1.setStyleSheet("color: #94a3b8;")
        l1.addWidget(d1)
        b1 = QPushButton("Run Orphan Package Triage (Interactive)")
        b1.setProperty("class", "warning")
        b1.clicked.connect(lambda: self.run_custom_terminal(f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --orphans", "SysPilot Orphan Triage"))
        l1.addWidget(b1)
        layout.addWidget(c1)

        # 2. Refresh & Rank Mirrors
        c2 = QFrame()
        c2.setProperty("class", "card")
        l2 = QVBoxLayout(c2)
        t2 = QLabel("2) 🌐 Refresh & Rank Regional Mirrors")
        t2.setProperty("class", "sectionTitle")
        l2.addWidget(t2)
        d2 = QLabel("Benchmarks and ranks the fastest, most reliable regional Arch and EndeavourOS/distro mirrors using an atomic fallback gate.")
        d2.setStyleSheet("color: #94a3b8;")
        l2.addWidget(d2)
        b2 = QPushButton("Benchmark & Rank Fastest Mirrors")
        b2.setProperty("class", "secondary")
        b2.clicked.connect(lambda: self.run_custom_terminal(f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --mirrors", "SysPilot Mirror Ranking"))
        l2.addWidget(b2)
        layout.addWidget(c2)

        # 3. Clean (Safe Maintenance)
        c3 = QFrame()
        c3.setProperty("class", "card")
        l3 = QVBoxLayout(c3)
        t3 = QLabel("3) 🧹 Safe Maintenance & Cache Trimming")
        t3.setProperty("class", "sectionTitle")
        l3.addWidget(t3)
        d3 = QLabel("Trims pacman package cache to latest 2 versions, vacuums systemd journal logs (>14 days), and purges obsolete temporary run logs.")
        d3.setStyleSheet("color: #94a3b8;")
        l3.addWidget(d3)
        b3 = QPushButton("Execute Safe Maintenance")
        b3.setProperty("class", "success")
        b3.clicked.connect(self.run_maintenance_terminal)
        l3.addWidget(b3)
        layout.addWidget(c3)

        # 4. Deep Clean
        c4 = QFrame()
        c4.setProperty("class", "card")
        l4 = QVBoxLayout(c4)
        t4 = QLabel("4) 🧼 Deep Clean (Trash, Browser, Thumbnails)")
        t4.setProperty("class", "sectionTitle")
        l4.addWidget(t4)
        d4 = QLabel("Deep non-destructive reclamation: empties user Trash, clears thumbnail caches, browser cache tempfiles, and old diagnostic runs.")
        d4.setStyleSheet("color: #94a3b8;")
        l4.addWidget(d4)
        b4 = QPushButton("Execute Deep Clean")
        b4.setProperty("class", "secondary")
        b4.clicked.connect(lambda: self.run_custom_terminal(f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --deep-clean", "SysPilot Deep Clean"))
        l4.addWidget(b4)
        layout.addWidget(c4)

        # 5. Pacnew Configuration Conflicts
        c5 = QFrame()
        c5.setProperty("class", "card")
        l5 = QVBoxLayout(c5)
        t5 = QLabel("5) 🔧 Configuration Conflicts (.pacnew Reconciler)")
        t5.setProperty("class", "sectionTitle")
        l5.addWidget(t5)
        self.pacnew_desc_lbl = QLabel("Configuration files (.pacnew) created during package updates.")
        self.pacnew_desc_lbl.setStyleSheet("color: #94a3b8;")
        l5.addWidget(self.pacnew_desc_lbl)
        b5 = QPushButton("Ask AI Copilot to Reconcile .pacnew Files")
        b5.clicked.connect(lambda: self.ask_copilot("Scan for any active .pacnew configuration files and guide me through surgical, non-destructive merging"))
        l5.addWidget(b5)
        layout.addWidget(c5)

        layout.addStretch()
        scroll.setWidget(content)
        m_layout = QVBoxLayout(self.maintenance_tab)
        m_layout.addWidget(scroll)

    # --------------------------------------------------------------------------
    # TAB 3: AI COPILOT (WITH ONBOARDING WIZARD)
    # --------------------------------------------------------------------------
    def setup_copilot_tab(self):
        self.copilot_layout = QVBoxLayout(self.copilot_tab)
        self.copilot_stack = QStackedWidget()
        self.copilot_layout.addWidget(self.copilot_stack)

        # Page 0: Active Copilot Workspace
        self.copilot_active_page = QWidget()
        self.setup_copilot_active_page()
        self.copilot_stack.addWidget(self.copilot_active_page)

        # Page 1: Layman Onboarding Wizard (First-Run or Key Setup)
        self.copilot_setup_page = QWidget()
        self.setup_copilot_setup_page()
        self.copilot_stack.addWidget(self.copilot_setup_page)

        # Switch page according to current configuration
        self.refresh_copilot_page()

    def refresh_copilot_page(self):
        ready, _ = is_copilot_ready()
        if ready:
            self.copilot_stack.setCurrentIndex(0)
            providers = get_configured_providers()
            active_p = [k for k, v in providers.items() if v]
            p_name = active_p[0].capitalize() if active_p else "Configured"
            self.badge_lbl.setText(f"🟢 Connected to {p_name} | Token-Lean Mode Active")
        else:
            self.copilot_stack.setCurrentIndex(1)

    def setup_copilot_active_page(self):
        layout = QVBoxLayout(self.copilot_active_page)

        # Top connection bar
        conn_bar = QHBoxLayout()
        self.badge_lbl = QLabel("🟢 Connected to AI Provider | Token-Lean Mode Active")
        self.badge_lbl.setStyleSheet("font-weight: bold; color: #10b981; font-size: 13px;")
        conn_bar.addWidget(self.badge_lbl)
        conn_bar.addStretch()

        btn_view_persona = QPushButton("🧠 View Agent Persona & Directives")
        btn_view_persona.setProperty("class", "purple")
        btn_view_persona.clicked.connect(lambda: self.tabs.setCurrentWidget(self.skill_tab))
        conn_bar.addWidget(btn_view_persona)

        btn_reconfig = QPushButton("⚙ Change Provider / API Key")
        btn_reconfig.setProperty("class", "secondary")
        btn_reconfig.clicked.connect(lambda: self.copilot_stack.setCurrentIndex(1))
        conn_bar.addWidget(btn_reconfig)
        layout.addLayout(conn_bar)

        # Playbook buttons
        layout.addWidget(QLabel("<b>Battle-Tested SRE Playbooks (1-Click Solutions):</b>"))
        pills_layout = QHBoxLayout()

        btn_faf = QPushButton("⚡ Setup / Repair FAF Client")
        btn_faf.setProperty("class", "secondary")
        btn_faf.clicked.connect(lambda: self.ask_copilot("Explain how to setup and configure Forged Alliance Forever (FAF) with the faf-linux runner and fix Game.prefs sync"))
        pills_layout.addWidget(btn_faf)

        btn_proton = QPushButton("🎮 Steam / Proton Launch Fix")
        btn_proton.setProperty("class", "secondary")
        btn_proton.clicked.connect(lambda: self.ask_copilot("Diagnose why a game is failing to launch in Steam under Proton and check missing 32-bit Vulkan/graphics libraries"))
        pills_layout.addWidget(btn_proton)

        btn_pacnew = QPushButton("🔧 Reconcile .pacnew Files")
        btn_pacnew.setProperty("class", "secondary")
        btn_pacnew.clicked.connect(lambda: self.ask_copilot("Check for active .pacnew configuration files and guide me through safe surgical merging"))
        pills_layout.addWidget(btn_pacnew)

        btn_audio = QPushButton("🔊 PipeWire Audio Fix")
        btn_audio.setProperty("class", "secondary")
        btn_audio.clicked.connect(lambda: self.ask_copilot("Audio is crackling or missing sinks in PipeWire/WirePlumber, how do I reset and fix it?"))
        pills_layout.addWidget(btn_audio)

        layout.addLayout(pills_layout)

        # Interactive Terminal Launcher
        btn_interactive = QPushButton("💻 Open Interactive Copilot Session in Terminal")
        btn_interactive.clicked.connect(self.open_copilot_terminal)
        layout.addWidget(btn_interactive)

        # Query input
        input_layout = QHBoxLayout()
        self.query_input = QLineEdit()
        self.query_input.setPlaceholderText("Ask SysPilot Copilot anything (e.g. 'Why did my audio fail?' or 'Fix service XYZ')...")
        self.query_input.returnPressed.connect(self.submit_custom_query)
        input_layout.addWidget(self.query_input, stretch=1)

        self.btn_ask = QPushButton("Ask Copilot 🚀")
        self.btn_ask.clicked.connect(self.submit_custom_query)
        input_layout.addWidget(self.btn_ask)
        layout.addLayout(input_layout)

        # Output console
        self.copilot_output = QTextEdit()
        self.copilot_output.setReadOnly(True)
        self.copilot_output.setPlaceholderText("SysPilot Copilot telemetry and surgical recommendations will stream here...")
        layout.addWidget(self.copilot_output, stretch=1)

    def setup_copilot_setup_page(self):
        """Layman-friendly, 1-minute onboarding wizard."""
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        content = QWidget()
        layout = QVBoxLayout(content)

        # Header card
        header_card = QFrame()
        header_card.setProperty("class", "card")
        header_card.setStyleSheet("background-color: #1e293b; border-left: 5px solid #3b82f6; padding: 16px; border-radius: 8px;")
        hc_layout = QVBoxLayout(header_card)

        t = QLabel("✈️ Welcome to SysPilot AI Copilot Setup")
        t.setStyleSheet("font-size: 18px; font-weight: bold; color: #38bdf8;")
        hc_layout.addWidget(t)

        sub = QLabel(
            "SysPilot includes an autonomous Site Reliability Engineer (Copilot) powered by <b>Goose</b>.<br>"
            "To activate your Copilot, simply choose an AI provider and paste your API key below.<br>"
            "<i>(No terminal commands or YAML editing required — SysPilot configures everything automatically).</i>"
        )
        sub.setStyleSheet("color: #cbd5e1; font-size: 13px; line-height: 1.4;")
        hc_layout.addWidget(sub)
        layout.addWidget(header_card)

        # Step 1: Provider selection
        p_card = QFrame()
        p_card.setProperty("class", "card")
        p_layout = QVBoxLayout(p_card)
        p_title = QLabel("Step 1: Choose Your AI Provider")
        p_title.setProperty("class", "sectionTitle")
        p_layout.addWidget(p_title)

        self.rb_google = QRadioButton("Google Gemini Flash (Recommended — Fastest, lowest cost & generous free tier)")
        self.rb_google.setChecked(True)
        self.rb_google.setStyleSheet("font-weight: bold; color: #10b981; font-size: 13px;")
        p_layout.addWidget(self.rb_google)

        self.rb_openai = QRadioButton("OpenAI (GPT-4o / GPT-4o-mini)")
        p_layout.addWidget(self.rb_openai)

        self.rb_anthropic = QRadioButton("Anthropic Claude (Claude 3.5 Sonnet / Haiku)")
        p_layout.addWidget(self.rb_anthropic)

        self.rb_ollama = QRadioButton("Local Ollama (Self-hosted offline, e.g. qwen2.5-coder)")
        p_layout.addWidget(self.rb_ollama)

        self.provider_group = QButtonGroup()
        self.provider_group.addButton(self.rb_google, 1)
        self.provider_group.addButton(self.rb_openai, 2)
        self.provider_group.addButton(self.rb_anthropic, 3)
        self.provider_group.addButton(self.rb_ollama, 4)
        self.provider_group.buttonClicked.connect(self.on_provider_changed)

        layout.addWidget(p_card)

        # Step 2: API Key input & Free Link
        k_card = QFrame()
        k_card.setProperty("class", "card")
        k_layout = QVBoxLayout(k_card)
        k_title = QLabel("Step 2: Enter Your API Key")
        k_title.setProperty("class", "sectionTitle")
        k_layout.addWidget(k_title)

        self.free_link_lbl = QLabel(
            "👉 <b>Don't have a Google Gemini key?</b> "
            "<a href='https://aistudio.google.com/app/apikey' style='color: #38bdf8; text-decoration: underline;'>"
            "Click here to get a free API Key from Google AI Studio</a> (Takes 30 seconds, no credit card required)."
        )
        self.free_link_lbl.setOpenExternalLinks(True)
        k_layout.addWidget(self.free_link_lbl)

        key_box = QHBoxLayout()
        self.api_key_input = QLineEdit()
        self.api_key_input.setPlaceholderText("Paste your API key here (e.g. AIzaSy...)")
        self.api_key_input.setEchoMode(QLineEdit.EchoMode.Password)
        key_box.addWidget(self.api_key_input, stretch=1)

        self.btn_show_key = QPushButton("👁 Show")
        self.btn_show_key.setProperty("class", "secondary")
        self.btn_show_key.clicked.connect(self.toggle_show_key)
        key_box.addWidget(self.btn_show_key)
        k_layout.addLayout(key_box)

        layout.addWidget(k_card)

        # Step 3: Save & Test Button
        act_box = QHBoxLayout()
        self.btn_save_copilot = QPushButton("🚀 Save & Connect Copilot")
        self.btn_save_copilot.setProperty("class", "success")
        self.btn_save_copilot.setStyleSheet("padding: 10px 20px; font-size: 14px;")
        self.btn_save_copilot.clicked.connect(self.save_and_test_copilot)
        act_box.addWidget(self.btn_save_copilot)

        self.btn_cancel_setup = QPushButton("Back to Copilot")
        self.btn_cancel_setup.setProperty("class", "secondary")
        self.btn_cancel_setup.clicked.connect(lambda: self.copilot_stack.setCurrentIndex(0))
        act_box.addWidget(self.btn_cancel_setup)
        act_box.addStretch()

        layout.addLayout(act_box)

        # Feedback label
        self.setup_feedback_lbl = QLabel("")
        self.setup_feedback_lbl.setStyleSheet("font-size: 13px; margin-top: 8px;")
        layout.addWidget(self.setup_feedback_lbl)

        layout.addStretch()
        scroll.setWidget(content)
        s_layout = QVBoxLayout(self.copilot_setup_page)
        s_layout.addWidget(scroll)

    def on_provider_changed(self, button):
        if self.rb_google.isChecked():
            self.free_link_lbl.setText(
                "👉 <b>Don't have a Google Gemini key?</b> "
                "<a href='https://aistudio.google.com/app/apikey' style='color: #38bdf8; text-decoration: underline;'>"
                "Click here to get a free API Key from Google AI Studio</a> (Takes 30 seconds, no credit card required)."
            )
            self.api_key_input.setPlaceholderText("Paste your Google API key (e.g. AIzaSy...)")
        elif self.rb_openai.isChecked():
            self.free_link_lbl.setText(
                "👉 <b>Need an OpenAI key?</b> "
                "<a href='https://platform.openai.com/api-keys' style='color: #38bdf8; text-decoration: underline;'>"
                "Click here to get an OpenAI API Key</a>"
            )
            self.api_key_input.setPlaceholderText("Paste your OpenAI API key (e.g. sk-...)")
        elif self.rb_anthropic.isChecked():
            self.free_link_lbl.setText(
                "👉 <b>Need an Anthropic key?</b> "
                "<a href='https://console.anthropic.com/settings/keys' style='color: #38bdf8; text-decoration: underline;'>"
                "Click here to get an Anthropic API Key</a>"
            )
            self.api_key_input.setPlaceholderText("Paste your Anthropic key (e.g. sk-ant-...)")
        elif self.rb_ollama.isChecked():
            self.free_link_lbl.setText("👉 <b>Offline Local LLM:</b> Make sure Ollama is running (`ollama serve`).")
            self.api_key_input.setPlaceholderText("Ollama Host URL (default: http://localhost:11434)")

    def toggle_show_key(self):
        if self.api_key_input.echoMode() == QLineEdit.EchoMode.Password:
            self.api_key_input.setEchoMode(QLineEdit.EchoMode.Normal)
            self.btn_show_key.setText("🔒 Hide")
        else:
            self.api_key_input.setEchoMode(QLineEdit.EchoMode.Password)
            self.btn_show_key.setText("👁 Show")

    def save_and_test_copilot(self):
        p_type = "google"
        if self.rb_openai.isChecked(): p_type = "openai"
        elif self.rb_anthropic.isChecked(): p_type = "anthropic"
        elif self.rb_ollama.isChecked(): p_type = "ollama"

        key = self.api_key_input.text().strip()
        if not key and p_type != "ollama":
            self.setup_feedback_lbl.setText("⚠️ Please paste your API key before connecting.")
            self.setup_feedback_lbl.setStyleSheet("color: #f59e0b; font-weight: bold;")
            return

        self.setup_feedback_lbl.setText("⏳ Saving configuration and testing Copilot connection...")
        self.setup_feedback_lbl.setStyleSheet("color: #38bdf8;")
        self.btn_save_copilot.setEnabled(False)

        success, msg = configure_provider(p_type, key)
        if not success:
            self.setup_feedback_lbl.setText(f"❌ Error: {msg}")
            self.setup_feedback_lbl.setStyleSheet("color: #ef4444; font-weight: bold;")
            self.btn_save_copilot.setEnabled(True)
            return

        self.setup_feedback_lbl.setText(f"✔ {msg} SysPilot Copilot is primed and ready!")
        self.setup_feedback_lbl.setStyleSheet("color: #10b981; font-weight: bold;")
        self.btn_save_copilot.setEnabled(True)

        QTimer.singleShot(1500, self.refresh_copilot_page)

    # --------------------------------------------------------------------------
    # TAB 4: SRE SKILL & PERSONA BLUEPRINT (NEW!)
    # --------------------------------------------------------------------------
    def setup_skill_blueprint_tab(self):
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        content = QWidget()
        layout = QVBoxLayout(content)

        # 1. Architecture & Duality Banner
        arch_card = QFrame()
        arch_card.setProperty("class", "card")
        arch_card.setStyleSheet("background-color: #1e293b; border-left: 5px solid #8b5cf6; padding: 14px; border-radius: 8px;")
        ac_layout = QVBoxLayout(arch_card)

        t = QLabel("🧠 Agent Architecture: Goose Engine + sys-pilot-admin SRE Skill")
        t.setStyleSheet("font-size: 16px; font-weight: bold; color: #a78bfa;")
        ac_layout.addWidget(t)

        desc = QLabel(
            "<b>How SysPilot Operates:</b><br>"
            "SysPilot does <b>NOT</b> run an unconstrained generic chatbot. Instead, it pairs:<br>"
            "• <b>Goose CLI (The Hands / Runner):</b> Manages LLM tool execution, command sandboxing, and terminal I/O.<br>"
            "• <b><code>sys-pilot-admin</code> (The Brain / SRE Architect):</b> Specialized skill built on the battle-tested "
            "<code>eos-admin</code> SRE architecture. It enforces strict safety guardrails, reads local telemetry first (0 tokens), "
            "and follows battle-tested repair playbooks."
        )
        desc.setStyleSheet("color: #cbd5e1; font-size: 13px; line-height: 1.4;")
        ac_layout.addWidget(desc)

        # Badges row
        badges_layout = QHBoxLayout()
        for badge_text, color in [
            ("🛡️ Reversibility First", "#10b981"),
            ("📊 0-Token Local Telemetry", "#38bdf8"),
            ("🚫 No Blind Deletions", "#f59e0b"),
            ("📖 7 SRE Playbooks Loaded", "#a78bfa")
        ]:
            lbl = QLabel(badge_text)
            lbl.setStyleSheet(f"background-color: #0f172a; border: 1px solid {color}; color: {color}; font-size: 11px; font-weight: bold; padding: 4px 10px; border-radius: 12px;")
            badges_layout.addWidget(lbl)
        badges_layout.addStretch()
        ac_layout.addLayout(badges_layout)
        layout.addWidget(arch_card)

        # 2. Local Telemetry & Zero-Token Data Access
        telemetry_card = QFrame()
        telemetry_card.setProperty("class", "card")
        tc_layout = QVBoxLayout(telemetry_card)
        tc_title = QLabel("📊 Telemetry Contract: Zero-Token Pre-Gathered State")
        tc_title.setProperty("class", "sectionTitle")
        tc_layout.addWidget(tc_title)

        tc_desc = QLabel(
            "Before the AI Copilot ever spends a single token, SysPilot automatically interrogates runtime state and feeds it into the skill:<br>"
            "• <code>~/.local/state/syspilot/status.json</code>: Pending updates, core packages, failed systemd units, root/home storage, .pacnew count, GameMode state.<br>"
            "• <code>~/.local/state/system-health/summary.json</code>: OS, kernel version, bootloader sync, initramfs engine, GPU driver, Vulkan multilib readiness."
        )
        tc_desc.setStyleSheet("color: #94a3b8; font-size: 12px; line-height: 1.4;")
        tc_layout.addWidget(tc_desc)

        btn_view_telemetry = QPushButton("📄 Inspect Active Telemetry JSON (0-Token State)")
        btn_view_telemetry.setProperty("class", "secondary")
        btn_view_telemetry.clicked.connect(self.show_telemetry_dialog)
        tc_layout.addWidget(btn_view_telemetry)
        layout.addWidget(telemetry_card)

        # 3. User Custom Directives & Rig Profile (Editable!)
        custom_card = QFrame()
        custom_card.setProperty("class", "card")
        cc_layout = QVBoxLayout(custom_card)
        cc_title = QLabel("⚙️ User Custom Directives & Rig Profile (Your Rig, Your Rules)")
        cc_title.setProperty("class", "sectionTitle")
        cc_layout.addWidget(cc_title)

        cc_desc = QLabel(
            "<b>📍 You are currently in the 'SRE Skill & Persona' tab.</b><br>"
            "Inject your own persistent rules, hardware quirks, or software preferences directly into the <code>sys-pilot-admin</code> skill prompt in the text box below.<br>"
            "<i>(These directives are permanently saved into the skill and strictly honored by the AI Copilot in every session).</i>"
        )
        cc_desc.setStyleSheet("color: #94a3b8; font-size: 12px;")
        cc_layout.addWidget(cc_desc)

        skill_info = get_skill_info()
        self.txt_custom_directives = QTextEdit()
        self.txt_custom_directives.setPlaceholderText("# Add your custom instructions here (e.g. 'Prefer paru over yay', 'My DAC is Focusrite Scarlett', 'Desktop is Hyprland on Wayland')...")
        self.txt_custom_directives.setPlainText(skill_info.get("custom_directives", ""))
        self.txt_custom_directives.setMinimumHeight(110)
        cc_layout.addWidget(self.txt_custom_directives)

        c_act_box = QHBoxLayout()
        self.btn_save_directives = QPushButton("💾 Save Directives to Agent Skill")
        self.btn_save_directives.setProperty("class", "success")
        self.btn_save_directives.clicked.connect(self.save_user_directives)
        c_act_box.addWidget(self.btn_save_directives)

        self.directives_feedback_lbl = QLabel("")
        self.directives_feedback_lbl.setStyleSheet("font-size: 12px; margin-left: 10px;")
        c_act_box.addWidget(self.directives_feedback_lbl)
        c_act_box.addStretch()
        cc_layout.addLayout(c_act_box)

        layout.addWidget(custom_card)

        # 4. Battle-Tested Playbook Arsenal Viewer
        pb_card = QFrame()
        pb_card.setProperty("class", "card")
        pb_layout = QVBoxLayout(pb_card)
        pb_title = QLabel("📖 Battle-Tested SRE Playbook Arsenal (Deterministic Recipes)")
        pb_title.setProperty("class", "sectionTitle")
        pb_layout.addWidget(pb_title)

        pb_desc = QLabel(
            "The <code>sys-pilot-admin</code> skill is pre-equipped with deterministic step-by-step procedures. "
            "Select any playbook below to inspect the exact engineering logic the Copilot follows:"
        )
        pb_desc.setStyleSheet("color: #94a3b8; font-size: 12px;")
        pb_layout.addWidget(pb_desc)

        # Dropdown
        self.pb_combo = QComboBox()
        self.playbooks_data = list_available_playbooks()
        for pb in self.playbooks_data:
            self.pb_combo.addItem(f"📜 {pb['title']} ({pb['filename']})", pb['filename'])
        self.pb_combo.currentIndexChanged.connect(self.on_playbook_selected)
        pb_layout.addWidget(self.pb_combo)

        # Playbook Content Viewer
        self.txt_playbook_viewer = QTextEdit()
        self.txt_playbook_viewer.setReadOnly(True)
        self.txt_playbook_viewer.setMinimumHeight(180)
        if self.playbooks_data:
            initial_content = get_playbook_content(self.playbooks_data[0]['filename'])
            self.txt_playbook_viewer.setPlainText(initial_content)
        pb_layout.addWidget(self.txt_playbook_viewer)

        layout.addWidget(pb_card)

        layout.addStretch()
        scroll.setWidget(content)
        slayout = QVBoxLayout(self.skill_tab)
        slayout.addWidget(scroll)

    def on_playbook_selected(self, index):
        if 0 <= index < len(self.playbooks_data):
            fname = self.playbooks_data[index]['filename']
            content = get_playbook_content(fname)
            self.txt_playbook_viewer.setPlainText(content)

    def save_user_directives(self):
        text = self.txt_custom_directives.toPlainText()
        ok, msg = save_custom_directives(text)
        if ok:
            self.directives_feedback_lbl.setText("✔ Directives saved to sys-pilot-admin!")
            self.directives_feedback_lbl.setStyleSheet("color: #10b981; font-weight: bold;")
            QTimer.singleShot(3000, lambda: self.directives_feedback_lbl.setText(""))
        else:
            self.directives_feedback_lbl.setText(f"❌ Error: {msg}")
            self.directives_feedback_lbl.setStyleSheet("color: #ef4444; font-weight: bold;")

    def show_telemetry_dialog(self):
        dlg = QDialog(self)
        dlg.setWindowTitle("SysPilot — Active Pre-Gathered Telemetry (0 Tokens)")
        dlg.resize(700, 500)
        dlg.setStyleSheet(DARK_STYLESHEET)
        d_layout = QVBoxLayout(dlg)
        
        d_info = QLabel("<b>Raw State JSON:</b> Pre-gathered locally in <1s without root. Fed directly into the Copilot's skill context.")
        d_info.setStyleSheet("color: #38bdf8; font-size: 12px;")
        d_layout.addWidget(d_info)

        txt = QTextEdit()
        txt.setReadOnly(True)
        try:
            with open(STATUS_FILE, "r", encoding="utf-8") as f:
                txt.setPlainText(f.read())
        except Exception as e:
            txt.setPlainText(f"Error loading {STATUS_FILE}: {e}")
        d_layout.addWidget(txt)

        btn_close = QPushButton("Close")
        btn_close.clicked.connect(dlg.accept)
        d_layout.addWidget(btn_close)
        dlg.exec()

    # --------------------------------------------------------------------------
    # TAB 5: SETTINGS & AUTOSTART
    # --------------------------------------------------------------------------
    def setup_settings_tab(self):
        layout = QVBoxLayout(self.settings_tab)

        # Autostart card
        autostart_card = QFrame()
        autostart_card.setProperty("class", "card")
        a_layout = QVBoxLayout(autostart_card)
        a_title = QLabel("🚀 Autostart & Background Sentinel")
        a_title.setProperty("class", "sectionTitle")
        a_layout.addWidget(a_title)

        self.chk_autostart = QCheckBox("Start SysPilot quietly in System Tray at login")
        self.chk_autostart.setChecked(os.path.exists(AUTOSTART_FILE))
        self.chk_autostart.stateChanged.connect(self.toggle_autostart)
        a_layout.addWidget(self.chk_autostart)

        a_layout.addWidget(QLabel("When enabled, SysPilot monitors updates and system health in the system tray, automatically pausing during gaming sessions."))
        layout.addWidget(autostart_card)

        # Model preference card
        model_card = QFrame()
        model_card.setProperty("class", "card")
        m_layout = QVBoxLayout(model_card)
        m_title = QLabel("🧠 Copilot AI Engine & Reasoning Level")
        m_title.setProperty("class", "sectionTitle")
        m_layout.addWidget(m_title)

        m_layout.addWidget(QLabel("Recommended model: <b>Google Gemini 3.8 / 2.5 Flash</b> (Fastest, ultra-lean token cost)."))

        b_reconf = QPushButton("⚙ Launch AI Copilot Setup Wizard")
        b_reconf.setProperty("class", "secondary")
        b_reconf.clicked.connect(lambda: (self.tabs.setCurrentWidget(self.copilot_tab), self.copilot_stack.setCurrentIndex(1)))
        m_layout.addWidget(b_reconf)
        layout.addWidget(model_card)

        layout.addStretch()

    def toggle_autostart(self, state):
        os.makedirs(AUTOSTART_DIR, exist_ok=True)
        if state:
            desktop_entry = f"""[Desktop Entry]
Type=Application
Name=SysPilot
Comment=Autonomous Token-Lean SRE Copilot for Arch Linux
Exec={os.path.join(PROJECT_ROOT, 'bin', 'syspilot')} --tray
Icon=utilities-system-monitor
Terminal=false
Categories=System;Monitor;
X-GNOME-Autostart-enabled=true
"""
            try:
                with open(AUTOSTART_FILE, "w", encoding="utf-8") as f:
                    f.write(desktop_entry)
                QMessageBox.information(self, "Autostart Enabled", "SysPilot will now start in the system tray upon user login.")
            except Exception as e:
                QMessageBox.critical(self, "Error", f"Could not create autostart entry: {e}")
        else:
            if os.path.exists(AUTOSTART_FILE):
                try:
                    os.remove(AUTOSTART_FILE)
                    QMessageBox.information(self, "Autostart Disabled", "SysPilot autostart entry removed.")
                except Exception as e:
                    QMessageBox.critical(self, "Error", f"Could not remove autostart entry: {e}")

    # --------------------------------------------------------------------------
    # DATA BINDING & REFRESH
    # --------------------------------------------------------------------------
    def update_ui_from_state(self):
        """Read status file and refresh all widgets."""
        if not os.path.exists(STATUS_FILE):
            run_triage(check_pkgs=False)

        try:
            with open(STATUS_FILE, "r", encoding="utf-8") as f:
                data = json.load(f)
        except Exception:
            return

        status = data.get("status", "FLIGHT_READY")
        reasons = data.get("status_reasons", [])
        updates = data.get("updates", {})
        standalone = data.get("standalone_software", {})
        disk = data.get("disk", {}).get("root", {})
        failed_services = data.get("failed_services", {})
        pacnew = data.get("pacnew", {})
        gaming_mode = data.get("gaming_mode", False)

        # Header
        if status == "ACTION_REQUIRED":
            self.status_icon_label.setText("🔴")
            self.status_title.setText("System Status: Action Required")
            self.status_title.setStyleSheet("font-size: 18px; font-weight: bold; color: #ef4444;")
            self.status_sub.setText("; ".join(reasons) if reasons else "Critical issue detected.")
        elif status == "PRE_FLIGHT_ATTENTION":
            self.status_icon_label.setText("🟡")
            self.status_title.setText("System Status: Pre-Flight Attention")
            self.status_title.setStyleSheet("font-size: 18px; font-weight: bold; color: #f59e0b;")
            self.status_sub.setText("; ".join(reasons) if reasons else "Updates or maintenance pending.")
        else:
            self.status_icon_label.setText("🟢")
            self.status_title.setText("System Status: Flight Ready")
            self.status_title.setStyleSheet("font-size: 18px; font-weight: bold; color: #10b981;")
            self.status_sub.setText("All core diagnostics pass. System is primed and stable.")

        # Updates card
        tot_up = updates.get("total", 0)
        c_up = updates.get("core_count", 0)
        r_up = updates.get("regular_count", 0)
        self.updates_lbl.setText(f"{tot_up} pending system updates ({c_up} core packages, {r_up} regular packages)")
        
        core_pkgs = updates.get("core_packages", [])
        if core_pkgs:
            self.core_pkgs_lbl.setText(f"Core packages pending: {', '.join([p.split()[0] for p in core_pkgs])}")
            self.core_pkgs_lbl.setStyleSheet("color: #fbbf24; font-size: 12px; font-weight: bold;")
        else:
            self.core_pkgs_lbl.setText("All core packages (kernel, systemd, drivers, bootloader) are up to date.")
            self.core_pkgs_lbl.setStyleSheet("color: #94a3b8; font-size: 12px;")

        # Standalone apps
        if standalone.get("checked", False):
            aur_p = standalone.get("aur_pending", 0)
            fp_p = standalone.get("flatpak_pending", 0)
            g_det = standalone.get("details", {}).get("goose", {})
            g_ver = g_det.get("version", "N/A")
            g_latest = g_det.get("latest", g_ver)
            g_up_avail = g_det.get("update_available", False)
            g_up = f"Update available: v{g_latest}" if g_up_avail else "up to date"
            
            uv_det = standalone.get("details", {}).get("uv", {})
            uv_ver = uv_det.get("version", "N/A")
            uv_up = "Update available" if uv_det.get("update_available") else "up to date"

            self.software_lbl.setText(
                f"AUR: {aur_p} pending | Flatpak: {fp_p} pending | Goose: {g_ver} ({g_up}) | UV: {uv_ver} ({uv_up})"
            )
            if g_up_avail:
                self.btn_update_goose.setText(f"⚡ Update Goose AI Agent ({g_ver} → {g_latest})")
                self.btn_update_goose.setVisible(True)
            else:
                self.btn_update_goose.setVisible(False)
        else:
            self.software_lbl.setText("Standalone apps triage pending. Click below to inspect.")

        # Services
        sys_f = failed_services.get("system", [])
        usr_f = failed_services.get("user", [])
        if sys_f or usr_f:
            f_str = []
            if sys_f: f_str.append(f"System: {', '.join(sys_f)}")
            if usr_f: f_str.append(f"User: {', '.join(usr_f)}")
            self.services_lbl.setText(f"⚠️ Failed Units Detected: {' | '.join(f_str)}")
            self.services_lbl.setStyleSheet("color: #ef4444; font-weight: bold;")
        else:
            self.services_lbl.setText("✔ Systemd Units: All system and user units operational.")
            self.services_lbl.setStyleSheet("color: #10b981; font-weight: bold;")

        # Disk
        used_pct = disk.get("used_pct", 0)
        avail_gb = disk.get("avail_gb", 0)
        self.disk_bar.setValue(used_pct)
        self.disk_bar.setFormat(f"%v% used")
        self.disk_sub_lbl.setText(f"{avail_gb} GB free on root mount (/)")

        # Pacnew
        p_count = pacnew.get("count", 0)
        if p_count > 0:
            self.pacnew_desc_lbl.setText(f"⚠️ {p_count} .pacnew configuration file(s) require review to prevent service deprecations.")
            self.pacnew_desc_lbl.setStyleSheet("color: #f59e0b; font-weight: bold;")
        else:
            self.pacnew_desc_lbl.setText("✔ No .pacnew configuration conflicts detected.")
            self.pacnew_desc_lbl.setStyleSheet("color: #10b981;")

        # Gaming
        if gaming_mode:
            self.gaming_lbl.setText("🎮 GameMode: ACTIVE (Background diagnostics inhibited)")
            self.gaming_lbl.setStyleSheet("color: #38bdf8; font-weight: bold;")
        else:
            self.gaming_lbl.setText("🎮 GameMode: Inactive (Normal desktop operation)")
            self.gaming_lbl.setStyleSheet("color: #94a3b8;")

        # Update tray icon
        self.tray_app.update_tray_icon(status, gaming_mode)

    def trigger_refresh(self):
        self.refresh_btn.setEnabled(False)
        self.refresh_btn.setText("Scanning...")
        threading.Thread(target=self._run_bg_refresh, daemon=True).start()

    def _run_bg_refresh(self):
        run_triage(check_pkgs=True)
        QTimer.singleShot(0, self._on_refresh_finished)

    def _on_refresh_finished(self):
        self.update_ui_from_state()
        self.refresh_btn.setEnabled(True)
        self.refresh_btn.setText("↻ Refresh Triage")

    # --------------------------------------------------------------------------
    # TERMINAL RUNNERS
    # --------------------------------------------------------------------------
    def run_guarded_upgrade_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --upgrade"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Guarded Upgrade")
        subprocess.Popen(term_cmd)

    def run_software_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --software"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Standalone Software Triage")
        subprocess.Popen(term_cmd)

    def run_update_goose_terminal(self):
        cmd = (
            "echo '⚡ Updating Goose AI Agent...'; "
            "if type -P goose &>/dev/null && goose update --help &>/dev/null; then "
            "  goose update; "
            "elif type -P yay &>/dev/null; then "
            "  yay -S --needed goose-cli; "
            "elif type -P paru &>/dev/null; then "
            "  paru -S --needed goose-cli; "
            "else "
            "  curl -fsSL https://github.com/block/goose/releases/download/stable/download_cli.sh | bash; "
            "fi"
        )
        term_cmd = get_terminal_cmd(cmd, "SysPilot Goose AI Agent Update")
        subprocess.Popen(term_cmd)

    def run_maintenance_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --maintenance"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Safe Maintenance")
        subprocess.Popen(term_cmd)

    def run_audit_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --audit"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Full Audit")
        subprocess.Popen(term_cmd)

    def run_custom_terminal(self, command: str, title: str):
        term_cmd = get_terminal_cmd(command, title)
        subprocess.Popen(term_cmd)

    def open_copilot_terminal(self):
        recipe_path = os.path.join(PROJECT_ROOT, "copilot", "recipe.yaml")
        cmd = f"goose run --recipe '{recipe_path}' -s"
        term_cmd = get_terminal_cmd(cmd, "SysPilot AI Copilot Interactive")
        subprocess.Popen(term_cmd)

    # --------------------------------------------------------------------------
    # COPILOT EXECUTION
    # --------------------------------------------------------------------------
    def submit_custom_query(self):
        query = self.query_input.text().strip()
        if query:
            self.ask_copilot(query)
            self.query_input.clear()

    def ask_copilot(self, query: str):
        ready, _ = is_copilot_ready()
        if not ready:
            self.tabs.setCurrentWidget(self.copilot_tab)
            self.copilot_stack.setCurrentIndex(1)
            return

        self.tabs.setCurrentWidget(self.copilot_tab)
        self.copilot_stack.setCurrentIndex(0)
        self.copilot_output.clear()
        self.btn_ask.setEnabled(False)

        self.thread = threading.Thread(target=self._exec_copilot_worker, args=(query,), daemon=True)
        self.thread.start()

    def _exec_copilot_worker(self, query: str):
        worker = CopilotWorker(query)
        worker.output_signal.connect(self._append_copilot_output)
        worker.finished_signal.connect(lambda: self.btn_ask.setEnabled(True))
        worker.run()

    def _append_copilot_output(self, text: str):
        self.copilot_output.moveCursor(self.copilot_output.textCursor().MoveOperation.End)
        self.copilot_output.insertPlainText(text)
        self.copilot_output.moveCursor(self.copilot_output.textCursor().MoveOperation.End)

    def closeEvent(self, event):
        """Minimize to tray on window close instead of exiting completely."""
        event.ignore()
        self.hide()
        self.tray_app.tray_icon.showMessage(
            "SysPilot",
            "SysPilot is still guarding your system in the tray.",
            QSystemTrayIcon.MessageIcon.Information,
            2000
        )


class SysPilotApp:
    """System Tray Application Controller."""

    def __init__(self):
        self.app = QApplication.instance() or QApplication(sys.argv)
        self.app.setQuitOnLastWindowClosed(False)

        # Icons
        self.icon_green = make_status_icon("#10b981", "✔")
        self.icon_yellow = make_status_icon("#f59e0b", "!")
        self.icon_red = make_status_icon("#ef4444", "✖")
        self.icon_game = make_status_icon("#38bdf8", "🎮")

        # Tray Icon setup
        self.tray_icon = QSystemTrayIcon()
        self.tray_icon.setIcon(self.icon_green)
        self.tray_icon.setToolTip("SysPilot — Flight Ready")

        # Menu
        self.menu = QMenu()
        self.action_show = self.menu.addAction("✈ Open SysPilot Dashboard")
        self.action_show.triggered.connect(self.show_window)

        self.action_refresh = self.menu.addAction("↻ Quick Health Triage")
        self.action_refresh.triggered.connect(self.quick_triage)

        self.action_maintenance = self.menu.addAction("🛠 System Maintenance")
        self.action_maintenance.triggered.connect(self.open_maintenance)

        self.action_copilot = self.menu.addAction("🤖 Ask AI Copilot")
        self.action_copilot.triggered.connect(self.open_copilot)

        self.action_skill = self.menu.addAction("🧠 SRE Skill & Persona")
        self.action_skill.triggered.connect(self.open_skill_blueprint)

        self.menu.addSeparator()
        self.action_quit = self.menu.addAction("❌ Quit SysPilot")
        self.action_quit.triggered.connect(self.quit_app)

        self.tray_icon.setContextMenu(self.menu)
        self.tray_icon.activated.connect(self.on_tray_activated)
        self.tray_icon.show()

        # Window
        self.window = SysPilotWindow(self)

        # Periodic Timer (checks state every 5 minutes in memory)
        self.timer = QTimer()
        self.timer.timeout.connect(self.periodic_check)
        self.timer.start(300000)

    def update_tray_icon(self, status: str, gaming: bool):
        if gaming:
            self.tray_icon.setIcon(self.icon_game)
            self.tray_icon.setToolTip("SysPilot — Gaming Mode Active")
        elif status == "ACTION_REQUIRED":
            self.tray_icon.setIcon(self.icon_red)
            self.tray_icon.setToolTip("SysPilot — Action Required ⚠️")
        elif status == "PRE_FLIGHT_ATTENTION":
            self.tray_icon.setIcon(self.icon_yellow)
            self.tray_icon.setToolTip("SysPilot — Attention Needed ℹ️")
        else:
            self.tray_icon.setIcon(self.icon_green)
            self.tray_icon.setToolTip("SysPilot — Flight Ready ✔")

    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.show_window()

    def show_window(self):
        self.window.show()
        self.window.raise_()
        self.window.activateWindow()

    def quick_triage(self):
        self.window.trigger_refresh()

    def open_maintenance(self):
        self.show_window()
        self.window.tabs.setCurrentWidget(self.window.maintenance_tab)

    def open_copilot(self):
        self.show_window()
        self.window.tabs.setCurrentWidget(self.window.copilot_tab)

    def open_skill_blueprint(self):
        self.show_window()
        self.window.tabs.setCurrentWidget(self.window.skill_tab)

    def periodic_check(self):
        if not is_gamemode_active():
            run_triage(check_pkgs=False)
            self.window.update_ui_from_state()

    def quit_app(self):
        self.tray_icon.hide()
        self.app.quit()

    def run(self, start_minimized: bool = False):
        if not start_minimized:
            self.show_window()
        return self.app.exec()


if __name__ == "__main__":
    start_min = "--tray" in sys.argv
    app = SysPilotApp()
    sys.exit(app.run(start_minimized=start_min))
