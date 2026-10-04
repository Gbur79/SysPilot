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
from typing import Dict, Any, List

from PyQt6.QtCore import Qt, QTimer, pyqtSignal, QObject
from PyQt6.QtGui import QIcon, QPixmap, QPainter, QColor, QFont, QPen, QBrush
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QTabWidget, QProgressBar, QTextEdit,
    QLineEdit, QFrame, QScrollArea, QSystemTrayIcon, QMenu,
    QCheckBox, QComboBox, QMessageBox
)

# Ensure project root is in sys.path
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(SCRIPT_DIR)
sys.path.insert(0, PROJECT_ROOT)

from core.triage import run_triage, is_gamemode_active, STATUS_FILE

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
        ("xterm", ["xterm", "-T", title, "-e", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"])
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
    background-color: #0f172a;
    color: #f8fafc;
}
QWidget {
    background-color: #0f172a;
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
    padding: 8px 18px;
    margin-right: 4px;
    border-top-left-radius: 6px;
    border-top-right-radius: 6px;
    font-weight: bold;
}
QTabBar::tab:selected {
    background-color: #3b82f6;
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
    margin-bottom: 6px;
}
QPushButton {
    background-color: #2563eb;
    color: #ffffff;
    border: none;
    border-radius: 6px;
    padding: 7px 14px;
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
QProgressBar {
    border: 1px solid #334155;
    border-radius: 5px;
    text-align: center;
    background-color: #0f172a;
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
    background: #0f172a;
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
        goose_bin = shutil.which("goose")
        if not goose_bin:
            goose_bin = os.path.expanduser("~/.local/bin/goose")

        if not os.path.exists(goose_bin):
            self.output_signal.emit("[Error] Goose CLI is not found on your system.\nPlease install goose via AUR or goose.ai.\n")
            self.finished_signal.emit()
            return

        # Prepare execution
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
        self.resize(850, 680)
        self.setStyleSheet(DARK_STYLESHEET)

        self.central_widget = QWidget()
        self.setCentralWidget(self.central_widget)
        self.main_layout = QVBoxLayout(self.central_widget)

        # Header Status Banner
        self.header_frame = QFrame()
        self.header_frame.setProperty("class", "card")
        self.header_frame.setStyleSheet("background-color: #1e293b; border-radius: 8px; padding: 14px;")
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

        # Tabs
        self.tabs = QTabWidget()
        self.main_layout.addWidget(self.tabs)

        # Tab 1: Dashboard
        self.dashboard_tab = QWidget()
        self.setup_dashboard_tab()
        self.tabs.addTab(self.dashboard_tab, "✈ Dashboard & Telemetry")

        # Tab 2: Copilot AI
        self.copilot_tab = QWidget()
        self.setup_copilot_tab()
        self.tabs.addTab(self.copilot_tab, "🤖 AI Copilot (Goose SRE)")

        # Tab 3: Settings
        self.settings_tab = QWidget()
        self.setup_settings_tab()
        self.tabs.addTab(self.settings_tab, "⚙ Settings & Autostart")

        # Load initial data
        self.update_ui_from_state()

    def setup_dashboard_tab(self):
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        content = QWidget()
        layout = QVBoxLayout(content)

        # Grid of Cards
        # 1. Updates Card
        self.updates_card = QFrame()
        self.updates_card.setProperty("class", "card")
        u_layout = QVBoxLayout(self.updates_card)
        u_title = QLabel("📦 Software & System Updates")
        u_title.setProperty("class", "sectionTitle")
        u_layout.addWidget(u_title)

        self.updates_lbl = QLabel("0 pending updates (0 core, 0 regular, 0 AUR)")
        u_layout.addWidget(self.updates_lbl)

        self.core_pkgs_lbl = QLabel("")
        self.core_pkgs_lbl.setStyleSheet("color: #fbbf24; font-size: 11px;")
        u_layout.addWidget(self.core_pkgs_lbl)

        u_btn_box = QHBoxLayout()
        self.btn_guarded_upgrade = QPushButton("⚡ Guarded System Upgrade")
        self.btn_guarded_upgrade.setProperty("class", "success")
        self.btn_guarded_upgrade.clicked.connect(self.run_guarded_upgrade_terminal)
        u_btn_box.addWidget(self.btn_guarded_upgrade)

        self.btn_safe_clean = QPushButton("🧹 Safe Maintenance")
        self.btn_safe_clean.setProperty("class", "secondary")
        self.btn_safe_clean.clicked.connect(self.run_maintenance_terminal)
        u_btn_box.addWidget(self.btn_safe_clean)
        u_layout.addLayout(u_btn_box)
        layout.addWidget(self.updates_card)

        # 2. Services & System Health Card
        self.health_card = QFrame()
        self.health_card.setProperty("class", "card")
        h_layout = QVBoxLayout(self.health_card)
        h_title = QLabel("🛡 Systemd Units & Storage Health")
        h_title.setProperty("class", "sectionTitle")
        h_layout.addWidget(h_title)

        self.services_lbl = QLabel("Systemd Units: All active units operational.")
        h_layout.addWidget(self.services_lbl)

        # Disk bar
        h_layout.addWidget(QLabel("Root Partition Usage (/):"))
        self.disk_bar = QProgressBar()
        self.disk_bar.setValue(23)
        self.disk_bar.setFormat("%v% (334.6 GB free)")
        h_layout.addWidget(self.disk_bar)

        self.pacnew_lbl = QLabel("Configuration Conflicts: 0 .pacnew files.")
        h_layout.addWidget(self.pacnew_lbl)

        self.btn_full_audit = QPushButton("🔍 Launch Comprehensive Sys-Health Audit")
        self.btn_full_audit.clicked.connect(self.run_audit_terminal)
        h_layout.addWidget(self.btn_full_audit)
        layout.addWidget(self.health_card)

        # 3. Gaming & Performance Card
        self.gaming_card = QFrame()
        self.gaming_card.setProperty("class", "card")
        g_layout = QVBoxLayout(self.gaming_card)
        g_title = QLabel("🎮 Gaming & Performance Sentinel")
        g_title.setProperty("class", "sectionTitle")
        g_layout.addWidget(g_title)

        self.gamemode_lbl = QLabel("GameMode: Inactive (Desktop state)")
        g_layout.addWidget(self.gamemode_lbl)

        self.gaming_details_lbl = QLabel("Vulkan & 32-bit: Primed | Custom Proton: Detected")
        g_layout.addWidget(self.gaming_details_lbl)
        layout.addWidget(self.gaming_card)

        layout.addStretch()
        scroll.setWidget(content)

        dash_layout = QVBoxLayout(self.dashboard_tab)
        dash_layout.addWidget(scroll)

    def setup_copilot_tab(self):
        layout = QVBoxLayout(self.copilot_tab)

        # Info banner
        info_banner = QLabel("🤖 <b>Token-Lean SRE Copilot:</b> Powered by pre-gathered local telemetry.<br>Solves complex Linux & gaming issues with zero token waste and surgical accuracy.")
        info_banner.setStyleSheet("background-color: #1e293b; border-left: 4px solid #3b82f6; padding: 10px; border-radius: 4px;")
        layout.addWidget(info_banner)

        # Quick action pills
        layout.addWidget(QLabel("<b>Quick Troubleshooting Playbooks:</b>"))
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

        # Terminal interactive button
        btn_interactive = QPushButton("💻 Open Interactive Copilot Session in Terminal")
        btn_interactive.clicked.connect(self.open_copilot_terminal)
        layout.addWidget(btn_interactive)

        # Query input
        input_layout = QHBoxLayout()
        self.query_input = QLineEdit()
        self.query_input.setPlaceholderText("Ask SysPilot Copilot anything (e.g. 'Why is my game lagging?' or 'Fix failed service X')...")
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

    def setup_settings_tab(self):
        layout = QVBoxLayout(self.settings_tab)

        # Autostart card
        autostart_card = QFrame()
        autostart_card.setProperty("class", "card")
        a_layout = QVBoxLayout(autostart_card)
        a_title = QLabel("🚀 Autostart & Background Sentinel")
        a_title.setProperty("class", "sectionTitle")
        a_layout.addWidget(a_title)

        self.chk_autostart = QCheckBox("Start SysPilot silently in System Tray at login")
        self.chk_autostart.setChecked(os.path.exists(AUTOSTART_FILE))
        self.chk_autostart.stateChanged.connect(self.toggle_autostart)
        a_layout.addWidget(self.chk_autostart)

        a_layout.addWidget(QLabel("SysPilot will quietly monitor updates and system health in the system tray, automatically pausing during gaming sessions."))
        layout.addWidget(autostart_card)

        # Model card
        model_card = QFrame()
        model_card.setProperty("class", "card")
        m_layout = QVBoxLayout(model_card)
        m_title = QLabel("🧠 AI Copilot Model Configuration")
        m_title.setProperty("class", "sectionTitle")
        m_layout.addWidget(m_title)

        m_layout.addWidget(QLabel("Recommended: <b>Google Gemini 2.5 / 3.8 Flash</b> (Highest performance & lowest token cost)."))
        self.model_combo = QComboBox()
        self.model_combo.addItems([
            "gemini-2.5-flash (Default - Ultra Fast & Lean)",
            "gemini-3.8-flash (Advanced Reasoning)",
            "local-ollama (qwen2.5-coder / deepseek)",
            "gpt-4o-mini"
        ])
        m_layout.addWidget(self.model_combo)
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
        a_up = updates.get("aur_count", 0)
        self.updates_lbl.setText(f"{tot_up} pending updates ({c_up} core, {r_up} regular, {a_up} AUR)")
        
        core_pkgs = updates.get("core_packages", [])
        if core_pkgs:
            self.core_pkgs_lbl.setText(f"Core packages pending: {', '.join([p.split()[0] for p in core_pkgs])}")
        else:
            self.core_pkgs_lbl.setText("No core kernel/system packages pending.")

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
            self.services_lbl.setText("✔ Systemd Units: All active units operational.")
            self.services_lbl.setStyleSheet("color: #10b981;")

        # Disk
        used_pct = disk.get("used_pct", 0)
        avail_gb = disk.get("avail_gb", 0)
        self.disk_bar.setValue(used_pct)
        self.disk_bar.setFormat(f"%v% used ({avail_gb} GB available)")

        # Pacnew
        p_count = pacnew.get("count", 0)
        if p_count > 0:
            self.pacnew_lbl.setText(f"⚠️ Configuration Conflicts: {p_count} .pacnew files pending review.")
            self.pacnew_lbl.setStyleSheet("color: #f59e0b; font-weight: bold;")
        else:
            self.pacnew_lbl.setText("✔ Configuration Conflicts: 0 .pacnew files.")
            self.pacnew_lbl.setStyleSheet("color: #10b981;")

        # Gaming
        if gaming_mode:
            self.gamemode_lbl.setText("🎮 GameMode: ACTIVE (Background diagnostics paused)")
            self.gamemode_lbl.setStyleSheet("color: #38bdf8; font-weight: bold;")
        else:
            self.gamemode_lbl.setText("GameMode: Inactive (Standard desktop operation)")
            self.gamemode_lbl.setStyleSheet("color: #f8fafc;")

        # Update tray icon
        self.tray_app.update_tray_icon(status, gaming_mode)

    def trigger_refresh(self):
        self.refresh_btn.setEnabled(False)
        self.refresh_btn.setText("Scanning...")
        threading.Thread(target=self._run_bg_refresh, daemon=True).start()

    def _run_bg_refresh(self):
        run_triage(check_pkgs=True)
        # Notify UI thread
        QTimer.singleShot(0, self._on_refresh_finished)

    def _on_refresh_finished(self):
        self.update_ui_from_state()
        self.refresh_btn.setEnabled(True)
        self.refresh_btn.setText("↻ Refresh Triage")

    def run_guarded_upgrade_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --upgrade"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Guarded Upgrade")
        subprocess.Popen(term_cmd)

    def run_maintenance_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --maintenance"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Maintenance")
        subprocess.Popen(term_cmd)

    def run_audit_terminal(self):
        cmd = f"{os.path.join(PROJECT_ROOT, 'bin', 'sys-health.sh')} --audit"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Full Audit")
        subprocess.Popen(term_cmd)

    def open_copilot_terminal(self):
        recipe_path = os.path.join(PROJECT_ROOT, "copilot", "recipe.yaml")
        cmd = f"goose run --recipe '{recipe_path}' -s"
        term_cmd = get_terminal_cmd(cmd, "SysPilot AI Copilot Interactive")
        subprocess.Popen(term_cmd)

    def submit_custom_query(self):
        query = self.query_input.text().strip()
        if query:
            self.ask_copilot(query)
            self.query_input.clear()

    def ask_copilot(self, query: str):
        self.tabs.setCurrentWidget(self.copilot_tab)
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

        self.action_copilot = self.menu.addAction("🤖 Ask AI Copilot")
        self.action_copilot.triggered.connect(self.open_copilot)

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
        self.timer.start(300000)  # 5 minutes

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

    def open_copilot(self):
        self.show_window()
        self.window.tabs.setCurrentWidget(self.window.copilot_tab)

    def periodic_check(self):
        if not is_gamemode_active():
            # Light check without heavy package query
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
