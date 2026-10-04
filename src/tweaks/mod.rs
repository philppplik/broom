//! Tweaks & optimizer: a catalog of reversible system changes.
//!
//! Every apply records the *actual* previous state in a journal, so undo restores exactly what
//! was there before - not just a guessed default.
//!
//! Sources:
//!   * winutil (MIT, Chris Titus Tech) - tweaks.json, feature.json, appx.json, dns.json, preset.json
//!     are embedded verbatim and interpreted natively.
//!   * Broom's own catalog - written independently; ideas inspired by Optimizer (hellzerg),
//!     Sparkle (thedogecraft), ReviOS Playbook (meetrevision) and Slate (QuiteAFancyEmerald).

#[cfg(target_os = "linux")]
mod catalog_linux;
#[cfg(target_os = "macos")]
mod catalog_mac;
#[cfg(windows)]
mod catalog_win;
#[cfg(windows)]
pub mod debloat;
pub mod dns;
#[cfg(windows)]
mod winutil;

#[cfg(windows)]
use crate::util::reg::{self, Val};
use crate::util::{self, fs as bfs, Risk};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum Op {
    #[cfg(windows)]
    RegSet { path: String, name: String, val: Val },
    #[cfg(windows)]
    RegDel { path: String, name: String },
    /// Windows service start type: Automatic | AutomaticDelayedStart | Manual | Disabled
    #[cfg(windows)]
    Service { name: String, start: String },
    /// Scheduled task Enable/Disable
    #[cfg(windows)]
    Task { name: String, enable: bool },
    /// PowerShell snippet (Windows)
    #[cfg(windows)]
    Ps(String),
    /// macOS `defaults write domain key -type value` (`value` None = delete)
    #[cfg(target_os = "macos")]
    Defaults { domain: String, key: String, ty: String, value: Option<String> },
    /// Linux GNOME setting
    #[cfg(target_os = "linux")]
    Gsettings { schema: String, key: String, value: String },
    /// Linux kernel parameter, persisted in /etc/sysctl.d/99-broom.conf
    #[cfg(target_os = "linux")]
    Sysctl { key: String, value: String },
    /// Write (Some) or remove (None) a whole file, e.g. a config drop-in
    File { path: String, content: Option<String> },
    /// Run a program
    Cmd { prog: String, args: Vec<String> },
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Tweak {
    pub id: String,
    pub name: String,
    pub category: String,
    pub desc: String,
    pub risk: Risk,
    pub source: &'static str,
    pub link: Option<String>,
    pub apply: Vec<Op>,
    /// Fallback undo when nothing is in the journal (e.g. applied by another tool)
    pub undo: Vec<Op>,
    /// Needs a sign-out or restart to take effect
    pub restart: bool,
    /// Explains why Broom recommends it for *this* machine (filled by `recommend`)
    pub hint: Option<String>,
}

/// How well a tweak can be undone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Undo {
    /// every change is journaled and restored to the exact previous value
    Exact,
    /// an undo script / fallback exists, but it may not restore everything exactly
    Script,
    /// one-way: removing software etc. Reinstall manually to go back.
    None,
}

impl Tweak {
    pub fn undo_kind(&self) -> Undo {
        // Decided by the kind of change only - never by the current machine state.
        if !self.apply.is_empty() && self.apply.iter().all(journaled_kind) {
            Undo::Exact
        } else if !self.undo.is_empty() {
            Undo::Script
        } else {
            Undo::None
        }
    }
}

/// Can the previous state of this kind of op be captured and restored exactly?
fn journaled_kind(op: &Op) -> bool {
    match op {
        Op::Cmd { .. } => is_cosmetic(op),
        #[cfg(windows)]
        Op::Ps(_) | Op::Task { .. } => false,
        _ => true,
    }
}

/// Ops that don't change state (e.g. restarting Finder after a defaults write).
fn is_cosmetic(op: &Op) -> bool {
    matches!(op, Op::Cmd { prog, .. } if prog == "killall")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Applied,
    NotApplied,
    /// Can't tell (script-only tweak) - check the journal instead
    Unknown,
}

pub fn catalog() -> Vec<Tweak> {
    #[allow(unused_mut)]
    let mut v: Vec<Tweak> = Vec::new();
    #[cfg(windows)]
    {
        v.extend(catalog_win::tweaks());
        v.extend(winutil::tweaks());
        v.extend(winutil::features());
    }
    #[cfg(target_os = "macos")]
    v.extend(catalog_mac::tweaks());
    #[cfg(target_os = "linux")]
    v.extend(catalog_linux::tweaks());
    let order = categories();
    v.sort_by_key(|t| order.iter().position(|c| *c == t.category).unwrap_or(99));
    v
}

pub fn categories() -> Vec<&'static str> {
    vec![
        "Privacy",
        "Performance",
        "Gaming",
        "Explorer & UI",
        "Services",
        "Network",
        "Power",
        "Essential (winutil)",
        "Preferences (winutil)",
        "Advanced (winutil)",
        "Windows features",
        "Finder & Dock",
        "Desktop",
        "System",
    ]
}

/// Named selections. winutil presets are taken from its preset.json.
pub fn presets() -> Vec<(&'static str, &'static str, Vec<String>)> {
    #[allow(unused_mut)]
    let mut v: Vec<(&'static str, &'static str, Vec<String>)> = Vec::new();
    #[cfg(windows)]
    {
        v.extend(winutil::presets());
        v.push(("Broom Privacy", "Turn off ads, tracking, suggestions and AI data collection", catalog_win::ids_in(&["Privacy"])));
        v.push(("Broom Gaming", "Lower latency and less background noise for games", catalog_win::ids_in(&["Gaming"])));
        v.push(("Broom Snappy", "Faster menus, shutdown and Explorer", catalog_win::ids_in(&["Performance", "Explorer & UI"])));
    }
    #[cfg(not(windows))]
    {
        let all: Vec<String> = catalog().into_iter().filter(|t| t.risk == Risk::Safe).map(|t| t.id).collect();
        v.push(("Recommended", "All safe tweaks", all));
    }
    v
}

// ---------------------------------------------------------------- state

pub fn state(t: &Tweak) -> State {
    let mut checked = 0;
    for op in &t.apply {
        match current(op) {
            Some(true) => checked += 1,
            Some(false) => return State::NotApplied,
            None => {}
        }
    }
    if checked > 0 || journal().iter().any(|e| e.id == t.id) {
        State::Applied
    } else {
        State::Unknown
    }
}

/// Does the system currently match this op? None = can't be checked.
fn current(op: &Op) -> Option<bool> {
    match op {
        #[cfg(windows)]
        Op::RegSet { path, name, val } => Some(reg::get(path, name).as_ref() == Some(val) || loosely_equal(reg::get(path, name), val)),
        #[cfg(windows)]
        Op::RegDel { path, name } => Some(reg::get(path, name).is_none()),
        #[cfg(windows)]
        Op::Service { name, start } => service_start(name).map(|s| s.eq_ignore_ascii_case(start)),
        #[cfg(target_os = "macos")]
        Op::Defaults { domain, key, value, .. } => {
            let cur = util::run("defaults", &["read", domain, key]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            Some(match (cur, value) {
                (None, None) => true,
                (Some(c), Some(v)) => norm_bool(&c) == norm_bool(v),
                _ => false,
            })
        }
        #[cfg(target_os = "linux")]
        Op::Gsettings { schema, key, value } => {
            util::run("gsettings", &["get", schema, key]).ok().map(|c| c.trim().trim_matches('\'') == value.trim_matches('\''))
        }
        #[cfg(target_os = "linux")]
        Op::Sysctl { key, value } => {
            std::fs::read_to_string(format!("/proc/sys/{}", key.replace('.', "/"))).ok().map(|c| c.trim() == value)
        }
        Op::File { path, content } => Some(match content {
            Some(c) => std::fs::read_to_string(path).map(|x| x == *c).unwrap_or(false),
            None => !std::path::Path::new(path).exists(),
        }),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn norm_bool(s: &str) -> String {
    match s.trim() {
        "1" | "true" | "YES" => "1".into(),
        "0" | "false" | "NO" => "0".into(),
        o => o.into(),
    }
}

#[cfg(windows)]
fn loosely_equal(cur: Option<Val>, want: &Val) -> bool {
    // winutil sometimes writes numbers as strings and vice versa
    match (cur.and_then(|c| c.as_string()), want.as_string()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(windows)]
pub fn service_start(name: &str) -> Option<String> {
    let k = format!(r"HKLM\SYSTEM\CurrentControlSet\Services\{name}");
    let start = reg::get(&k, "Start")?.as_u64()?;
    let delayed = reg::get(&k, "DelayedAutostart").and_then(|v| v.as_u64()) == Some(1);
    Some(
        match (start, delayed) {
            (2, true) => "AutomaticDelayedStart",
            (2, false) => "Automatic",
            (3, _) => "Manual",
            (4, _) => "Disabled",
            (0 | 1, _) => "Boot",
            _ => "Unknown",
        }
        .to_string(),
    )
}

// ---------------------------------------------------------------- journal

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub name: String,
    pub time: String,
    /// ops that restore the previous state, in order
    pub restore: Vec<Op>,
}

fn journal_path() -> std::path::PathBuf {
    util::data_dir().join("tweak-journal.json")
}

pub fn journal() -> Vec<JournalEntry> {
    std::fs::read_to_string(journal_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_journal(j: &[JournalEntry]) {
    let _ = std::fs::write(journal_path(), serde_json::to_string_pretty(j).unwrap_or_default());
}

/// The op that would put back what is there right now (before `op` runs).
fn capture(op: &Op) -> Option<Op> {
    Some(match op {
        #[cfg(windows)]
        Op::RegSet { path, name, .. } | Op::RegDel { path, name } => match reg::get(path, name) {
            Some(val) => Op::RegSet { path: path.clone(), name: name.clone(), val },
            None => Op::RegDel { path: path.clone(), name: name.clone() },
        },
        #[cfg(windows)]
        Op::Service { name, .. } => Op::Service { name: name.clone(), start: service_start(name)? },
        #[cfg(target_os = "macos")]
        Op::Defaults { domain, key, .. } => {
            let ty = util::run("defaults", &["read-type", domain, key]).unwrap_or_default();
            let ty = ty.trim().strip_prefix("Type is ").unwrap_or("").to_string();
            let cur = util::run("defaults", &["read", domain, key]).ok().map(|s| s.trim().to_string()).filter(|_| !ty.is_empty());
            Op::Defaults { domain: domain.clone(), key: key.clone(), ty: if ty == "boolean" { "bool".into() } else { ty }, value: cur }
        }
        #[cfg(target_os = "linux")]
        Op::Gsettings { schema, key, .. } => Op::Gsettings {
            schema: schema.clone(),
            key: key.clone(),
            value: util::run("gsettings", &["get", schema, key]).ok()?.trim().to_string(),
        },
        #[cfg(target_os = "linux")]
        Op::Sysctl { key, .. } => Op::Sysctl {
            key: key.clone(),
            value: std::fs::read_to_string(format!("/proc/sys/{}", key.replace('.', "/"))).ok()?.trim().to_string(),
        },
        Op::File { path, .. } => Op::File { path: path.clone(), content: std::fs::read_to_string(path).ok() },
        _ => return None,
    })
}

// ---------------------------------------------------------------- execution

fn run_op(op: &Op) -> Result<(), String> {
    if bfs::dry() {
        return Ok(());
    }
    match op {
        #[cfg(windows)]
        Op::RegSet { path, name, val } => reg::set(path, name, val).map_err(|e| format!("{path}\\{name}: {e}")),
        #[cfg(windows)]
        Op::RegDel { path, name } => reg::delete_value(path, name).map_err(|e| e.to_string()),
        #[cfg(windows)]
        Op::Service { name, start } => {
            if service_start(name).is_none() {
                return Ok(()); // service doesn't exist on this edition
            }
            util::ps(&format!("Set-Service -Name {} -StartupType {} -ErrorAction Stop", util::ps_quote(name), start))
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        #[cfg(windows)]
        Op::Task { name, enable } => {
            let _ = util::run_status("schtasks.exe", &["/change", "/tn", name, if *enable { "/enable" } else { "/disable" }]);
            Ok(())
        }
        #[cfg(windows)]
        Op::Ps(script) => util::ps(&format!("$ErrorActionPreference='Continue';{script}")).map(|_| ()).map_err(|e| e.to_string()),
        #[cfg(target_os = "macos")]
        Op::Defaults { domain, key, ty, value } => {
            let (c, o) = match value {
                Some(v) => util::run_status("defaults", &["write", domain, key, &format!("-{ty}"), v]),
                None => util::run_status("defaults", &["delete", domain, key]),
            };
            if c == 0 || value.is_none() {
                Ok(())
            } else {
                Err(o)
            }
        }
        #[cfg(target_os = "linux")]
        Op::Gsettings { schema, key, value } => {
            let (c, o) = util::run_status("gsettings", &["set", schema, key, value]);
            if c == 0 {
                Ok(())
            } else {
                Err(o)
            }
        }
        #[cfg(target_os = "linux")]
        Op::Sysctl { key, value } => {
            let (c, o) = util::run_status("sysctl", &["-w", &format!("{key}={value}")]);
            if c != 0 {
                return Err(o);
            }
            let conf = "/etc/sysctl.d/99-broom.conf";
            let mut lines: Vec<String> = std::fs::read_to_string(conf)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.starts_with(&format!("{key}")))
                .map(String::from)
                .collect();
            lines.push(format!("{key} = {value}"));
            std::fs::write(conf, lines.join("\n") + "\n").map_err(|e| e.to_string())
        }
        Op::File { path, content } => match content {
            Some(c) => {
                if let Some(p) = std::path::Path::new(path).parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                std::fs::write(path, c).map_err(|e| e.to_string())
            }
            None => match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            },
        },
        Op::Cmd { prog, args } => {
            let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let (c, o) = util::run_status(prog, &a);
            if c == 0 {
                Ok(())
            } else {
                Err(format!("{prog}: {}", o.trim()))
            }
        }
    }
}

/// Apply a tweak: capture previous state, run ops, journal it.
pub fn apply(t: &Tweak) -> Result<(), String> {
    util::log(format!("tweak apply {} ({})", t.name, t.id));
    let restore: Vec<Op> = t.apply.iter().filter_map(capture).collect();
    let mut errors = Vec::new();
    for op in &t.apply {
        if let Err(e) = run_op(op) {
            errors.push(e);
        }
    }
    if !bfs::dry() {
        let mut j = journal();
        if !j.iter().any(|e| e.id == t.id) {
            // keep the oldest snapshot: that is the real "before Broom" state
            j.push(JournalEntry {
                id: t.id.clone(),
                name: t.name.clone(),
                time: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
                restore,
            });
            save_journal(&j);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Undo a tweak: journal snapshot if we have one, else the catalog's undo ops.
pub fn undo(t: &Tweak) -> Result<(), String> {
    util::log(format!("tweak undo {} ({})", t.name, t.id));
    let mut j = journal();
    let ops: Vec<Op> = match j.iter().position(|e| e.id == t.id) {
        Some(i) => j.remove(i).restore,
        None => t.undo.clone(),
    };
    if ops.is_empty() {
        return Err("no undo information for this tweak".into());
    }
    let mut errors = Vec::new();
    for op in &ops {
        if let Err(e) = run_op(op) {
            errors.push(e);
        }
    }
    if !bfs::dry() {
        save_journal(&j);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Undo by journal entry id (for tweaks that are no longer in the catalog).
pub fn undo_journal_entry(id: &str) -> Result<(), String> {
    let mut j = journal();
    let Some(i) = j.iter().position(|e| e.id == id) else { return Err("not in journal".into()) };
    let e = j.remove(i);
    for op in &e.restore {
        run_op(op)?;
    }
    save_journal(&j);
    Ok(())
}

/// Hardware-aware recommendations: mark tweaks that fit this machine, warn on ones that don't.
pub fn recommend(tweaks: &mut [Tweak], hw: &crate::doctor::Hardware) {
    for t in tweaks.iter_mut() {
        t.hint = match t.id.as_str() {
            "power-throttling-off" | "ultimate-plan" | "WPFTweaksPowershell7" if hw.is_laptop => {
                Some("Laptop detected: costs battery life".into())
            }
            "hags-on" if hw.gpu_vendors.iter().any(|g| g == "NVIDIA" || g == "AMD") => Some("Recommended: your GPU supports it".into()),
            "hags-on" => Some("Only useful with a recent NVIDIA/AMD GPU".into()),
            "WPFTweaksHiber" if hw.is_laptop => Some("Laptop detected: you may want to keep hibernation".into()),
            "sysmain-off" if hw.system_disk_ssd == Some(false) => Some("System disk is an HDD: keep SysMain on".into()),
            "game-dvr-off" | "game-mode-on" if hw.gpu_vendors.is_empty() => None,
            _ => t.hint.take(),
        };
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// apply -> state Applied -> undo restores the exact previous value (incl. "didn't exist").
    #[test]
    fn apply_undo_restores_exact_previous_state() {
        let dir = std::env::temp_dir().join(format!("broom-tweak-test-{}", std::process::id()));
        std::env::set_var("BROOM_DATA_DIR", &dir);
        let key = r"HKCU\Software\BroomTest";
        reg::set(key, "Existing", &Val::Dword(7)).unwrap();
        let t = Tweak {
            id: "test-tweak".into(),
            name: "test".into(),
            category: "Test".into(),
            desc: String::new(),
            risk: Risk::Safe,
            source: "test",
            link: None,
            apply: vec![
                Op::RegSet { path: key.into(), name: "Existing".into(), val: Val::Dword(1) },
                Op::RegSet { path: key.into(), name: "New".into(), val: Val::Sz("hello".into()) },
            ],
            undo: vec![],
            restart: false,
            hint: None,
        };
        assert_eq!(state(&t), State::NotApplied);
        apply(&t).unwrap();
        assert_eq!(state(&t), State::Applied);
        assert_eq!(reg::get(key, "Existing"), Some(Val::Dword(1)));
        undo(&t).unwrap();
        assert_eq!(reg::get(key, "Existing"), Some(Val::Dword(7)), "old value back");
        assert_eq!(reg::get(key, "New"), None, "value that didn't exist is removed again");
        assert!(journal().is_empty());
        let _ = reg::delete_key(key);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
