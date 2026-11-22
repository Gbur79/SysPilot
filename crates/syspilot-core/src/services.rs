use crate::models::FailedServicesStatus;
use crate::sys_root::sub_path;
#[cfg(test)]
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use zbus::zvariant::OwnedObjectPath;

const SYSTEMD_DEST: &str = "org.freedesktop.systemd1";
const SYSTEMD_MANAGER_PATH: &str = "/org/freedesktop/systemd1";
const SYSTEMD_MANAGER_IFACE: &str = "org.freedesktop.systemd1.Manager";
const SYSTEMD_UNIT_IFACE: &str = "org.freedesktop.systemd1.Unit";
const SYSTEMD_SERVICE_IFACE: &str = "org.freedesktop.systemd1.Service";
const SYSTEMD_SCOPE_IFACE: &str = "org.freedesktop.systemd1.Scope";
const DBUS_QUERY_TIMEOUT: Duration = Duration::from_millis(1500);
const TRANSIENT_QUERY_TIMEOUT: Duration = Duration::from_millis(1000);

pub type SystemdUnitRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);

#[cfg(test)]
thread_local! {
    static TEST_DBUS_ADDRESSES: RefCell<(Option<String>, Option<String>)> =
        const { RefCell::new((None, None)) };
}

#[cfg(test)]
pub fn set_test_dbus_addresses(system: Option<String>, user: Option<String>) {
    TEST_DBUS_ADDRESSES.with(|addrs| *addrs.borrow_mut() = (system, user));
}

pub fn user_systemd_available_at(root: &Path, xdg_runtime_dir: Option<&Path>) -> bool {
    if !sub_path(root, "/run/systemd/system").is_dir() {
        return false;
    }
    let Some(xdg) = xdg_runtime_dir else {
        return false;
    };
    xdg.join("systemd/private").exists() || xdg.join("bus").exists()
}

pub fn is_candidate_app_unit(unit: &str) -> bool {
    let lower = unit.trim().to_ascii_lowercase();
    lower.starts_with("app-") && (lower.ends_with(".service") || lower.ends_with(".scope"))
}

pub fn unit_slice_interface(unit: &str) -> Option<&'static str> {
    let lower = unit.trim().to_ascii_lowercase();
    if lower.ends_with(".service") {
        Some(SYSTEMD_SERVICE_IFACE)
    } else if lower.ends_with(".scope") {
        Some(SYSTEMD_SCOPE_IFACE)
    } else {
        None
    }
}

async fn build_systemd_proxy(
    conn: &zbus::Connection,
    path: zbus::zvariant::ObjectPath<'_>,
    iface: &'static str,
) -> Result<zbus::Proxy<'static>, String> {
    zbus::proxy::Builder::<zbus::Proxy<'static>>::new(conn)
        .destination(SYSTEMD_DEST)
        .map_err(|e| format!("invalid systemd D-Bus destination: {e}"))?
        .path(path.into_owned())
        .map_err(|e| format!("invalid systemd D-Bus path: {e}"))?
        .interface(iface)
        .map_err(|e| format!("invalid systemd D-Bus interface: {e}"))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| format!("D-Bus proxy build failed: {e}"))
}

async fn connect_builder(
    builder: zbus::Result<zbus::connection::Builder<'_>>,
) -> Result<zbus::Connection, String> {
    let builder = builder.map_err(|e| format!("D-Bus address resolution failed: {e}"))?;
    timeout(DBUS_QUERY_TIMEOUT, builder.build())
        .await
        .map_err(|_| "D-Bus connect timed out".to_string())?
        .map_err(|e| format!("D-Bus connect failed: {e}"))
}

#[cfg(test)]
async fn connect_bus(user_mode: bool) -> Result<zbus::Connection, String> {
    let Some(addr) = TEST_DBUS_ADDRESSES.with(|addrs| {
        let pair = addrs.borrow();
        if user_mode {
            pair.1.clone()
        } else {
            pair.0.clone()
        }
    }) else {
        return Err("test D-Bus address not configured".to_string());
    };
    connect_builder(zbus::connection::Builder::address(addr.as_str())).await
}

#[cfg(not(test))]
async fn connect_bus(user_mode: bool) -> Result<zbus::Connection, String> {
    let builder = if user_mode {
        zbus::connection::Builder::session()
    } else {
        zbus::connection::Builder::system()
    };
    connect_builder(builder).await
}

async fn is_transient_desktop_app_unit_on(
    conn: &zbus::Connection,
    unit_name: &str,
    unit_path: &OwnedObjectPath,
) -> bool {
    let Some(slice_iface) = unit_slice_interface(unit_name) else {
        return false;
    };
    let Ok(unit_proxy) = build_systemd_proxy(conn, unit_path.as_ref(), SYSTEMD_UNIT_IFACE).await
    else {
        return false;
    };
    let Ok(true) = timeout(
        TRANSIENT_QUERY_TIMEOUT,
        unit_proxy.get_property::<bool>("Transient"),
    )
    .await
    .unwrap_or(Ok(false)) else {
        return false;
    };
    let Ok(slice_proxy) = build_systemd_proxy(conn, unit_path.as_ref(), slice_iface).await else {
        return false;
    };
    matches!(
        timeout(
            TRANSIENT_QUERY_TIMEOUT,
            slice_proxy.get_property::<String>("Slice"),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .as_deref(),
        Some("app.slice")
    )
}

pub async fn get_transient_desktop_app_units(unit_names: &[String]) -> HashSet<String> {
    let candidates: Vec<&str> = unit_names
        .iter()
        .map(String::as_str)
        .filter(|u| is_candidate_app_unit(u))
        .collect();

    if candidates.is_empty() {
        return HashSet::new();
    }

    let Ok(conn) = connect_bus(true).await else {
        return HashSet::new();
    };
    let manager_path = zbus::zvariant::ObjectPath::from_static_str_unchecked(SYSTEMD_MANAGER_PATH);
    let Ok(manager_proxy) = build_systemd_proxy(&conn, manager_path, SYSTEMD_MANAGER_IFACE).await
    else {
        return HashSet::new();
    };

    let mut ignored = HashSet::new();
    for unit_name in candidates {
        let Ok(Ok(unit_path)) = timeout(
            TRANSIENT_QUERY_TIMEOUT,
            manager_proxy.call::<_, _, OwnedObjectPath>("GetUnit", &(unit_name,)),
        )
        .await
        else {
            continue;
        };
        if is_transient_desktop_app_unit_on(&conn, unit_name, &unit_path).await {
            ignored.insert(unit_name.to_string());
        }
    }
    ignored
}

pub async fn query_failed_unit_entries_on(
    conn: &zbus::Connection,
) -> Result<Vec<(String, OwnedObjectPath)>, String> {
    let manager_path = zbus::zvariant::ObjectPath::from_static_str_unchecked(SYSTEMD_MANAGER_PATH);
    let proxy = build_systemd_proxy(conn, manager_path, SYSTEMD_MANAGER_IFACE).await?;
    let rows: Vec<SystemdUnitRow> = timeout(
        DBUS_QUERY_TIMEOUT,
        proxy.call("ListUnitsFiltered", &(&["failed"][..],)),
    )
    .await
    .map_err(|_| "systemd D-Bus ListUnitsFiltered timed out".to_string())?
    .map_err(|e| format!("systemd D-Bus ListUnitsFiltered failed: {e}"))?;

    Ok(rows.into_iter().map(|row| (row.0, row.6)).collect())
}

pub async fn query_failed_units(user_mode: bool) -> Result<Vec<String>, String> {
    let conn = connect_bus(user_mode).await?;
    let entries = query_failed_unit_entries_on(&conn).await?;
    Ok(entries.into_iter().map(|(name, _)| name).collect())
}

pub async fn check_failed_services() -> FailedServicesStatus {
    let system = match query_failed_units(false).await {
        Ok(units) => units,
        Err(err) => {
            eprintln!("syspilot-core: system systemd D-Bus query failed: {err}");
            Vec::new()
        }
    };
    let (user, user_manager_available) = match connect_bus(true).await {
        Ok(user_conn) => match query_failed_unit_entries_on(&user_conn).await {
            Ok(entries) => {
                let mut user = Vec::new();
                for (unit_name, unit_path) in entries {
                    if is_candidate_app_unit(&unit_name)
                        && is_transient_desktop_app_unit_on(&user_conn, &unit_name, &unit_path)
                            .await
                    {
                        continue;
                    }
                    user.push(unit_name);
                }
                (user, true)
            }
            Err(err) => {
                eprintln!("syspilot-core: user systemd D-Bus ListUnitsFiltered failed: {err}");
                (Vec::new(), false)
            }
        },
        Err(err) => {
            eprintln!("syspilot-core: user systemd D-Bus unavailable: {err}");
            (Vec::new(), false)
        }
    };

    FailedServicesStatus {
        system,
        user,
        user_manager_available,
    }
}
