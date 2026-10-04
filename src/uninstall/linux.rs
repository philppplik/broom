//! Linux: dpkg / rpm / pacman packages, Flatpak and Snap apps.

use super::{Confidence, Kind, Leftover, LeftoverKind, Program, UninstallMode};
use crate::util::{self, fs as bfs};

fn prog(kind: Kind, name: &str, version: &str, size_kb: u64, publisher: &str) -> Program {
    Program {
        id: format!("{}:{name}", kind.label()),
        name: name.into(),
        version: version.into(),
        publisher: publisher.into(),
        kind,
        size: size_kb * 1024,
        measured: false,
        installed: String::new(),
        location: None,
        uninstall_cmd: None,
        quiet_cmd: None,
        reg_key: None,
        protected: false,
        running: false,
    }
}

/// Packages the system needs: never offered for removal.
const ESSENTIAL: &[&str] = &[
    "linux-image",
    "linux-firmware",
    "systemd",
    "libc6",
    "glibc",
    "bash",
    "coreutils",
    "sudo",
    "apt",
    "dpkg",
    "rpm",
    "dnf",
    "pacman",
    "base",
    "grub",
    "shim",
    "kernel",
    "util-linux",
    "openssh-server",
    "network-manager",
    "NetworkManager",
];

fn essential(name: &str) -> bool {
    ESSENTIAL.iter().any(|e| name == *e || name.starts_with(&format!("{e}-")))
}

pub fn list() -> Vec<Program> {
    let mut v = Vec::new();
    if util::has_cmd("dpkg-query") {
        let out = util::run("dpkg-query", &["-W", "-f=${Package}\t${Version}\t${Installed-Size}\t${Priority}\t${Maintainer}\n"])
            .unwrap_or_default();
        for l in out.lines() {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() < 5 {
                continue;
            }
            let mut p = prog(Kind::Deb, c[0], c[1], c[2].parse().unwrap_or(0), c[4].split('<').next().unwrap_or("").trim());
            p.protected = essential(c[0]) || c[3] == "required" || c[3] == "important";
            v.push(p);
        }
    } else if util::has_cmd("rpm") {
        let out = util::run("rpm", &["-qa", "--qf", "%{NAME}\t%{VERSION}-%{RELEASE}\t%{SIZE}\t%{VENDOR}\n"]).unwrap_or_default();
        for l in out.lines() {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() < 4 {
                continue;
            }
            let mut p = prog(Kind::Rpm, c[0], c[1], c[2].parse::<u64>().unwrap_or(0) / 1024, c[3]);
            p.protected = essential(c[0]);
            v.push(p);
        }
    } else if util::has_cmd("pacman") {
        let out = util::run("pacman", &["-Qi"]).unwrap_or_default();
        for block in out.split("\n\n") {
            let field = |k: &str| {
                block
                    .lines()
                    .find_map(|l| l.strip_prefix(k).map(|r| r.trim_start_matches([' ', ':']).trim().to_string()))
                    .unwrap_or_default()
            };
            let name = field("Name");
            if name.is_empty() {
                continue;
            }
            let size = field("Installed Size");
            let kb = parse_size_kb(&size);
            let mut p = prog(Kind::Pacman, &name, &field("Version"), kb, &field("Packager"));
            p.protected = essential(&name);
            v.push(p);
        }
    }
    if util::has_cmd("flatpak") {
        let out = util::run("flatpak", &["list", "--app", "--columns=application,name,version,size"]).unwrap_or_default();
        for l in out.lines() {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() < 4 {
                continue;
            }
            let mut p = prog(Kind::Flatpak, c[0], c[2], parse_size_kb(c[3]), "Flatpak");
            p.name = format!("{} ({})", c[1], c[0]);
            p.id = format!("flatpak:{}", c[0]);
            v.push(p);
        }
    }
    if util::has_cmd("snap") {
        let out = util::run("snap", &["list"]).unwrap_or_default();
        for l in out.lines().skip(1) {
            let c: Vec<&str> = l.split_whitespace().collect();
            if c.len() < 5 {
                continue;
            }
            let mut p = prog(Kind::Snap, c[0], c[1], 0, c.get(4).copied().unwrap_or(""));
            p.protected = c.get(5).map(|n| n.contains("base") || n.contains("core") || n.contains("snapd")).unwrap_or(false)
                || c[0] == "snapd"
                || c[0].starts_with("core");
            p.location = Some(std::path::PathBuf::from(format!("/snap/{}/current", c[0])));
            v.push(p);
        }
    }
    v
}

fn parse_size_kb(s: &str) -> u64 {
    let s = s.replace(',', ".");
    let mut parts = s.split_whitespace();
    let n: f64 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let unit = parts.next().unwrap_or("KiB").to_lowercase();
    let mult = match unit.as_str() {
        u if u.starts_with('g') => 1024.0 * 1024.0,
        u if u.starts_with('m') => 1024.0,
        u if u.starts_with('b') => 1.0 / 1024.0,
        _ => 1.0,
    };
    (n * mult) as u64
}

pub fn uninstall(p: &Program, _mode: UninstallMode) -> Result<String, String> {
    let name = p.id.split_once(':').map(|x| x.1).unwrap_or(&p.name).to_string();
    let (c, o) = match p.kind {
        Kind::Deb => util::run_status("apt-get", &["remove", "-y", &name]),
        Kind::Rpm => util::run_status(if util::has_cmd("dnf") { "dnf" } else { "zypper" }, &["remove", "-y", &name]),
        Kind::Pacman => util::run_status("pacman", &["-R", "--noconfirm", &name]),
        Kind::Flatpak => util::run_status("flatpak", &["uninstall", "-y", "--noninteractive", &name]),
        Kind::Snap => util::run_status("snap", &["remove", &name]),
        _ => return Err("unsupported".into()),
    };
    if c == 0 {
        Ok("removed".into())
    } else {
        Err(o.lines().last().unwrap_or("failed").to_string())
    }
}

pub fn leftovers(p: &Program, others: &[&Program]) -> Vec<Leftover> {
    let mut out = Vec::new();
    let h = util::home();
    let name = p.id.split_once(':').map(|x| x.1).unwrap_or(&p.name).to_string();
    if p.kind == Kind::Flatpak {
        let d = h.join(".var/app").join(&name);
        if d.is_dir() {
            out.push(Leftover {
                kind: LeftoverKind::Folder,
                size: bfs::size_of(&d),
                path: d.to_string_lossy().into(),
                confidence: Confidence::High,
                reason: "Flatpak app data".into(),
            });
        }
        return out;
    }
    if p.kind == Kind::Snap {
        let d = h.join("snap").join(&name);
        if d.is_dir() {
            out.push(Leftover {
                kind: LeftoverKind::Folder,
                size: bfs::size_of(&d),
                path: d.to_string_lossy().into(),
                confidence: Confidence::High,
                reason: "Snap app data".into(),
            });
        }
        return out;
    }
    // never $HOME itself: a package "git" must not match the user's ~/git projects folder
    let roots = [h.join(".config"), h.join(".local/share"), h.join(".cache")];
    // dot-folders in $HOME named after the package (e.g. ~/.vscode for "code" won't match - by design)
    let mut q = p.clone();
    q.name = name.clone();
    super::folder_leftovers(&q, others, &roots, &mut out);
    if let Ok(rd) = std::fs::read_dir(&h) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.strip_prefix('.').map(|x| x == name).unwrap_or(false) {
                out.push(Leftover {
                    kind: LeftoverKind::Folder,
                    size: bfs::size_of(&e.path()),
                    path: e.path().to_string_lossy().into(),
                    confidence: Confidence::High,
                    reason: "hidden folder named after the package".into(),
                });
            }
        }
    }
    out
}
