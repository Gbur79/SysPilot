use crate::sentinels::SentinelStatus;
use crate::state::SystemSnapshot;
use crate::terminal::{spawn_detached_gui, spawn_terminal_command};
use crate::updater::UpdateSummary;
use ksni::{Category, MenuItem, Status, ToolTip, Tray};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct SysPilotTray {
    pub snapshot: SystemSnapshot,
    pub sentinels: SentinelStatus,
    pub updates: Option<UpdateSummary>,
    pub gamemode_active: bool,
    syspilot: Option<PathBuf>,
    exit_tx: mpsc::Sender<()>,
    last_gui_spawn: Option<Instant>,
}

impl SysPilotTray {
    pub fn new(
        snapshot: SystemSnapshot,
        sentinels: SentinelStatus,
        updates: Option<UpdateSummary>,
        gamemode_active: bool,
        syspilot: Option<PathBuf>,
        exit_tx: mpsc::Sender<()>,
    ) -> Self {
        Self {
            snapshot,
            sentinels,
            updates,
            gamemode_active,
            syspilot,
            exit_tx,
            last_gui_spawn: None,
        }
    }

    pub fn effective_updates(&self) -> (u32, u32) {
        if let Some(u) = &self.updates {
            // If snapshot was refreshed to 0 (e.g. after upgrade),
            // while internal cache holds older package count, authoritative state is clean (0).
            if self.snapshot.updates_total == 0 && u.total() > 0 {
                (0, 0)
            } else {
                (u.total(), u.core_count)
            }
        } else {
            (self.snapshot.updates_total, self.snapshot.core_updates)
        }
    }

    fn action_gui(&self, label: &str) -> MenuItem<Self> {
        use ksni::menu::StandardItem;
        let executable = self.syspilot.clone();
        StandardItem {
            label: label.into(),
            enabled: executable.is_some(),
            activate: Box::new(move |_| {
                if let Some(path) = executable.as_deref() {
                    if let Err(error) = spawn_detached_gui(path) {
                        eprintln!("SysPilot: {error}");
                    }
                }
            }),
            ..Default::default()
        }
        .into()
    }

    fn action(
        &self,
        label: &str,
        args: &'static [&'static str],
        title: &'static str,
    ) -> MenuItem<Self> {
        use ksni::menu::StandardItem;
        let executable = self.syspilot.clone();
        StandardItem {
            label: label.into(),
            enabled: executable.is_some(),
            activate: Box::new(move |_| {
                if let Some(path) = executable.as_deref() {
                    if let Err(error) = spawn_terminal_command(path, args, title) {
                        eprintln!("SysPilot: {error}");
                    }
                }
            }),
            ..Default::default()
        }
        .into()
    }
}

impl Tray for SysPilotTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "syspilot-sentinel".into()
    }

    fn title(&self) -> String {
        "SysPilot SRE Copilot".into()
    }

    fn category(&self) -> Category {
        Category::SystemServices
    }

    fn status(&self) -> Status {
        let (total_updates, _) = self.effective_updates();

        if self.sentinels.pacman_orphaned_lock
            || self.sentinels.reboot_pending
            || self.snapshot.has_data_problem()
            || total_updates > 0
            || !matches!(
                self.snapshot.effective_status(),
                "ALL_CLEAR" | "FLIGHT_READY"
            )
        {
            Status::NeedsAttention
        } else {
            Status::Active
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        if let Some(last) = self.last_gui_spawn {
            if last.elapsed() < Duration::from_millis(1500) {
                return;
            }
        }
        if let Some(path) = self.syspilot.as_deref() {
            if let Err(error) = spawn_detached_gui(path) {
                eprintln!("SysPilot: {error}");
            } else {
                self.last_gui_spawn = Some(Instant::now());
            }
        }
    }

    fn icon_name(&self) -> String {
        if self.sentinels.pacman_orphaned_lock {
            return "dialog-error".into();
        }
        if self.sentinels.reboot_pending || self.snapshot.reboot_pending {
            return "dialog-warning".into();
        }
        let (total_updates, _) = self.effective_updates();

        if total_updates > 0 {
            return "dialog-warning".into();
        }
        if self.snapshot.has_data_problem() {
            return "dialog-warning".into();
        }

        match self.snapshot.effective_status() {
            "ACTION_REQUIRED" => "dialog-error".into(),
            "REVIEW_WARNINGS" | "PRE_FLIGHT_ATTENTION" => "dialog-warning".into(),
            "ALL_CLEAR" | "FLIGHT_READY" if self.gamemode_active => "input-gaming".into(),
            "ALL_CLEAR" | "FLIGHT_READY" => "security-high".into(),
            _ => "utilities-system-monitor".into(),
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let (total_updates, core_updates) = self.effective_updates();

        let reboot_required = self.sentinels.reboot_pending || self.snapshot.reboot_pending;

        let title = if self.sentinels.pacman_orphaned_lock {
            "SysPilot — Pacman database locked (orphaned db.lck)".into()
        } else if reboot_required {
            "SysPilot — Reboot required (new kernel installed)".into()
        } else if let Some(error) = &self.snapshot.source_error {
            format!("SysPilot — telemetry unavailable: {error}")
        } else if self.snapshot.cache_stale {
            format!(
                "SysPilot — telemetry stale (age {})",
                self.snapshot.age_label()
            )
        } else if self.snapshot.effective_status() == "ACTION_REQUIRED" {
            "SysPilot — action required".into()
        } else if self.snapshot.security_high_risk > 0 {
            format!(
                "SysPilot — {} high-risk security CVE(s) pending",
                self.snapshot.security_high_risk
            )
        } else if total_updates > 0 {
            format!("SysPilot — {total_updates} updates available ({core_updates} core)")
        } else if self.gamemode_active {
            "SysPilot — GameMode active".into()
        } else {
            format!("SysPilot — Status: {}", self.snapshot.effective_status())
        };

        let description = format!(
            "Updates: {} ({} core)\nReboot: {}\nDisk (/): {}% used ({:.1}G free)",
            total_updates,
            core_updates,
            if reboot_required {
                "REQUIRED ⚠"
            } else {
                "clean"
            },
            self.sentinels.root_used_pct,
            self.sentinels.root_avail_gb,
        );

        ToolTip {
            title,
            description,
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::menu::StandardItem;

        let (total_updates, core_updates) = self.effective_updates();

        let header = if self.sentinels.pacman_orphaned_lock {
            "CRITICAL — Pacman database locked (orphaned db.lck)".into()
        } else if self.sentinels.reboot_pending || self.snapshot.reboot_pending {
            "REBOOT REQUIRED — New kernel modules installed".into()
        } else if self.snapshot.source_error.is_some() {
            "Telemetry unavailable — last known data is not trusted".into()
        } else if self.snapshot.cache_stale {
            format!("Telemetry stale — cache age {}", self.snapshot.age_label())
        } else if total_updates > 0 {
            format!(
                "Updates Pending — {} packages ({} core), {} errors",
                total_updates, core_updates, self.snapshot.errors
            )
        } else {
            format!(
                "{} — System primed and healthy (0 errors)",
                self.snapshot.effective_status()
            )
        };
        let exit_tx = self.exit_tx.clone();

        vec![
            StandardItem {
                label: header,
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            self.action_gui("✈ Open SysPilot Dashboard"),
            self.action("🔍 Full System Diagnostic", &["-a"], "SysPilot Audit"),
            self.action("⚡ Guarded System Upgrade", &["-u"], "Guarded Upgrade"),
            self.action(
                "🤖 Ask SysPilot Copilot",
                &["--copilot-shell"],
                "SysPilot AI Copilot",
            ),
            MenuItem::Separator,
            StandardItem {
                label: "Exit SysPilot".into(),
                activate: Box::new(move |_| {
                    let _ = exit_tx.try_send(());
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
