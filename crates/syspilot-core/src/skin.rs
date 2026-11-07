use std::path::{Path, PathBuf};

pub fn get_skin_config_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config/syspilot/skin"))
}

pub fn load_skin_preference_from(path: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(path) {
        return content.trim().to_lowercase() != "classic";
    }
    true // default: LCARS
}

pub fn save_skin_preference_to(path: &Path, is_lcars: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let val = if is_lcars { "lcars\n" } else { "classic\n" };
    std::fs::write(path, val)
}

pub fn load_skin_preference() -> bool {
    if let Some(path) = get_skin_config_path() {
        load_skin_preference_from(&path)
    } else {
        true
    }
}

pub fn save_skin_preference(is_lcars: bool) {
    if let Some(path) = get_skin_config_path() {
        let _ = save_skin_preference_to(&path, is_lcars);
    }
}

pub fn get_last_tab_config_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config/syspilot/last_tab"))
}

pub fn load_last_tab_preference_from(path: &Path) -> i32 {
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(val) = content.trim().parse::<i32>() {
            if (0..=2).contains(&val) {
                return val;
            }
        }
    }
    0 // default: 01 DASHBOARD
}

pub fn save_last_tab_preference_to(path: &Path, tab_idx: i32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{tab_idx}\n"))
}

pub fn load_last_tab_preference() -> i32 {
    if let Some(path) = get_last_tab_config_path() {
        load_last_tab_preference_from(&path)
    } else {
        0
    }
}

pub fn save_last_tab_preference(tab_idx: i32) {
    if let Some(path) = get_last_tab_config_path() {
        let _ = save_last_tab_preference_to(&path, tab_idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_tab_persistence_cycle() {
        let tmp = std::env::temp_dir().join(format!("syspilot_test_tab_{}", std::process::id()));
        assert_eq!(load_last_tab_preference_from(&tmp), 0);
        let _ = save_last_tab_preference_to(&tmp, 1);
        assert_eq!(load_last_tab_preference_from(&tmp), 1);
        let _ = save_last_tab_preference_to(&tmp, 2);
        assert_eq!(load_last_tab_preference_from(&tmp), 2);
        // Out-of-bounds fallback
        let _ = save_last_tab_preference_to(&tmp, 99);
        assert_eq!(load_last_tab_preference_from(&tmp), 0);
        let _ = std::fs::remove_file(tmp);
    }
}
