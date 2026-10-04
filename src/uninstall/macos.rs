//! macOS: .app bundles (with AppCleaner-style leftover search by bundle id) and Homebrew.

use super::{Confidence, Kind, Leftover, LeftoverKind, Program, UninstallMode};
use crate::util::{self, fs as bfs};
use std::path::{Path, PathBuf};

fn plist(app: &Path, key: &str) -> String {
    util::run("plutil", &["-extract", key, "raw", &app.join("Contents/Info.plist").to_string_lossy()])
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn apps() -> Vec<Program> {
    let mut v = Vec::new();
    let dirs = [PathBuf::from("/Applications"), util::home().join("Applications"), PathBuf::from("/Applications/Utilities")];
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x != "app").unwrap_or(true) {
                continue;
            }
            let bundle = plist(&p, "CFBundleIdentifier");
            let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let installed = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            v.push(Program {
                id: format!("app:{}", p.display()),
                version: plist(&p, "CFBundleShortVersionString"),
                publisher: bundle.split('.').nth(1).unwrap_or("").to_string(),
                kind: Kind::App,
                size: 0,
                measured: false,
                installed,
                // Apple's own apps live on the sealed system volume
                protected: bundle.starts_with("com.apple."),
                location: Some(p),
                uninstall_cmd: None,
                quiet_cmd: Some(bundle),
                reg_key: None,
                running: false,
                name,
            });
        }
    }
    v
}

fn brew() -> Vec<Program> {
    if !util::has_cmd("brew") {
        return vec![];
    }
    let mut v = Vec::new();
    for (flag, kind) in [("--formula", Kind::Brew), ("--cask", Kind::Cask)] {
        let out = util::run("brew", &["list", flag, "--versions"]).unwrap_or_default();
        for l in out.lines() {
            let mut it = l.split_whitespace();
            let Some(name) = it.next() else { continue };
            v.push(Program {
                id: format!("{}:{name}", kind.label()),
                name: name.into(),
                version: it.next().unwrap_or("").into(),
                publisher: "Homebrew".into(),
                kind,
                size: 0,
                measured: false,
                installed: String::new(),
                location: None,
                uninstall_cmd: Some(format!("brew uninstall {flag} {name}")),
                quiet_cmd: Some(format!("brew uninstall {flag} {name}")),
                reg_key: None,
                protected: false,
                running: false,
            });
        }
    }
    v
}

pub fn list() -> Vec<Program> {
    let mut v = apps();
    // casks also install .app bundles: hide the duplicate app entry
    let b = brew();
    v.retain(|a| !b.iter().any(|c| c.kind == Kind::Cask && super::norm_name(&c.name) == super::norm_name(&a.name)));
    v.extend(b);
    v
}

pub fn uninstall(p: &Program, _mode: UninstallMode) -> Result<String, String> {
    match p.kind {
        Kind::App => {
            let loc = p.location.clone().ok_or("no location")?;
            // move to the user's Trash so it can be restored from Finder
            let script = format!("tell application \"Finder\" to delete POSIX file \"{}\"", loc.display());
            let (c, o) = util::run_status("osascript", &["-e", &script]);
            if c == 0 && !loc.exists() {
                Ok("moved to Trash".into())
            } else {
                Err(format!("could not remove (needs sudo or app is running?) {}", o.trim()))
            }
        }
        Kind::Brew | Kind::Cask => {
            let flag = if p.kind == Kind::Cask { "--cask" } else { "--formula" };
            let (c, o) = util::run_status("brew", &["uninstall", flag, &p.name]);
            if c == 0 {
                Ok("removed".into())
            } else {
                Err(o.lines().last().unwrap_or("brew failed").to_string())
            }
        }
        _ => Err("unsupported".into()),
    }
}

pub fn leftovers(p: &Program, others: &[&Program]) -> Vec<Leftover> {
    let mut out = Vec::new();
    let bundle = p.quiet_cmd.clone().filter(|_| p.kind == Kind::App).unwrap_or_default();
    let h = util::home();
    let lib = h.join("Library");
    // by bundle id: exact, unambiguous
    if bundle.contains('.') {
        let candidates = [
            lib.join("Application Support").join(&bundle),
            lib.join("Caches").join(&bundle),
            lib.join("Containers").join(&bundle),
            lib.join("HTTPStorages").join(&bundle),
            lib.join("WebKit").join(&bundle),
            lib.join("Logs").join(&bundle),
            lib.join("Saved Application State").join(format!("{bundle}.savedState")),
            lib.join("Preferences").join(format!("{bundle}.plist")),
            lib.join("Cookies").join(format!("{bundle}.binarycookies")),
        ];
        for c in candidates {
            if let Ok(md) = std::fs::symlink_metadata(&c) {
                out.push(Leftover {
                    kind: if md.is_dir() { LeftoverKind::Folder } else { LeftoverKind::File },
                    size: if md.is_dir() { bfs::size_of(&c) } else { md.len() },
                    path: c.to_string_lossy().into(),
                    confidence: Confidence::High,
                    reason: format!("belongs to {bundle}"),
                });
            }
        }
        for dir in [
            lib.join("Group Containers"),
            lib.join("LaunchAgents"),
            PathBuf::from("/Library/LaunchAgents"),
            PathBuf::from("/Library/LaunchDaemons"),
            lib.join("Preferences/ByHost"),
        ] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.contains(&bundle) {
                    let launch = dir.to_string_lossy().contains("Launch");
                    out.push(Leftover {
                        kind: if launch {
                            LeftoverKind::LaunchItem
                        } else if e.path().is_dir() {
                            LeftoverKind::Folder
                        } else {
                            LeftoverKind::File
                        },
                        size: bfs::size_of(&e.path()),
                        path: e.path().to_string_lossy().into(),
                        confidence: Confidence::High,
                        reason: format!("contains {bundle}"),
                    });
                }
            }
        }
    }
    // by name in Application Support / Caches (BCU-style matching)
    super::folder_leftovers(p, others, &[lib.join("Application Support"), lib.join("Caches"), lib.join("Logs")], &mut out);
    out.retain(|l| {
        p.location.as_ref().map(|loc| Path::new(&l.path) != loc.as_path()).unwrap_or(true)
            || p.kind != Kind::App
            || Path::new(&l.path).exists()
    });
    out
}
