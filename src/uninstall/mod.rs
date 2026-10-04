//! Uninstaller: list installed software with real sizes, uninstall (quietly where safe),
//! then hunt down leftovers and quarantine them.
//!
//! Ideas credited to Bulk Crap Uninstaller (Apache-2.0, Marcin Szeniak) and Prune (MIT, jimman0I):
//! quiet-uninstall detection per installer type, per-program leftover scan with confidence levels,
//! quarantine instead of deletion.

mod demo;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
pub mod startup;
#[cfg(windows)]
mod windows;

use crate::util::{self, fs as bfs};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Kind {
    Program,
    Msi,
    Store,
    App,
    Brew,
    Cask,
    Deb,
    Rpm,
    Pacman,
    Flatpak,
    Snap,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Program => "Win32",
            Kind::Msi => "MSI",
            Kind::Store => "Store",
            Kind::App => "App",
            Kind::Brew => "brew",
            Kind::Cask => "cask",
            Kind::Deb => "deb",
            Kind::Rpm => "rpm",
            Kind::Pacman => "pacman",
            Kind::Flatpak => "flatpak",
            Kind::Snap => "snap",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Program {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub kind: Kind,
    pub size: u64,
    /// true = measured on disk, false = what the installer claims (or unknown when 0)
    pub measured: bool,
    pub installed: String,
    pub location: Option<PathBuf>,
    pub uninstall_cmd: Option<String>,
    pub quiet_cmd: Option<String>,
    pub reg_key: Option<String>,
    /// System component / framework: listed but protected from removal
    pub protected: bool,
    pub running: bool,
}

impl Program {
    pub fn can_quiet(&self) -> bool {
        self.quiet_cmd.is_some()
    }
}

/// Everything installed, sorted by size (largest first).
pub fn list() -> Vec<Program> {
    if demo::on() {
        return demo::programs();
    }
    #[allow(unused_mut)]
    let mut v: Vec<Program> = Vec::new();
    #[cfg(windows)]
    v.extend(windows::list());
    #[cfg(target_os = "macos")]
    v.extend(macos::list());
    #[cfg(target_os = "linux")]
    v.extend(linux::list());
    mark_running(&mut v);
    v.sort_by(|a, b| b.size.cmp(&a.size).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

/// Measure folders of programs whose size is unknown or only claimed. Parallel.
pub fn measure(programs: &mut [Program]) {
    use rayon::prelude::*;
    if demo::on() {
        return;
    }
    programs.par_iter_mut().for_each(|p| {
        if p.measured {
            return;
        }
        if let Some(loc) = p.location.as_ref().filter(|l| l.is_dir() && bfs::is_safe(l)) {
            let s = bfs::size_of(loc);
            if s > 0 {
                p.size = s;
                p.measured = true;
            }
        }
    });
}

fn mark_running(v: &mut [Program]) {
    if demo::on() {
        return;
    }
    let procs: Vec<String> = util::sys::processes().into_iter().map(|(n, _)| n).collect();
    for p in v.iter_mut() {
        let Some(loc) = p.location.as_ref() else { continue };
        let Ok(rd) = std::fs::read_dir(loc) else { continue };
        let exes: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if cfg!(windows) {
                    n.strip_suffix(".exe").map(String::from)
                } else {
                    Some(n)
                }
            })
            .collect();
        p.running = exes.iter().any(|e| e.len() > 2 && procs.iter().any(|r| r == e));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UninstallMode {
    /// Silent where a silent command is known, else the program's own wizard.
    Auto,
    /// Always show the vendor's wizard.
    Interactive,
}

/// Run the uninstaller. Blocks until it finished (or a timeout). Returns a human-readable result.
pub fn uninstall(p: &Program, mode: UninstallMode) -> Result<String, String> {
    if p.protected {
        return Err("protected system component".into());
    }
    if bfs::dry() {
        return Ok("dry run".into());
    }
    util::log(format!("uninstall {} {} [{:?}] {:?}", p.name, p.version, p.kind, mode));
    #[cfg(windows)]
    return windows::uninstall(p, mode);
    #[cfg(target_os = "macos")]
    return macos::uninstall(p, mode);
    #[cfg(target_os = "linux")]
    return linux::uninstall(p, mode);
}

/// Is the program gone after uninstalling?
pub fn still_installed(p: &Program) -> bool {
    #[cfg(windows)]
    return windows::still_installed(p);
    #[cfg(not(windows))]
    return list().iter().any(|q| q.id == p.id);
}

// ---------------------------------------------------------------- leftovers

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Confidence {
    High,
    Medium,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum LeftoverKind {
    Folder,
    File,
    RegKey,
    RegValue { name: String },
    Service,
    Task,
    LaunchItem,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Leftover {
    pub kind: LeftoverKind,
    pub path: String,
    pub size: u64,
    pub confidence: Confidence,
    pub reason: String,
}

/// Normalize a product/publisher name for matching: lower-case alphanumerics, no versions,
/// no bitness markers, no legal suffixes.
pub fn norm_name(input: &str) -> String {
    // drop "(x64 en-US)", "[beta]" ...
    let mut s = String::new();
    let mut depth = 0i32;
    for c in input.to_lowercase().chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => s.push(c),
            _ => {}
        }
    }
    for junk in [
        " x64",
        " x86",
        " arm64",
        "64-bit",
        "32-bit",
        " inc.",
        " inc",
        " llc",
        " ltd.",
        " ltd",
        " gmbh",
        " corporation",
        " corp.",
        " corp",
        " co.",
        " s.a.",
        " ag",
    ] {
        if let Some(stripped) = s.strip_suffix(junk) {
            s = stripped.to_string();
        }
        s = s.replace(&format!("{junk} "), " ");
    }
    // drop version-like words ("2.3.1", "v24")
    let is_version = |w: &str| {
        let w = w.trim_start_matches('v');
        !w.is_empty() && w.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-')
    };
    s.split_whitespace().filter(|w| !is_version(w)).collect::<String>().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Leftovers of `p`, given the full list of still-installed programs (to avoid touching shared vendor folders).
pub fn scan_leftovers(p: &Program, all: &[Program]) -> Vec<Leftover> {
    let others: Vec<&Program> = all.iter().filter(|o| o.id != p.id).collect();
    #[allow(unused_mut)]
    let mut v: Vec<Leftover> = Vec::new();
    #[cfg(windows)]
    v.extend(windows::leftovers(p, &others));
    #[cfg(target_os = "macos")]
    v.extend(macos::leftovers(p, &others));
    #[cfg(target_os = "linux")]
    v.extend(linux::leftovers(p, &others));
    // de-duplicate and drop children of already-listed folders
    v.sort_by_key(|a| a.path.to_lowercase());
    let mut out: Vec<Leftover> = Vec::new();
    for l in v {
        let lp = l.path.to_lowercase();
        if out.iter().any(|o| {
            let op = o.path.to_lowercase();
            lp == op
                || (matches!(o.kind, LeftoverKind::Folder | LeftoverKind::RegKey) && lp.starts_with(&format!("{op}{}", sep_for(&o.kind))))
        }) {
            continue;
        }
        out.push(l);
    }
    out.sort_by(|a, b| a.confidence.cmp(&b.confidence).then(b.size.cmp(&a.size)));
    out
}

fn sep_for(k: &LeftoverKind) -> char {
    match k {
        LeftoverKind::RegKey => '\\',
        _ => std::path::MAIN_SEPARATOR,
    }
}

/// Remove leftovers: files and folders go to quarantine, registry is exported first,
/// services/tasks are stopped and deleted. Returns (removed, bytes, quarantine folder).
pub fn remove_leftovers(label: &str, items: &[Leftover]) -> (usize, u64, PathBuf) {
    let qdir = bfs::quarantine_dir(label);
    let mut n = 0;
    let mut bytes = 0;
    for l in items {
        let ok = match &l.kind {
            LeftoverKind::Folder | LeftoverKind::File => {
                bfs::quarantine(std::path::Path::new(&l.path), &qdir).map(|e| bytes += e.bytes).is_some()
            }
            #[cfg(windows)]
            LeftoverKind::RegKey => crate::util::reg::delete_key(&l.path).is_ok(),
            #[cfg(windows)]
            LeftoverKind::RegValue { name } => crate::util::reg::delete_value(&l.path, name).is_ok(),
            #[cfg(windows)]
            LeftoverKind::Service => windows::delete_service(&l.path),
            #[cfg(windows)]
            LeftoverKind::Task => bfs::dry() || util::run_status("schtasks.exe", &["/delete", "/f", "/tn", &l.path]).0 == 0,
            #[cfg(not(windows))]
            LeftoverKind::LaunchItem => {
                if !bfs::dry() {
                    let _ = util::run_status("launchctl", &["unload", &l.path]);
                }
                bfs::quarantine(std::path::Path::new(&l.path), &qdir).is_some()
            }
            #[allow(unreachable_patterns)]
            _ => false,
        };
        util::log(format!("leftover {:?} {} -> {}", l.kind, l.path, if ok { "removed" } else { "failed" }));
        if ok {
            n += 1;
        }
    }
    (n, bytes, qdir)
}

/// Common file-system leftover search: `roots` are scanned one level deep for folders matching the
/// program (or `publisher/program`).
pub(crate) fn folder_leftovers(p: &Program, others: &[&Program], roots: &[PathBuf], out: &mut Vec<Leftover>) {
    let name = norm_name(&p.name);
    let publisher = norm_name(&p.publisher);
    if name.len() < 3 {
        return;
    }
    let publisher_shared = publisher.is_empty() || others.iter().any(|o| norm_name(&o.publisher) == publisher);
    let other_names: Vec<String> = others.iter().map(|o| norm_name(&o.name)).collect();
    // A folder name matching another installed program exactly is never ours.
    let ours = |folder: &str| -> bool {
        let f = norm_name(folder);
        f.len() >= 3 && (f == name || (name.starts_with(&f) && f.len() * 10 >= name.len() * 7)) && !other_names.contains(&f)
    };
    for root in roots {
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(md) = std::fs::symlink_metadata(&path) else { continue };
            if bfs::is_link(&md) {
                continue;
            }
            let fname = e.file_name().to_string_lossy().to_string();
            if md.is_dir() && ours(&fname) {
                out.push(Leftover {
                    kind: LeftoverKind::Folder,
                    size: bfs::size_of(&path),
                    path: path.to_string_lossy().into(),
                    confidence: Confidence::High,
                    reason: "folder named after the program".into(),
                });
                continue;
            }
            if md.is_dir() && !publisher.is_empty() && norm_name(&fname) == publisher {
                // vendor folder: look for the product inside
                if let Ok(inner) = std::fs::read_dir(&path) {
                    for i in inner.flatten() {
                        if i.path().is_dir() && ours(&i.file_name().to_string_lossy()) {
                            out.push(Leftover {
                                kind: LeftoverKind::Folder,
                                size: bfs::size_of(&i.path()),
                                path: i.path().to_string_lossy().into(),
                                confidence: Confidence::High,
                                reason: "vendor\\product folder".into(),
                            });
                        }
                    }
                }
                if !publisher_shared && bfs::tree_is_empty(&path) {
                    out.push(Leftover {
                        kind: LeftoverKind::Folder,
                        size: 0,
                        path: path.to_string_lossy().into(),
                        confidence: Confidence::Medium,
                        reason: "empty vendor folder, no other product of this vendor".into(),
                    });
                }
            }
            if md.is_file() && fname.to_lowercase().ends_with(".lnk") && ours(fname.trim_end_matches(".lnk")) {
                out.push(Leftover {
                    kind: LeftoverKind::File,
                    size: md.len(),
                    path: path.to_string_lossy().into(),
                    confidence: Confidence::High,
                    reason: "shortcut named after the program".into(),
                });
            }
        }
    }
    if let Some(loc) = p.location.as_ref().filter(|l| l.exists() && bfs::is_safe(l)) {
        out.push(Leftover {
            kind: LeftoverKind::Folder,
            size: bfs::size_of(loc),
            path: loc.to_string_lossy().into(),
            confidence: Confidence::High,
            reason: "install folder still exists".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::norm_name;
    #[test]
    fn names() {
        assert_eq!(norm_name("Mozilla Firefox (x64 en-US)"), norm_name("Mozilla Firefox"));
        assert_eq!(norm_name("7-Zip 24.08 (x64)"), "7zip");
        assert_eq!(norm_name("Discord Inc."), "discord");
    }
}
