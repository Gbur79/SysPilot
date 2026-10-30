#!/usr/bin/env python3
"""
SysPilot Desktop Dashboard & System Tray
Autonomous Token-Lean SRE Copilot for Arch Linux & derivatives.
"""

import os
import sys
import re
import json
import time
import shutil
import subprocess
import threading
import fcntl
from typing import Dict, Any, List, Optional, Tuple

from PyQt6.QtCore import Qt, QTimer, pyqtSignal, QObject, QUrl, QEvent
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

from core.triage import run_triage, is_gamemode_active, STATUS_FILE, SYS_HEALTH_SUMMARY
from core.copilot_config import (
    is_goose_installed, is_copilot_ready, get_configured_providers,
    configure_provider, find_goose_binary, get_skill_info,
    save_custom_directives, list_available_playbooks, get_playbook_content,
    get_active_model_details, get_user_copilot_config
)
from PyQt6.QtNetwork import QLocalServer, QLocalSocket

AUTOSTART_DIR = os.path.expanduser("~/.config/autostart")
AUTOSTART_FILE = os.path.join(AUTOSTART_DIR, "syspilot.desktop")


def get_syspilot_bin() -> str:
    """Resolve the SysPilot CLI router executable path."""
    user_bin = os.path.expanduser("~/.local/bin/syspilot")
    if os.path.isfile(user_bin) and os.access(user_bin, os.X_OK):
        return user_bin
    proj_bin = os.path.join(PROJECT_ROOT, "bin", "syspilot")
    if os.path.isfile(proj_bin) and os.access(proj_bin, os.X_OK):
        return proj_bin
    which_bin = shutil.which("syspilot")
    if which_bin:
        return which_bin
    return "syspilot"


def get_terminal_cmd(command_to_run: str, title: str = "SysPilot") -> List[str]:
    """Find available terminal and return command list to execute in terminal."""
    terminals = [
        ("konsole", ["konsole", "--nofork", "-e", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("alacritty", ["alacritty", "-e", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("kitty", ["kitty", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
        ("xfce4-terminal", ["xfce4-terminal", "--disable-server", "-e", f"bash -c \"{command_to_run}; echo ''; read -p 'Press Enter to close...'\""]),
        ("gnome-terminal", ["gnome-terminal", "--wait", "--", "bash", "-c", f"{command_to_run}; echo ''; read -p 'Press Enter to close...'"]),
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


STYLE_BADGE_PASS = "background-color: #07271a; border: 1px solid #065f46; color: #4ade80; font-weight: bold; font-size: 11px; padding: 2px 8px; border-radius: 6px; font-family: 'Hack', monospace;"
STYLE_BADGE_WARN = "background-color: #2b1704; border: 1px solid #92400e; color: #fbbf24; font-weight: bold; font-size: 11px; padding: 2px 8px; border-radius: 6px; font-family: 'Hack', monospace;"
STYLE_BADGE_FAIL = "background-color: #2b0e12; border: 1px solid #991b1b; color: #f87171; font-weight: bold; font-size: 11px; padding: 2px 8px; border-radius: 6px; font-family: 'Hack', monospace;"
STYLE_BADGE_INFO = "background-color: #0c2838; border: 1px solid #0284c7; color: #38bdf8; font-weight: bold; font-size: 11px; padding: 2px 8px; border-radius: 6px; font-family: 'Hack', monospace;"


DARK_STYLESHEET = """
QMainWindow {
    background-color: #0b0f17;
    color: #ffffff;
    font-family: 'Hack', 'DejaVu Sans Mono', 'Noto Sans Mono', monospace;
}
QWidget {
    background-color: #0b0f17;
    color: #ffffff;
    font-family: 'Hack', 'DejaVu Sans Mono', 'Noto Sans Mono', monospace;
    font-size: 13px;
}
QTabWidget::pane {
    border: none;
    background-color: transparent;
    padding: 0px;
    margin-top: 6px;
}
QTabBar::tab {
    background-color: #121824;
    color: #94a3b8;
    padding: 8px 16px;
    margin-right: 6px;
    border-radius: 6px;
    font-family: 'Hack', monospace;
    font-weight: bold;
    font-size: 12px;
    border: 1px solid #1c2636;
}
QTabBar::tab:selected {
    background-color: #1a2332;
    color: #fbbf24;
    border: 1px solid #d97706;
}
QTabBar::tab:hover:!selected {
    background-color: #182232;
    color: #ffffff;
    border-color: #283548;
}
QFrame.card {
    background-color: #111722;
    border: 1px solid #1a2332;
    border-radius: 8px;
    padding: 12px;
}
QFrame.gridTable {
    background-color: #0d121c;
    border: 1px solid #17202e;
    border-radius: 6px;
}
QFrame.flatHeader {
    background-color: transparent;
    border: none;
    padding: 2px 0px 6px 0px;
}
QLabel.sectionTitle {
    font-size: 13px;
    font-weight: bold;
    color: #fbbf24;
    font-family: 'Hack', monospace;
    letter-spacing: 0.5px;
}
QPushButton {
    background-color: #1a2332;
    color: #e2e8f0;
    border: 1px solid #2d3748;
    border-radius: 6px;
    padding: 7px 16px;
    font-weight: bold;
    font-family: 'Hack', monospace;
    font-size: 12px;
}
QPushButton:hover {
    background-color: #243044;
    color: #ffffff;
    border-color: #4a5568;
}
QPushButton:pressed {
    background-color: #0f172a;
}
QPushButton.primaryAction {
    background-color: #10b981;
    color: #042f1a;
    border: 1px solid #059669;
    border-radius: 6px;
    padding: 8px 18px;
    font-size: 12px;
    font-weight: bold;
}
QPushButton.primaryAction:hover {
    background-color: #34d399;
    color: #000000;
}
QPushButton.ghostAction {
    background-color: transparent;
    color: #94a3b8;
    border: 1px solid #2d3748;
    border-radius: 6px;
    padding: 7px 16px;
    font-size: 12px;
    font-weight: bold;
}
QPushButton.ghostAction:hover {
    background-color: #17202e;
    color: #f8fafc;
    border-color: #4b5563;
}
QPushButton.pillWarning {
    background-color: #271705;
    color: #fbbf24;
    border: 1px solid #92400e;
    border-radius: 6px;
    padding: 4px 12px;
    font-size: 11px;
    font-weight: bold;
}
QPushButton.pillWarning:hover {
    background-color: #3d2408;
    color: #fef08a;
    border-color: #b45309;
}
QPushButton.secondary {
    background-color: transparent;
    color: #94a3b8;
    border: 1px solid #2d3748;
    border-radius: 6px;
    padding: 7px 16px;
    font-size: 12px;
    font-weight: bold;
}
QPushButton.secondary:hover {
    background-color: #17202e;
    color: #f8fafc;
    border-color: #4b5563;
}
QPushButton.success {
    background-color: #10b981;
    color: #042f1a;
    border: 1px solid #059669;
    border-radius: 6px;
    padding: 8px 18px;
    font-size: 12px;
    font-weight: bold;
}
QPushButton.success:hover {
    background-color: #34d399;
    color: #000000;
}
QPushButton.warning {
    background-color: #271705;
    color: #fbbf24;
    border: 1px solid #92400e;
    border-radius: 6px;
    padding: 4px 12px;
    font-size: 11px;
    font-weight: bold;
}
QPushButton.warning:hover {
    background-color: #3d2408;
    color: #ffffff;
    border-color: #b45309;
}
QPushButton.purple {
    background-color: #082438;
    color: #38bdf8;
    border: 1px solid #0284c7;
    border-radius: 6px;
}
QPushButton.purple:hover {
    background-color: #0284c7;
    color: #ffffff;
}
QProgressBar {
    border: 1px solid #1c2636;
    border-radius: 4px;
    text-align: center;
    background-color: #0d121c;
    color: #ffffff;
    font-family: 'Hack', monospace;
    font-size: 11px;
    font-weight: bold;
}
QProgressBar::chunk {
    background-color: #2563eb;
    border-radius: 3px;
}
QTextEdit, QLineEdit {
    background-color: #0d121c;
    border: 1px solid #1c2636;
    border-radius: 6px;
    color: #f8fafc;
    padding: 8px;
    font-family: 'Hack', 'DejaVu Sans Mono', monospace;
    font-size: 12px;
}
QTextEdit:focus, QLineEdit:focus {
    border: 1px solid #fbbf24;
}
QScrollBar:vertical {
    border: none;
    background: #0b0f17;
    width: 8px;
    border-radius: 4px;
}
QScrollBar::handle:vertical {
    background: #1e293b;
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
        user_conf = get_user_copilot_config()
        recipe_path = user_conf.get("SYSPILOT_GOOSE_RECIPE")
        if not recipe_path or not os.path.isfile(recipe_path):
            recipe_path = os.path.join(PROJECT_ROOT, "copilot", "recipe.yaml")

        goose_bin = find_goose_binary()

        if not goose_bin:
            self.output_signal.emit("[Error] Goose CLI is not found on your system.\nPlease install goose via AUR or configure custom copilot in ~/.config/syspilot/config.env.\n")
            self.finished_signal.emit()
            return

        model_info = get_active_model_details()
        model_name = model_info.get("model", "unknown")
        provider_name = model_info.get("provider_display", "unknown")

        self.output_signal.emit(
            f"🚀 Calling SysPilot Copilot [{model_name} · {provider_name}]\n"
            f"Query: \"{self.query}\"\n"
            f"(Token-Lean mode: reading local telemetry & playbooks...)\n"
            f"━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n"
        )

        try:
            cmd = [
                goose_bin, "run",
                "--recipe", recipe_path,
                "--params", f"user_query={self.query}",
                "--output-format", "stream-json",
                "--no-session"
            ]
            process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                cwd=PROJECT_ROOT
            )
            
            header_shown = False
            raw_fallback_lines = []
            json_parsed_any = False

            for line in process.stdout:
                line_str = line.strip()
                if not line_str.startswith("{"):
                    raw_fallback_lines.append(line)
                    continue
                try:
                    data = json.loads(line_str)
                    dtype = data.get("type")
                    if dtype == "message":
                        msg = data.get("message", {})
                        for c in msg.get("content", []):
                            ctype = c.get("type")
                            if ctype == "toolRequest":
                                json_parsed_any = True
                                tool_call = c.get("toolCall", {}).get("value", {})
                                name = tool_call.get("name", "tool")
                                args = tool_call.get("arguments", {})
                                cmd_str = args.get("command") or args.get("name") or str(args)
                                if len(cmd_str) > 75:
                                    cmd_str = cmd_str[:72] + "..."
                                self.output_signal.emit(f"⚙️ [Diagnostic Check] {name}: {cmd_str}\n")
                            elif ctype == "text":
                                json_parsed_any = True
                                if not header_shown:
                                    self.output_signal.emit("\n💬 Copilot Response:\n")
                                    header_shown = True
                                self.output_signal.emit(c.get("text", ""))
                    elif dtype == "complete":
                        json_parsed_any = True
                        tokens = data.get("total_tokens", 0)
                        cost = data.get("cost_usd", 0.0)
                        self.output_signal.emit(
                            f"\n\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n"
                            f"✔ Analysis Complete | Model: {model_name} | Total Tokens: {tokens:,} | Cost: ${cost:.4f}\n"
                        )
                except Exception:
                    raw_fallback_lines.append(line)

            process.wait()

            if not json_parsed_any and raw_fallback_lines:
                for rl in raw_fallback_lines:
                    self.output_signal.emit(rl)
                self.output_signal.emit("\n✔ Copilot process ended.\n")

        except Exception as e:
            self.output_signal.emit(f"\n[Error executing Copilot]: {str(e)}\n")

        self.finished_signal.emit()


def get_system_uptime() -> str:
    """Read human-readable system uptime."""
    try:
        with open("/proc/uptime", "r") as f:
            secs = float(f.readline().split()[0])
        hours = int(secs // 3600)
        mins = int((secs % 3600) // 60)
        if hours > 0:
            return f"{hours}h {mins}m"
        return f"{mins}m"
    except Exception:
        return "N/A"


def create_grid_header() -> QFrame:
    """Create minimalist flat table column header matching modern UI guidelines."""
    hdr_box = QFrame()
    hdr_box.setStyleSheet("background-color: #111723; border-bottom: 1px solid #1c2636; border-top-left-radius: 6px; border-top-right-radius: 6px;")
    h_l = QHBoxLayout(hdr_box)
    h_l.setContentsMargins(14, 8, 14, 8)

    col1 = QLabel("Component")
    col1.setFixedWidth(270)
    col1.setStyleSheet("color: #64748b; font-weight: bold; font-size: 11px; font-family: 'Hack', monospace;")
    h_l.addWidget(col1)

    col2 = QLabel("Status / Diagnostic Telemetry")
    col2.setStyleSheet("color: #64748b; font-weight: bold; font-size: 11px; font-family: 'Hack', monospace; margin-left: 12px;")
    h_l.addWidget(col2)
    h_l.addStretch()
    return hdr_box


def create_grid_row(comp_name: str, val_label: QLabel, is_alt: bool, has_bottom_border: bool = True, extra_widget: Optional[QWidget] = None) -> QFrame:
    """Create modern flat table row with subtle whitespace separation."""
    row_box = QFrame()
    bg = "#111724" if is_alt else "#0d131f"
    border_b = "border-bottom: 1px solid #182232;" if has_bottom_border else ""
    border_rad = "border-bottom-left-radius: 6px; border-bottom-right-radius: 6px;" if not has_bottom_border else ""
    row_box.setStyleSheet(f"background-color: {bg}; {border_b} {border_rad}")
    r_l = QHBoxLayout(row_box)
    r_l.setContentsMargins(14, 7, 14, 7)

    c_lbl = QLabel(comp_name)
    c_lbl.setFixedWidth(270)
    c_lbl.setStyleSheet("color: #e2e8f0; font-weight: bold; font-size: 13px; font-family: 'Hack', monospace;")
    r_l.addWidget(c_lbl)

    val_label.setStyleSheet("font-family: 'Hack', monospace; font-size: 13px; margin-left: 12px;")
    r_l.addWidget(val_label)

    if extra_widget:
        r_l.addSpacing(10)
        r_l.addWidget(extra_widget)

    r_l.addStretch()
    return row_box


class SysPilotWindow(QMainWindow):
    """Main Dashboard Window."""

    terminal_finished = pyqtSignal()

    def __init__(self, tray_app):
        super().__init__()
        self.tray_app = tray_app
        self.standalone_mode = (tray_app is None)
        self.setWindowTitle("SysPilot — Autonomous SRE Desktop Copilot")
        self.resize(980, 880)
        self.setStyleSheet(DARK_STYLESHEET)
        self.terminal_finished.connect(self.trigger_refresh)

        self.central_widget = QWidget()
        self.setCentralWidget(self.central_widget)
        self.main_layout = QVBoxLayout(self.central_widget)
        self.main_layout.setContentsMargins(14, 12, 14, 12)
        self.main_layout.setSpacing(12)

        # Header Status Banner (Modern Flat HUD)
        self.header_frame = QFrame()
        self.header_frame.setProperty("class", "flatHeader")
        header_layout = QVBoxLayout(self.header_frame)
        header_layout.setContentsMargins(4, 4, 4, 8)
        header_layout.setSpacing(8)

        header_top = QHBoxLayout()
        self.status_icon_label = QLabel("🟡")
        self.status_icon_label.setStyleSheet("font-size: 18px;")
        header_top.addWidget(self.status_icon_label)

        self.status_title = QLabel("SYS HEALTH  ›  Control Panel & Triage Sentinel")
        self.status_title.setStyleSheet("font-size: 15px; font-weight: bold; color: #fbbf24; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
        header_top.addWidget(self.status_title)
        header_top.addStretch(1)

        model_info = get_active_model_details()
        self.header_model_badge = QLabel(f"🤖 {model_info['model']}")
        self.header_model_badge.setStyleSheet(
            "background-color: #0c2838; border: 1px solid #0369a1; color: #38bdf8; "
            "font-size: 11px; font-weight: bold; padding: 4px 10px; border-radius: 6px; font-family: 'Hack', monospace;"
        )
        source_cfg = "~/.config/syspilot/config.env" if model_info.get("provider") == "custom" else "~/.config/goose/config.yaml"
        self.header_model_badge.setToolTip(f"Active AI Engine: {model_info['full_label']}\nConfigured in {source_cfg}")
        header_top.addWidget(self.header_model_badge)

        self.refresh_btn = QPushButton("↻ REFRESH TRIAGE")
        self.refresh_btn.setStyleSheet(
            "background-color: transparent; border: 1px solid #334155; color: #94a3b8; "
            "font-size: 11px; font-weight: bold; padding: 4px 12px; border-radius: 6px; font-family: 'Hack', monospace;"
        )
        self.refresh_btn.clicked.connect(self.trigger_refresh)
        header_top.addWidget(self.refresh_btn)
        header_layout.addLayout(header_top)

        header_sub = QHBoxLayout()
        self.env_chip_lbl = QLabel("kernel: ... • uptime: ... • root: ...")
        self.env_chip_lbl.setStyleSheet("color: #64748b; font-size: 12px; font-family: 'Hack', monospace;")
        header_sub.addWidget(self.env_chip_lbl)
        header_sub.addStretch(1)

        self.status_sub = QLabel("PRE-FLIGHT ATTENTION ⚠")
        self.status_sub.setStyleSheet(
            "background-color: #2b1704; border: 1px solid #92400e; color: #fbbf24; "
            "font-weight: bold; font-size: 11px; padding: 3px 10px; border-radius: 6px; font-family: 'Hack', monospace;"
        )
        header_sub.addWidget(self.status_sub)
        header_layout.addLayout(header_sub)

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
        self.tabs.addTab(self.copilot_tab, "🤖 AI Copilot")

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

        # Non-blocking startup triage refresh (100ms delay to keep GUI initialization instant while eliminating stale render lag)
        QTimer.singleShot(100, self._check_initial_refresh)

    def changeEvent(self, event):
        """Auto-refresh dashboard view when SysPilot window regains focus."""
        if event.type() == QEvent.Type.ActivationChange and self.isActiveWindow():
            self.update_ui_from_state()
        super().changeEvent(event)

    def _check_initial_refresh(self):
        """Perform initial background triage scan so dashboard is immediately up to date."""
        if not is_gamemode_active():
            self.trigger_refresh()

    # --------------------------------------------------------------------------
    # TAB 1: LEAN DASHBOARD (GUM & SYS-HEALTH CONTIGUOUS GRID ARCHITECTURE)
    # --------------------------------------------------------------------------
    def setup_lean_dashboard_tab(self):
        layout = QVBoxLayout(self.dashboard_tab)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(12)

        # ----------------------------------------------------------------------
        # SECTION 1: SYSTEM UPDATES
        # ----------------------------------------------------------------------
        self.sec1_frame = QFrame()
        self.sec1_frame.setProperty("class", "card")
        l1 = QVBoxLayout(self.sec1_frame)
        l1.setContentsMargins(10, 10, 10, 10)
        l1.setSpacing(8)

        t1_row = QHBoxLayout()
        t1_lbl = QLabel("SYSTEM UPDATES")
        t1_lbl.setStyleSheet("color: #e2e8f0; font-weight: bold; font-size: 13px; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
        t1_row.addWidget(t1_lbl)
        t1_row.addStretch()

        self.sec1_badge = QLabel("UPDATE ⚠")
        self.sec1_badge.setStyleSheet(STYLE_BADGE_WARN)
        t1_row.addWidget(self.sec1_badge)
        l1.addLayout(t1_row)

        # Contiguous Grid Table
        grid1 = QFrame()
        grid1.setProperty("class", "gridTable")
        grid1_l = QVBoxLayout(grid1)
        grid1_l.setContentsMargins(0, 0, 0, 0)
        grid1_l.setSpacing(0)

        grid1_l.addWidget(create_grid_header())

        self.core_pkgs_val_lbl = QLabel("Checking...")
        grid1_l.addWidget(create_grid_row("Core System Packages", self.core_pkgs_val_lbl, is_alt=False, has_bottom_border=True))

        self.repo_pkgs_val_lbl = QLabel("Checking...")
        grid1_l.addWidget(create_grid_row("Official Repositories", self.repo_pkgs_val_lbl, is_alt=True, has_bottom_border=False))

        l1.addWidget(grid1)

        # Action Buttons
        btn_box1 = QHBoxLayout()
        btn_box1.setContentsMargins(2, 6, 2, 2)
        self.btn_guarded_upgrade = QPushButton("⚡ RUN GUARDED SYSTEM UPGRADE")
        self.btn_guarded_upgrade.setProperty("class", "primaryAction")
        self.btn_guarded_upgrade.clicked.connect(self.run_guarded_upgrade_terminal)
        btn_box1.addWidget(self.btn_guarded_upgrade)

        self.btn_quick_audit = QPushButton("🔍 FULL DIAGNOSTIC AUDIT")
        self.btn_quick_audit.setProperty("class", "ghostAction")
        self.btn_quick_audit.clicked.connect(self.run_audit_terminal)
        btn_box1.addWidget(self.btn_quick_audit)
        btn_box1.addStretch()

        l1.addLayout(btn_box1)
        layout.addWidget(self.sec1_frame)

        # ----------------------------------------------------------------------
        # SECTION 2: THIRD-PARTY UPDATES
        # ----------------------------------------------------------------------
        self.sec2_frame = QFrame()
        self.sec2_frame.setProperty("class", "card")
        l2 = QVBoxLayout(self.sec2_frame)
        l2.setContentsMargins(10, 10, 10, 10)
        l2.setSpacing(8)

        t2_row = QHBoxLayout()
        t2_lbl = QLabel("THIRD-PARTY UPDATES")
        t2_lbl.setStyleSheet("color: #e2e8f0; font-weight: bold; font-size: 13px; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
        t2_row.addWidget(t2_lbl)
        t2_row.addStretch()

        self.sec2_badge = QLabel("UPDATE ⚠")
        self.sec2_badge.setStyleSheet(STYLE_BADGE_WARN)
        t2_row.addWidget(self.sec2_badge)
        l2.addLayout(t2_row)

        grid2 = QFrame()
        grid2.setProperty("class", "gridTable")
        grid2_l = QVBoxLayout(grid2)
        grid2_l.setContentsMargins(0, 0, 0, 0)
        grid2_l.setSpacing(0)

        grid2_l.addWidget(create_grid_header())

        self.aur_val_lbl = QLabel("Checking...")
        grid2_l.addWidget(create_grid_row("AUR Packages (yay/paru)", self.aur_val_lbl, is_alt=False, has_bottom_border=True))

        self.flatpak_val_lbl = QLabel("Checking...")
        grid2_l.addWidget(create_grid_row("Flatpak Applications", self.flatpak_val_lbl, is_alt=True, has_bottom_border=True))

        self.devtools_val_lbl = QLabel("Checking...")
        grid2_l.addWidget(create_grid_row("Developer Tools (Goose/UV)", self.devtools_val_lbl, is_alt=False, has_bottom_border=False))

        l2.addWidget(grid2)

        btn_box2 = QHBoxLayout()
        btn_box2.setContentsMargins(2, 6, 2, 2)
        self.btn_check_apps = QPushButton("📦 STANDALONE 3RD-PARTY TRIAGE")
        self.btn_check_apps.setProperty("class", "ghostAction")
        self.btn_check_apps.clicked.connect(self.run_software_terminal)
        btn_box2.addWidget(self.btn_check_apps)

        self.btn_update_goose = QPushButton("⚡ UPDATE GOOSE AI AGENT")
        self.btn_update_goose.setProperty("class", "primaryAction")
        self.btn_update_goose.setVisible(False)
        self.btn_update_goose.clicked.connect(self.run_update_goose_terminal)
        btn_box2.addWidget(self.btn_update_goose)
        btn_box2.addStretch()

        l2.addLayout(btn_box2)
        layout.addWidget(self.sec2_frame)

        # ----------------------------------------------------------------------
        # SECTION 3: SYSTEM HEALTH & HYGIENE
        # ----------------------------------------------------------------------
        self.sec3_frame = QFrame()
        self.sec3_frame.setProperty("class", "card")
        l3 = QVBoxLayout(self.sec3_frame)
        l3.setContentsMargins(10, 10, 10, 10)
        l3.setSpacing(8)

        t3_row = QHBoxLayout()
        t3_lbl = QLabel("SYSTEM HEALTH & HYGIENE")
        t3_lbl.setStyleSheet("color: #e2e8f0; font-weight: bold; font-size: 13px; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
        t3_row.addWidget(t3_lbl)
        t3_row.addStretch()

        self.sec3_badge = QLabel("CLEAN ✔")
        self.sec3_badge.setStyleSheet(STYLE_BADGE_PASS)
        t3_row.addWidget(self.sec3_badge)
        l3.addLayout(t3_row)

        grid3 = QFrame()
        grid3.setProperty("class", "gridTable")
        grid3_l = QVBoxLayout(grid3)
        grid3_l.setContentsMargins(0, 0, 0, 0)
        grid3_l.setSpacing(0)

        grid3_l.addWidget(create_grid_header())

        # Dynamic Reboot Row (hidden unless reboot pending)
        self.reboot_val_lbl = QLabel("WARN ⚠ (Running kernel replaced on disk - reboot recommended)")
        self.reboot_row_frame = create_grid_row("System Reboot Status", self.reboot_val_lbl, is_alt=False, has_bottom_border=True)
        self.reboot_row_frame.setVisible(False)
        grid3_l.addWidget(self.reboot_row_frame)

        self.services_val_lbl = QLabel("Checking...")
        grid3_l.addWidget(create_grid_row("Systemd Units", self.services_val_lbl, is_alt=False, has_bottom_border=True))

        self.baloo_journal_val_lbl = QLabel("Checking...")
        grid3_l.addWidget(create_grid_row("Baloo & Journal Health", self.baloo_journal_val_lbl, is_alt=True, has_bottom_border=True))

        self.orphans_val_lbl = QLabel("Checking...")
        self.btn_prune_orphans = QPushButton("🗑 PRUNE ORPHANS")
        self.btn_prune_orphans.setProperty("class", "pillWarning")
        self.btn_prune_orphans.setFixedHeight(26)
        self.btn_prune_orphans.setVisible(False)
        self.btn_prune_orphans.clicked.connect(lambda: self.run_custom_terminal(f"{get_syspilot_bin()} --orphans", "SysPilot Orphan Triage"))
        grid3_l.addWidget(create_grid_row("Orphan Dependencies", self.orphans_val_lbl, is_alt=False, has_bottom_border=True, extra_widget=self.btn_prune_orphans))

        self.pacnew_dash_lbl = QLabel("Checking...")
        grid3_l.addWidget(create_grid_row("Configuration (.pacnew)", self.pacnew_dash_lbl, is_alt=True, has_bottom_border=True))

        self.disk_val_lbl = QLabel("Checking...")
        grid3_l.addWidget(create_grid_row("Root Storage (/)", self.disk_val_lbl, is_alt=False, has_bottom_border=True))

        self.gaming_val_lbl = QLabel("Checking...")
        grid3_l.addWidget(create_grid_row("Gaming & Proton Stack", self.gaming_val_lbl, is_alt=True, has_bottom_border=True))

        self.hardware_val_lbl = QLabel("Detecting...")
        grid3_l.addWidget(create_grid_row("Hardware Platform & GPU", self.hardware_val_lbl, is_alt=False, has_bottom_border=False))

        l3.addWidget(grid3)
        layout.addWidget(self.sec3_frame)

        # ----------------------------------------------------------------------
        # COPILOT ACTION CARD (ONE-CLICK TERMINAL SRE COPILOT)
        # ----------------------------------------------------------------------
        copilot_card = QFrame()
        copilot_card.setProperty("class", "card")
        cp_layout = QHBoxLayout(copilot_card)
        cp_layout.setContentsMargins(14, 10, 14, 10)
        cp_layout.setSpacing(12)

        cp_text_col = QVBoxLayout()
        cp_title = QLabel("🤖 AUTONOMOUS SRE COPILOT")
        cp_title.setStyleSheet("color: #38bdf8; font-weight: bold; font-size: 13px; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
        cp_desc = QLabel("Launch interactive AI Copilot with local zero-waste telemetry")
        cp_desc.setStyleSheet("color: #94a3b8; font-size: 11px;")
        cp_text_col.addWidget(cp_title)
        cp_text_col.addWidget(cp_desc)
        cp_layout.addLayout(cp_text_col)
        cp_layout.addStretch()

        self.btn_dash_copilot = QPushButton("🤖 ASK AI COPILOT")
        self.btn_dash_copilot.setProperty("class", "primaryAction")
        self.btn_dash_copilot.setStyleSheet("background-color: #0369a1; color: #f0f9ff; font-weight: bold; padding: 8px 20px; font-size: 12px; border-radius: 6px;")
        self.btn_dash_copilot.clicked.connect(self.open_copilot_terminal)
        cp_layout.addWidget(self.btn_dash_copilot)

        layout.addWidget(copilot_card)
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
        b1.clicked.connect(lambda: self.run_custom_terminal(f"{get_syspilot_bin()} --orphans", "SysPilot Orphan Triage"))
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
        b2.clicked.connect(lambda: self.run_custom_terminal(f"{get_syspilot_bin()} --mirrors", "SysPilot Mirror Ranking"))
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
        b4.clicked.connect(lambda: self.run_custom_terminal(f"{get_syspilot_bin()} --deep-clean", "SysPilot Deep Clean"))
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
        model_info = get_active_model_details()
        if hasattr(self, "header_model_badge"):
            self.header_model_badge.setText(f"🤖 {model_info['model']}")
            source_cfg = "~/.config/syspilot/config.env" if model_info.get("provider") == "custom" else "~/.config/goose/config.yaml"
            self.header_model_badge.setToolTip(f"Active AI Engine: {model_info['full_label']}\nConfigured in {source_cfg}")

        if ready:
            self.copilot_stack.setCurrentIndex(0)
            self.badge_lbl.setText(
                f"🟢 Active Model: <span style='color: #38bdf8;'><b>{model_info['model']}</b></span> "
                f"<span style='color: #94a3b8;'>({model_info['provider_display']})</span> | Token-Lean Mode Active"
            )
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
        btn_faf.clicked.connect(lambda: self.ask_copilot("Diagnose and repair Forged Alliance Forever (FAF): check dependencies (bwrap, 32-bit Vulkan), verify ~/faf-linux runner, run update.sh perform, sync Game.prefs from Steam, ensure desktop shortcuts point to the dynamic runner, or execute 'syspilot --faf-repair'."))
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
            "SysPilot includes an autonomous Site Reliability Engineer (Copilot) for proactive maintenance and repair.<br>"
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

        t = QLabel("🧠 Agent Architecture: Modular Engine + SRE Copilot Skill")
        t.setStyleSheet("font-size: 16px; font-weight: bold; color: #a78bfa;")
        ac_layout.addWidget(t)

        desc = QLabel(
            "<b>How SysPilot Operates:</b><br>"
            "SysPilot does <b>NOT</b> run an unconstrained generic chatbot. Instead, it pairs:<br>"
            "• <b>Execution Engine (The Hands / Runner):</b> Manages LLM tool execution, command sandboxing, and terminal I/O (Goose / AGY / custom).<br>"
            "• <b><code>copilot-admin</code> (The Brain / SRE Architect):</b> Specialized skill built on "
            "battle-tested token-lean SRE architecture. It enforces strict safety guardrails, reads local telemetry first (0 tokens), "
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
    # DATA BINDING & REFRESH (GUM & SYS-HEALTH CONTIGUOUS GRID ARCHITECTURE)
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
        orphans = data.get("orphans", {})
        reboot_pending = data.get("reboot_pending", False)
        gaming_mode = data.get("gaming_mode", False)
        sys_health_data = data.get("sys_health", {})

        # ----------------------------------------------------------------------
        # Header Dynamic Severity Styling (Modern Flat HUD)
        # Synchronized with Rust Tray effective_status to eliminate Amber vs Green lag
        # ----------------------------------------------------------------------
        effective_status = status
        sys_errors = sys_health_data.get("errors", 0) if isinstance(sys_health_data, dict) else 0
        sys_warnings = sys_health_data.get("warnings", 0) if isinstance(sys_health_data, dict) else 0
        updates_total = updates.get("total", 0) if isinstance(updates, dict) else 0

        if status == "ACTION_REQUIRED" or sys_errors > 0:
            effective_status = "ACTION_REQUIRED"
        elif (
            status in ("REVIEW_WARNINGS", "PRE_FLIGHT_ATTENTION")
            or sys_warnings > 0
            or updates_total > 0
            or reboot_pending
        ):
            effective_status = "PRE_FLIGHT_ATTENTION"

        if gaming_mode:
            self.status_icon_label.setText("🎮")
            self.status_title.setText("SYS HEALTH  ›  Gaming Session Active (Diagnostics Inhibited)")
            self.status_title.setStyleSheet("font-size: 15px; font-weight: bold; color: #38bdf8; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
            self.status_sub.setText("GAMEMODE ACTIVE 🎮")
            self.status_sub.setStyleSheet(STYLE_BADGE_INFO)
        elif effective_status == "ACTION_REQUIRED":
            self.status_icon_label.setText("🔴")
            self.status_title.setText("SYS HEALTH  ›  Action Required ✖")
            self.status_title.setStyleSheet("font-size: 15px; font-weight: bold; color: #f87171; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
            sub_msg = "; ".join(reasons) if reasons else "ACTION REQUIRED ✖"
            self.status_sub.setText(f"ACTION REQUIRED ✖ ({sub_msg})")
            self.status_sub.setStyleSheet(STYLE_BADGE_FAIL)
        elif effective_status == "PRE_FLIGHT_ATTENTION":
            self.status_icon_label.setText("🟡")
            self.status_title.setText("SYS HEALTH  ›  Control Panel & Triage Sentinel")
            self.status_title.setStyleSheet("font-size: 15px; font-weight: bold; color: #fbbf24; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
            sub_msg = ", ".join(reasons) if reasons else "Advisories pending review"
            self.status_sub.setText(f"PRE-FLIGHT ATTENTION ⚠ ({sub_msg})")
            self.status_sub.setStyleSheet(STYLE_BADGE_WARN)
        else: # FLIGHT_READY
            self.status_icon_label.setText("🟢")
            self.status_title.setText("SYS HEALTH  ›  Flight Ready ✔")
            self.status_title.setStyleSheet("font-size: 15px; font-weight: bold; color: #4ade80; letter-spacing: 0.5px; font-family: 'Hack', monospace;")
            self.status_sub.setText("ALL CLEAR ✔ (System primed and stable)")
            self.status_sub.setStyleSheet(STYLE_BADGE_PASS)

        self.header_frame.setStyleSheet("background-color: transparent; border: none; padding: 2px 0px 6px 0px;")

        # Header Environment Telemetry Chip
        running_kern = os.uname().release
        used_pct = disk.get("used_pct", 0)
        avail_gb = disk.get("avail_gb", 0)
        uptime_str = get_system_uptime()
        gpu_str = data.get("sys_health", {}).get("gpu", {}).get("drivers_in_use", "")
        gpu_chip = f"   •   GPU: {gpu_str} (Active)" if gpu_str else ""
        self.env_chip_lbl.setText(f"kernel: {running_kern}   •   uptime: {uptime_str}   •   root: {used_pct}% used ({avail_gb}G free){gpu_chip}")

        # Header AI Model Badge
        if hasattr(self, "header_model_badge"):
            model_info = get_active_model_details()
            self.header_model_badge.setText(f"🤖 {model_info['model']}")
            source_cfg = "~/.config/syspilot/config.env" if model_info.get("provider") == "custom" else "~/.config/goose/config.yaml"
            self.header_model_badge.setToolTip(f"Active AI Engine: {model_info['full_label']}\nProvider: {model_info['provider_display']}\nConfigured in {source_cfg}")

        # ----------------------------------------------------------------------
        # SECTION 1: OFFICIAL REPOSITORY UPDATES
        # ----------------------------------------------------------------------
        tot_up = updates.get("total", 0)
        c_up = updates.get("core_count", 0)
        r_up = updates.get("regular_count", 0)
        core_pkgs = updates.get("core_packages", [])
        reg_pkgs = updates.get("regular_packages", [])

        if tot_up > 0:
            self.sec1_badge.setText(f"UPDATE ⚠ ({tot_up} pending)")
            self.sec1_badge.setStyleSheet(STYLE_BADGE_WARN)
        else:
            self.sec1_badge.setText("ALL CLEAR ✔")
            self.sec1_badge.setStyleSheet(STYLE_BADGE_PASS)

        if c_up > 0:
            cpkg_names = [p.split()[0] for p in core_pkgs]
            preview = ", ".join(cpkg_names[:4])
            if len(cpkg_names) > 4: preview += f" (+{len(cpkg_names)-4} more)"
            self.core_pkgs_val_lbl.setText(f"WARN ⚠ ({c_up} core packages: {preview})")
            self.core_pkgs_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
        else:
            self.core_pkgs_val_lbl.setText("PASS ✔ (Kernel, systemd, bootloader, NVIDIA drivers up to date)")
            self.core_pkgs_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        if r_up > 0:
            rpkg_names = [p.split()[0] for p in reg_pkgs]
            preview = ", ".join(rpkg_names[:5])
            if len(rpkg_names) > 5: preview += f" (+{len(rpkg_names)-5} more)"
            self.repo_pkgs_val_lbl.setText(f"UPDATE ⚠ ({r_up} packages pending: {preview})")
            self.repo_pkgs_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
        else:
            self.repo_pkgs_val_lbl.setText("PASS ✔ (All repository packages up to date)")
            self.repo_pkgs_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        self.btn_guarded_upgrade.setText(f"⚡ RUN GUARDED SYSTEM UPGRADE ({tot_up})")

        # ----------------------------------------------------------------------
        # SECTION 2: STANDALONE & 3RD-PARTY SOFTWARE
        # ----------------------------------------------------------------------
        if standalone.get("checked", False):
            aur_p = standalone.get("aur_pending", 0)
            fp_p = standalone.get("flatpak_pending", 0)
            g_det = standalone.get("details", {}).get("goose", {})
            g_ver = g_det.get("version", "N/A")
            g_latest = g_det.get("latest", g_ver)
            g_up_avail = g_det.get("update_available", False)
            
            uv_det = standalone.get("details", {}).get("uv", {})
            uv_ver = uv_det.get("version", "N/A")
            uv_up_avail = uv_det.get("update_available", False)

            aur_pkgs = standalone.get("details", {}).get("aur", {}).get("packages", [])
            tot_standalone = aur_p + fp_p

            if tot_standalone > 0:
                self.sec2_badge.setText(f"UPDATE ⚠ ({aur_p} AUR, {fp_p} Flatpak)")
                self.sec2_badge.setStyleSheet(STYLE_BADGE_WARN)
            else:
                self.sec2_badge.setText("UP TO DATE ✔")
                self.sec2_badge.setStyleSheet(STYLE_BADGE_PASS)

            if aur_p > 0:
                preview = ", ".join(aur_pkgs[:4]) if aur_pkgs else f"{aur_p} packages"
                self.aur_val_lbl.setText(f"UPDATE ⚠ ({aur_p} packages pending: {preview})")
                self.aur_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            else:
                self.aur_val_lbl.setText("PASS ✔ (All AUR packages up to date)")
                self.aur_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

            fp_inst = standalone.get("details", {}).get("flatpak", {}).get("installed", False)
            if not fp_inst:
                self.flatpak_val_lbl.setText("PASS ✔ (Not installed / Clean)")
                self.flatpak_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            elif fp_p > 0:
                self.flatpak_val_lbl.setText(f"UPDATE ⚠ ({fp_p} Flatpak updates available)")
                self.flatpak_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            else:
                self.flatpak_val_lbl.setText("PASS ✔ (All Flatpak applications up to date)")
                self.flatpak_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

            dev_parts = []
            if g_up_avail:
                dev_parts.append(f"Goose: UPDATE ⚠ (v{g_ver} → v{g_latest})")
            else:
                dev_parts.append(f"Goose: v{g_ver} ✔")
            if uv_up_avail:
                dev_parts.append(f"UV: UPDATE ⚠ (v{uv_ver})")
            else:
                dev_parts.append(f"UV: v{uv_ver} ✔")

            if g_up_avail or uv_up_avail:
                self.devtools_val_lbl.setText(f"UPDATE ⚠ ({' • '.join(dev_parts)})")
                self.devtools_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            else:
                self.devtools_val_lbl.setText(f"PASS ✔ ({' • '.join(dev_parts)})")
                self.devtools_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

            if g_up_avail:
                self.btn_update_goose.setText(f"⚡ UPDATE GOOSE ({g_ver} → {g_latest})")
                self.btn_update_goose.setVisible(True)
            else:
                self.btn_update_goose.setVisible(False)
        else:
            self.sec2_badge.setText("TRIAGE PENDING ℹ")
            self.sec2_badge.setStyleSheet(STYLE_BADGE_INFO)
            self.aur_val_lbl.setText("INFO ℹ (Pending inspection - click below)")
            self.aur_val_lbl.setStyleSheet("color: #5fd7ff; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.flatpak_val_lbl.setText("INFO ℹ (Pending inspection - click below)")
            self.flatpak_val_lbl.setStyleSheet("color: #5fd7ff; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.devtools_val_lbl.setText("INFO ℹ (Pending inspection - click below)")
            self.devtools_val_lbl.setStyleSheet("color: #5fd7ff; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.btn_update_goose.setVisible(False)

        # ----------------------------------------------------------------------
        # SECTION 3: SYSTEM HEALTH & STORAGE HYGIENE
        # ----------------------------------------------------------------------
        o_count = orphans.get("count", 0)
        p_count = pacnew.get("count", 0)
        sys_f = failed_services.get("system", [])
        usr_f = failed_services.get("user", [])
        total_gb = disk.get("total_gb", 0)
        used_pct = disk.get("used_pct", 0)
        avail_gb = disk.get("avail_gb", 0)

        has_advisories = (p_count > 0 or sys_f or usr_f or reboot_pending or used_pct >= 80 or not orphans.get("checked", True))
        if sys_f or used_pct >= 92:
            self.sec3_badge.setText("ACTION REQUIRED ✖")
            self.sec3_badge.setStyleSheet(STYLE_BADGE_FAIL)
        elif has_advisories:
            self.sec3_badge.setText("REVIEW ADVISORIES ⚠")
            self.sec3_badge.setStyleSheet(STYLE_BADGE_WARN)
        else:
            self.sec3_badge.setText("ALL CLEAR ✔ (0 Errors)")
            self.sec3_badge.setStyleSheet(STYLE_BADGE_PASS)

        # Reboot row
        if reboot_pending:
            self.reboot_row_frame.setVisible(True)
            self.reboot_val_lbl.setText("WARN ⚠ (Running kernel replaced on disk - reboot recommended)")
            self.reboot_val_lbl.setStyleSheet("color: #ffaf00; font-weight: bold; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
        else:
            self.reboot_row_frame.setVisible(False)

        # Services
        if sys_f or usr_f:
            f_str = []
            if sys_f: f_str.append(f"System: {', '.join(sys_f)}")
            if usr_f: f_str.append(f"User: {', '.join(usr_f)}")
            self.services_val_lbl.setText(f"FAIL ✖ ({' • '.join(f_str)})")
            self.services_val_lbl.setStyleSheet("color: #ef4444; font-weight: bold; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
        else:
            self.services_val_lbl.setText("PASS ✔ (All system and user units operational: 0 failed)")
            self.services_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        # Baloo & Journal Rate Limit Health
        checks = data.get("sys_health", {}).get("checks", {})
        if not checks and os.path.isfile(SYS_HEALTH_SUMMARY):
            try:
                with open(SYS_HEALTH_SUMMARY, "r", encoding="utf-8") as f:
                    checks = json.load(f).get("checks", {})
            except Exception:
                checks = {}

        baloo_check = checks.get("desktop_indexers", {})
        journal_check = checks.get("journal_rate_limit", {})
        baloo_obs = baloo_check.get("observations", [])
        baloo_state = baloo_obs[0].get("state", "unknown") if baloo_obs else "not reported"
        j_usage = journal_check.get("disk_usage_human", "OK")
        j_limited = journal_check.get("rate_limited", False)

        if baloo_check.get("status") == "fail" or journal_check.get("status") == "fail" or j_limited:
            self.baloo_journal_val_lbl.setText(f"WARN ⚠ (Baloo: {baloo_state} • Journal: {j_usage} rate-limited)")
            self.baloo_journal_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
        else:
            self.baloo_journal_val_lbl.setText(f"PASS ✔ (Baloo: {baloo_state} • Journal: {j_usage} healthy)")
            self.baloo_journal_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        # Orphans
        o_count = orphans.get("count", 0)
        opt_count = orphans.get("optional_count", 0)
        t1_count = orphans.get("tier1_count", 0)
        t3_count = orphans.get("tier3_count", 0)
        is_checked = orphans.get("checked", True)

        if not is_checked:
            err_msg = orphans.get("message", "pacman query failed")
            self.orphans_val_lbl.setText(f"WARN ⚠ ({err_msg})")
            self.orphans_val_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.btn_prune_orphans.setVisible(False)
        elif t1_count > 0:
            t1_pkgs = orphans.get("tier1_packages", [])
            preview = ", ".join(t1_pkgs[:3])
            if len(t1_pkgs) > 3: preview += f" (+{len(t1_pkgs)-3} more)"
            self.orphans_val_lbl.setText(f"INFO ℹ ({t1_count} strict orphan(s) eligible for pruning: {preview})")
            self.orphans_val_lbl.setStyleSheet("color: #5fd7ff; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.btn_prune_orphans.setText(f"🗑 PRUNE ORPHANS ({t1_count})")
            self.btn_prune_orphans.setProperty("class", "warning")
            self.btn_prune_orphans.setVisible(True)
        elif o_count > 0 or opt_count > 0:
            c_pkgs = orphans.get("tier3_packages", []) + orphans.get("tier2_packages", [])
            preview = ", ".join(c_pkgs[:3])
            if len(c_pkgs) > 3: preview += f" (+{len(c_pkgs)-3} more)"
            total_cand = len(c_pkgs) if c_pkgs else (o_count + opt_count)
            desc = f"0 strict unreferenced • {total_cand} candidate(s) in review: {preview}" if t3_count > 0 else f"0 strict orphans • {opt_count} optional candidate: {preview}"
            self.orphans_val_lbl.setText(f"INFO ℹ ({desc})")
            self.orphans_val_lbl.setStyleSheet("color: #5fd7ff; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.btn_prune_orphans.setText(f"🗑 REVIEW CANDIDATES ({total_cand})")
            self.btn_prune_orphans.setProperty("class", "secondary")
            self.btn_prune_orphans.setVisible(True)
        else:
            self.orphans_val_lbl.setText("PASS ✔ (Dependency tree clean: 0 orphans)")
            self.orphans_val_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
            self.btn_prune_orphans.setVisible(False)

        # Pacnew
        if p_count > 0:
            self.pacnew_dash_lbl.setText(f"WARN ⚠ ({p_count} .pacnew configuration file(s) require review)")
            self.pacnew_dash_lbl.setStyleSheet("color: #ffaf00; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")
        else:
            self.pacnew_dash_lbl.setText("PASS ✔ (0 .pacnew file conflicts pending review)")
            self.pacnew_dash_lbl.setStyleSheet("color: #4ade80; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        if hasattr(self, "pacnew_desc_lbl"):
            if p_count > 0:
                self.pacnew_desc_lbl.setText(f"⚠️ {p_count} .pacnew configuration file(s) require review to prevent service deprecations.")
                self.pacnew_desc_lbl.setStyleSheet("color: #f59e0b; font-weight: bold;")
            else:
                self.pacnew_desc_lbl.setText("✔ No .pacnew configuration conflicts detected.")
                self.pacnew_desc_lbl.setStyleSheet("color: #10b981;")

        # Disk
        disk_color = "#4ade80"
        disk_tag = "PASS ✔"
        if used_pct >= 92:
            disk_color = "#ef4444"
            disk_tag = "FAIL ✖ (Critically Full)"
        elif used_pct >= 80:
            disk_color = "#ffaf00"
            disk_tag = "WARN ⚠ (Elevated Usage)"
        self.disk_val_lbl.setText(f"{disk_tag} ({used_pct}% used • {avail_gb} GiB free of {total_gb} GiB total ext4)")
        self.disk_val_lbl.setStyleSheet(f"color: {disk_color}; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        # Gaming
        gaming_info = data.get("sys_health", {}).get("gaming", {})
        c_proton = gaming_info.get("custom_proton", "System Default")
        gm_txt = "ACTIVE" if gaming_mode else "Inactive"
        self.gaming_val_lbl.setText(f"INFO ℹ (GameMode: {gm_txt} • Multilib 32-bit: OK • {c_proton})")
        self.gaming_val_lbl.setStyleSheet("color: #5fd7ff; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        # Hardware & GPU Dynamic Row
        hw_txt, hw_color = self.get_hardware_summary(data.get("sys_health"))
        self.hardware_val_lbl.setText(hw_txt)
        self.hardware_val_lbl.setStyleSheet(f"color: {hw_color}; font-size: 13px; margin-left: 8px; font-family: 'Hack', monospace;")

        # Update tray icon
        if self.tray_app and hasattr(self.tray_app, "update_tray_icon"):
            self.tray_app.update_tray_icon(status, gaming_mode)

    def trigger_refresh(self):
        self.refresh_btn.setEnabled(False)
        self.refresh_btn.setText("Scanning...")
        threading.Thread(target=self._run_bg_refresh, daemon=True).start()

    def _run_bg_refresh(self):
        if self.tray_app and hasattr(self.tray_app, "last_pkg_check"):
            self.tray_app.last_pkg_check = time.time()
        run_triage(check_pkgs=True)
        QTimer.singleShot(0, self._on_refresh_finished)

    def _on_refresh_finished(self):
        self.update_ui_from_state()
        self.refresh_btn.setEnabled(True)
        self.refresh_btn.setText("↻ Refresh Triage")

    def get_hardware_summary(self, health_data: Optional[Dict[str, Any]] = None) -> Tuple[str, str]:
        """Dynamically detect CPU model, GPU model, and active driver without hardcoded assumptions."""
        if hasattr(self, "_cached_hw_summary"):
            return self._cached_hw_summary

        cpu_model = "CPU"
        try:
            with open("/proc/cpuinfo", "r", encoding="utf-8") as f:
                for line in f:
                    if line.startswith("model name"):
                        raw_cpu = line.split(":", 1)[1].strip()
                        cpu_model = re.sub(r'\(R\)|\(TM\)|CPU\s+|@.*$', '', raw_cpu).strip()
                        break
        except Exception:
            pass

        gpu_model = ""
        # 1. Try nvidia-smi if available
        try:
            smi_out = subprocess.check_output(
                ["nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader,nounits"],
                stderr=subprocess.DEVNULL, timeout=1.0
            ).decode().strip()
            if smi_out:
                parts = [p.strip() for p in smi_out.split(",")]
                gpu_model = f"{parts[0]} [{parts[1]}]"
        except Exception:
            pass

        # 2. Check lspci fallback
        if not gpu_model:
            try:
                lspci_out = subprocess.check_output(["lspci"], stderr=subprocess.DEVNULL, timeout=1.0).decode()
                for line in lspci_out.splitlines():
                    if any(k in line for k in ["VGA compatible controller", "3D controller"]):
                        raw_vga = line.split(":", 2)[-1].strip()
                        m = re.search(r'\[([^\]]+)\]', raw_vga)
                        gpu_model = m.group(1) if m else raw_vga
                        break
            except Exception:
                pass

        # 3. Fallback to summary.json gpu info
        if not gpu_model:
            gpu_info = (health_data or {}).get("gpu", {})
            drv = gpu_info.get("drivers_in_use", "")
            gpu_model = f"GPU ({drv})" if drv else "Universal Graphics"

        gpu_display = re.sub(r'Corporation|\[AMD/ATI\]', '', gpu_model).strip()
        summary = (f"PASS ✔ ({gpu_display} • {cpu_model})", "#4ade80")
        self._cached_hw_summary = summary
        return summary

    # --------------------------------------------------------------------------
    # TERMINAL RUNNERS (WITH AUTO-REFRESH ON COMPLETION)
    # --------------------------------------------------------------------------
    def _spawn_terminal(self, term_cmd: List[str]):
        """Run terminal command in background thread and auto-refresh triage on completion."""
        def _waiter():
            try:
                proc = subprocess.Popen(term_cmd)
                proc.wait()
            except Exception:
                pass
            # Trigger refresh on main Qt thread
            self.terminal_finished.emit()

        threading.Thread(target=_waiter, daemon=True).start()

    def run_guarded_upgrade_terminal(self):
        cmd = f"{get_syspilot_bin()} --upgrade"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Guarded Upgrade")
        self._spawn_terminal(term_cmd)

    def run_software_terminal(self):
        cmd = f"{get_syspilot_bin()} --software"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Standalone Software Triage")
        self._spawn_terminal(term_cmd)

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
        self._spawn_terminal(term_cmd)

    def run_maintenance_terminal(self):
        cmd = f"{get_syspilot_bin()} --maintenance"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Safe Maintenance")
        self._spawn_terminal(term_cmd)

    def run_audit_terminal(self):
        cmd = f"{get_syspilot_bin()} --audit"
        term_cmd = get_terminal_cmd(cmd, "SysPilot Full Audit")
        self._spawn_terminal(term_cmd)

    def run_custom_terminal(self, command: str, title: str):
        term_cmd = get_terminal_cmd(command, title)
        self._spawn_terminal(term_cmd)

    def open_copilot_terminal(self):
        user_conf = get_user_copilot_config()
        custom_cmd = user_conf.get("SYSPILOT_COPILOT_CMD") or user_conf.get("COPILOT_CMD")
        if custom_cmd:
            term_cmd = get_terminal_cmd(custom_cmd, "SysPilot AI Copilot Interactive")
        else:
            cmd = f"{get_syspilot_bin()} --copilot-shell"
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
        """Handle window close event."""
        if self.standalone_mode:
            event.accept()
            QApplication.quit()
            return
        event.ignore()
        self.hide()
        if self.tray_app and hasattr(self.tray_app, "tray_icon"):
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
        self.last_pkg_check = time.time()
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
            now = time.time()
            if now - getattr(self, "last_pkg_check", 0) >= 3600:
                self.last_pkg_check = now
                self.window.trigger_refresh()
            else:
                run_triage(check_pkgs=False)
                self.window.update_ui_from_state()

    def quit_app(self):
        self.tray_icon.hide()
        self.app.quit()

    def run(self, start_minimized: bool = False):
        if not start_minimized:
            self.show_window()
        return self.app.exec()


def run_gui_single_instance():
    """Run SysPilot GUI in standalone on-demand mode with single-instance enforcement."""
    socket_name = "syspilot_gui_single_instance"

    # Step 1: Check existing instance via QLocalSocket first
    socket = QLocalSocket()
    socket.connectToServer(socket_name)
    if socket.waitForConnected(300):
        # Existing instance is already running; signal it to activate and bring to focus
        socket.write(b"ACTIVATE\n")
        socket.waitForBytesWritten(300)
        socket.disconnectFromServer()
        sys.exit(0)

    # Step 2: Atomic file lock to eliminate startup race conditions between rapid double clicks
    state_dir = os.path.expanduser("~/.local/state/syspilot")
    os.makedirs(state_dir, exist_ok=True)
    lock_file = os.path.join(state_dir, ".gui.lock")
    try:
        lock_fd = os.open(lock_file, os.O_CREAT | os.O_RDWR, 0o600)
        fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except (BlockingIOError, OSError):
        # Another process holds the lock and is initializing; wait briefly and send ACTIVATE
        for _ in range(15):
            time.sleep(0.1)
            retry_socket = QLocalSocket()
            retry_socket.connectToServer(socket_name)
            if retry_socket.waitForConnected(300):
                retry_socket.write(b"ACTIVATE\n")
                retry_socket.waitForBytesWritten(300)
                retry_socket.disconnectFromServer()
                break
        sys.exit(0)

    app = QApplication.instance() or QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(True)

    # Step 3: Listen on socket BEFORE creating heavy window components
    server = QLocalServer()
    server.removeServer(socket_name)

    window = SysPilotWindow(tray_app=None)

    def handle_connection():
        client = server.nextPendingConnection()
        if client:
            client.waitForReadyRead(500)
            window.show()
            window.raise_()
            window.activateWindow()
            window.update_ui_from_state()
            client.disconnectFromServer()

    server.newConnection.connect(handle_connection)
    server.listen(socket_name)

    window.show()
    window.raise_()
    window.activateWindow()

    try:
        sys.exit(app.exec())
    finally:
        try:
            fcntl.flock(lock_fd, fcntl.LOCK_UN)
            os.close(lock_fd)
        except Exception:
            pass


if __name__ == "__main__":
    if "--tray" in sys.argv:
        app = SysPilotApp()
        sys.exit(app.run(start_minimized=True))
    else:
        run_gui_single_instance()
