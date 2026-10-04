//! Startup manager: see and toggle what starts with the system.
//! Disabling is always reversible (Windows uses the same switch as Task Manager).

use crate::util::{self, fs as bfs};
use std::path::PathBuf;

#[derive(Clone, Debug, serde::Serialize)]
pub enum Source {
    /// registry Run key; `approved` = matching StartupApproved key
    Registry { key: String, value: String, approved: String },
    /// .lnk / file in a Startup folder
    Folder { path: PathBuf, approved: String },
    /// macOS LaunchAgent / LaunchDaemon plist
    Launchd { path: PathBuf },
    /// Linux XDG autostart .desktop file
    Desktop { path: PathBuf },
    /// Linux systemd --user unit
    Systemd { unit: String },
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub location: String,
    pub enabled: bool,
    /// target program no longer exists
    pub broken: bool,
    pub source: Source,
}

pub fn list() -> Vec<StartupItem> {
    if super::demo::on() {
        return super::demo::startup();
    }
    #[allow(unused_mut)]
    let mut v = Vec::new();
    #[cfg(windows)]
    win::list(&mut v);
    #[cfg(target_os = "macos")]
    mac::list(&mut v);
    #[cfg(target_os = "linux")]
    lin::list(&mut v);
    v.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

pub fn set_enabled(item: &StartupItem, on: bool) -> Result<(), String> {
    if bfs::dry() {
        return Ok(());
    }
    util::log(format!("startup {} -> {}", item.name, if on { "enabled" } else { "disabled" }));
    match &item.source {
        #[cfg(windows)]
        Source::Registry { value, approved, .. } => win::approve(approved, value, on),
        #[cfg(windows)]
        Source::Folder { path, approved } => win::approve(approved, &path.file_name().unwrap_or_default().to_string_lossy(), on),
        #[cfg(target_os = "macos")]
        Source::Launchd { path } => mac::toggle(path, on),
        #[cfg(target_os = "linux")]
        Source::Desktop { path } => lin::toggle_desktop(path, on),
        #[cfg(target_os = "linux")]
        Source::Systemd { unit } => {
            let (c, o) = util::run_status("systemctl", &["--user", if on { "enable" } else { "disable" }, unit]);
            if c == 0 {
                Ok(())
            } else {
                Err(o)
            }
        }
        #[allow(unreachable_patterns)]
        _ => Err("not supported on this OS".into()),
    }
}

/// Remove the entry for good (registry value backed up, files go to quarantine).
pub fn delete(item: &StartupItem) -> Result<(), String> {
    if bfs::dry() {
        return Ok(());
    }
    util::log(format!("startup {} -> deleted", item.name));
    match &item.source {
        #[cfg(windows)]
        Source::Registry { key, value, approved } => {
            crate::util::reg::delete_value(key, value).map_err(|e| e.to_string())?;
            let _ = crate::util::reg::delete_value(approved, value);
            Ok(())
        }
        Source::Folder { path, .. } | Source::Launchd { path } | Source::Desktop { path } => {
            bfs::quarantine(path, &bfs::quarantine_dir("startup")).map(|_| ()).ok_or_else(|| "could not move to quarantine".into())
        }
        #[cfg(target_os = "linux")]
        Source::Systemd { unit } => {
            let (c, o) = util::run_status("systemctl", &["--user", "disable", "--now", unit]);
            if c == 0 {
                Ok(())
            } else {
                Err(o)
            }
        }
        #[allow(unreachable_patterns)]
        _ => Err("not supported on this OS".into()),
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use crate::clean::windows_paths::{exe_from_command, path_missing};
    use crate::util::reg::{self, Val};

    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    fn enabled(approved: &str, value: &str) -> bool {
        match reg::get(approved, value) {
            Some(Val::Binary(b)) if !b.is_empty() => b[0] % 2 == 0, // 02/06 enabled, 03/07 disabled
            _ => true,
        }
    }

    pub fn approve(approved: &str, value: &str, on: bool) -> Result<(), String> {
        let mut b = vec![0u8; 12];
        b[0] = if on { 2 } else { 3 };
        if !on {
            // bytes 4..12: FILETIME of when it was disabled (Task Manager shows it)
            let ft = (chrono::Utc::now().timestamp() as u64 + 11_644_473_600) * 10_000_000;
            b[4..12].copy_from_slice(&ft.to_le_bytes());
        }
        reg::set(approved, value, &Val::Binary(b)).map_err(|e| e.to_string())
    }

    pub fn list(v: &mut Vec<StartupItem>) {
        let mut roots = vec![("HKLM".to_string(), "All users")];
        roots.extend(reg::all_user_roots().into_iter().map(|r| (r, "User")));
        for (root, who) in roots {
            for (rel, sub) in [
                (r"Software\Microsoft\Windows\CurrentVersion\Run", "Run"),
                (r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "Run32"),
            ] {
                let key = format!(r"{root}\{rel}");
                let approved = format!(r"{root}\{APPROVED}\{sub}");
                for (name, val) in reg::values(&key) {
                    let cmd = val.as_string().unwrap_or_default();
                    let broken = exe_from_command(&cmd).map(|e| path_missing(&e)).unwrap_or(false);
                    v.push(StartupItem {
                        enabled: enabled(&approved, &name),
                        name: name.clone(),
                        command: cmd,
                        location: format!("{who} registry ({sub})"),
                        broken,
                        source: Source::Registry { key: key.clone(), value: name, approved: approved.clone() },
                    });
                }
            }
        }
        let mut folders: Vec<(PathBuf, String)> = Vec::new();
        if let Some(pd) = util::env_path("ProgramData") {
            folders.push((pd.join(r"Microsoft\Windows\Start Menu\Programs\Startup"), format!(r"HKLM\{APPROVED}\StartupFolder")));
        }
        let home = util::home();
        folders
            .push((home.join(r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup"), format!(r"HKCU\{APPROVED}\StartupFolder")));
        for (dir, approved) in folders {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let file = e.file_name().to_string_lossy().to_string();
                if file.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                v.push(StartupItem {
                    enabled: enabled(&approved, &file),
                    name: file.trim_end_matches(".lnk").to_string(),
                    command: e.path().to_string_lossy().into(),
                    location: "Startup folder".into(),
                    broken: false,
                    source: Source::Folder { path: e.path(), approved: approved.clone() },
                });
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;

    pub fn list(v: &mut Vec<StartupItem>) {
        let home = util::home();
        for (dir, who) in [
            (home.join("Library/LaunchAgents"), "User agent"),
            (PathBuf::from("/Library/LaunchAgents"), "All users agent"),
            (PathBuf::from("/Library/LaunchDaemons"), "System daemon"),
        ] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let file = e.file_name().to_string_lossy().to_string();
                if !file.ends_with(".plist") {
                    continue;
                }
                let prog = util::run("plutil", &["-extract", "Program", "raw", &p.to_string_lossy()]).unwrap_or_default();
                let prog = if prog.trim().is_empty() {
                    util::run("plutil", &["-extract", "ProgramArguments.0", "raw", &p.to_string_lossy()]).unwrap_or_default()
                } else {
                    prog
                };
                let prog = prog.trim().to_string();
                let disabled =
                    util::run("plutil", &["-extract", "Disabled", "raw", &p.to_string_lossy()]).unwrap_or_default().trim() == "true";
                v.push(StartupItem {
                    name: file.trim_end_matches(".plist").to_string(),
                    broken: !prog.is_empty() && prog.starts_with('/') && !std::path::Path::new(&prog).exists(),
                    command: prog,
                    location: who.into(),
                    enabled: !disabled,
                    source: Source::Launchd { path: p },
                });
            }
        }
    }

    pub fn toggle(path: &std::path::Path, on: bool) -> Result<(), String> {
        let p = path.to_string_lossy();
        let (c, o) = util::run_status("launchctl", &[if on { "load" } else { "unload" }, "-w", &p]);
        if c == 0 {
            Ok(())
        } else {
            Err(o)
        }
    }
}

#[cfg(target_os = "linux")]
mod lin {
    use super::*;

    pub fn list(v: &mut Vec<StartupItem>) {
        let user = util::env_path("XDG_CONFIG_HOME").unwrap_or_else(|| util::home().join(".config")).join("autostart");
        let mut seen = std::collections::HashSet::new();
        for (dir, who) in [(user.clone(), "User autostart"), (PathBuf::from("/etc/xdg/autostart"), "System autostart")] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let file = e.file_name().to_string_lossy().to_string();
                if !file.ends_with(".desktop") || !seen.insert(file.clone()) {
                    continue;
                }
                let text = std::fs::read_to_string(e.path()).unwrap_or_default();
                let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
                let hidden = field("Hidden") == "true" || field("X-GNOME-Autostart-enabled") == "false";
                v.push(StartupItem {
                    name: if field("Name").is_empty() { file.clone() } else { field("Name") },
                    command: field("Exec"),
                    location: who.into(),
                    enabled: !hidden,
                    broken: false,
                    source: Source::Desktop { path: e.path() },
                });
            }
        }
        let out =
            util::run("systemctl", &["--user", "list-unit-files", "--type=service", "--state=enabled", "--no-legend"]).unwrap_or_default();
        for l in out.lines() {
            if let Some(unit) = l.split_whitespace().next() {
                v.push(StartupItem {
                    name: unit.into(),
                    command: String::new(),
                    location: "systemd user unit".into(),
                    enabled: true,
                    broken: false,
                    source: Source::Systemd { unit: unit.into() },
                });
            }
        }
    }

    /// System entries are overridden by a user copy with Hidden=true (the XDG way).
    pub fn toggle_desktop(path: &std::path::Path, on: bool) -> Result<(), String> {
        let user = util::env_path("XDG_CONFIG_HOME").unwrap_or_else(|| util::home().join(".config")).join("autostart");
        let _ = std::fs::create_dir_all(&user);
        let target = user.join(path.file_name().unwrap_or_default());
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("Hidden=")).map(String::from).collect();
        if !on {
            lines.push("Hidden=true".into());
        }
        std::fs::write(&target, lines.join("\n") + "\n").map_err(|e| e.to_string())
    }
}
