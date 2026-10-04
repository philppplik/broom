//! Linux cleaning items.

use super::{Item, Outcome};
use crate::util::fs::{self as bfs, Walk};
use crate::util::{self, Risk};
use std::path::PathBuf;

fn homes(rel: &[&str]) -> Vec<PathBuf> {
    util::profiles().iter().flat_map(|h| rel.iter().map(move |r| h.join(r))).collect()
}

fn clear_many(paths: Vec<PathBuf>, w: &Walk) -> Outcome {
    let mut o = Outcome::default();
    for p in paths {
        o += bfs::clear(p, w);
    }
    o
}

fn root_note() -> Option<Outcome> {
    (!util::is_admin()).then(|| Outcome::default().note("needs sudo - run `sudo broom`"))
}

fn thumbnails() -> Outcome {
    clear_many(homes(&[".cache/thumbnails", ".thumbnails"]), &Walk::all())
}
fn user_cache() -> Outcome {
    // ~/.cache minus things owned by other items or holding state
    let keep = [
        "mozilla",
        "google-chrome",
        "chromium",
        "BraveSoftware",
        "microsoft-edge",
        "vivaldi",
        "opera",
        "thumbnails",
        "pip",
        "yarn",
        "go-build",
        "pnpm",
        "uv",
        "huggingface",
        "torch",
        "JetBrains",
        "flatpak",
        "fontconfig",
        "keyring",
    ];
    let mut o = Outcome::default();
    for h in util::profiles() {
        let Ok(rd) = std::fs::read_dir(h.join(".cache")) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if keep.iter().any(|k| n.eq_ignore_ascii_case(k)) {
                continue;
            }
            o += bfs::clear(e.path(), &Walk::all().older(7));
        }
    }
    o
}
fn trash() -> Outcome {
    clear_many(homes(&[".local/share/Trash/files", ".local/share/Trash/info"]), &Walk::all())
}
fn tmp() -> Outcome {
    clear_many(vec!["/tmp".into(), "/var/tmp".into()], &Walk::all().older(7))
}
fn crash() -> Outcome {
    clear_many(vec!["/var/crash".into(), "/var/lib/systemd/coredump".into()], &Walk::all())
}
fn journal() -> Outcome {
    if let Some(o) = root_note() {
        return o;
    }
    if bfs::dry() {
        let out = util::run("journalctl", &["--disk-usage"]).unwrap_or_default();
        return Outcome::default().note(out.trim().to_string());
    }
    let _ = util::run_status("journalctl", &["--vacuum-size=200M", "--vacuum-time=4weeks"]);
    Outcome::default()
}
fn pkg_cache() -> Outcome {
    if let Some(o) = root_note() {
        return o;
    }
    let mut o = Outcome::default();
    if util::has_cmd("apt-get") {
        if bfs::dry() {
            o.bytes += bfs::size_of(std::path::Path::new("/var/cache/apt/archives"));
        } else {
            let _ = util::run_status("apt-get", &["clean"]);
        }
    }
    if util::has_cmd("dnf") {
        if bfs::dry() {
            o.bytes += bfs::size_of(std::path::Path::new("/var/cache/dnf"));
        } else {
            let _ = util::run_status("dnf", &["clean", "all"]);
        }
    }
    if util::has_cmd("pacman") {
        if bfs::dry() {
            o.bytes += bfs::size_of(std::path::Path::new("/var/cache/pacman/pkg"));
        } else if util::has_cmd("paccache") {
            let _ = util::run_status("paccache", &["-rk1"]);
        } else {
            let _ = util::run_status("pacman", &["-Sc", "--noconfirm"]);
        }
    }
    if util::has_cmd("zypper") && !bfs::dry() {
        let _ = util::run_status("zypper", &["clean", "--all"]);
    }
    o
}
fn orphans() -> Outcome {
    if let Some(o) = root_note() {
        return o;
    }
    if bfs::dry() {
        return Outcome::default().note("apt autoremove / dnf autoremove / pacman orphans");
    }
    if util::has_cmd("apt-get") {
        let _ = util::run_status("apt-get", &["autoremove", "-y", "--purge"]);
    }
    if util::has_cmd("dnf") {
        let _ = util::run_status("dnf", &["autoremove", "-y"]);
    }
    if util::has_cmd("pacman") {
        let q = util::run("pacman", &["-Qdtq"]).unwrap_or_default();
        let pk: Vec<&str> = q.lines().filter(|l| !l.is_empty()).collect();
        if !pk.is_empty() {
            let mut args = vec!["-Rns", "--noconfirm"];
            args.extend(pk);
            let _ = util::run_status("pacman", &args);
        }
    }
    Outcome::default().note("unused dependencies removed")
}
fn snap_old() -> Outcome {
    if !util::has_cmd("snap") {
        return Outcome::default().note("snap not installed");
    }
    let list = util::run("snap", &["list", "--all"]).unwrap_or_default();
    let disabled: Vec<(String, String)> = list
        .lines()
        .filter(|l| l.contains("disabled"))
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            Some((c.first()?.to_string(), c.get(2)?.to_string()))
        })
        .collect();
    if bfs::dry() || !util::is_admin() {
        return Outcome::default().note(format!(
            "{} old snap revisions{}",
            disabled.len(),
            if util::is_admin() { "" } else { " (needs sudo)" }
        ));
    }
    for (name, rev) in &disabled {
        let _ = util::run_status("snap", &["remove", name, "--revision", rev]);
    }
    Outcome::default().note(format!("{} old revisions removed", disabled.len()))
}
fn flatpak_unused() -> Outcome {
    if !util::has_cmd("flatpak") {
        return Outcome::default().note("flatpak not installed");
    }
    if bfs::dry() {
        return Outcome::default().note("removes runtimes no app needs");
    }
    let _ = util::run_status("flatpak", &["uninstall", "--unused", "-y", "--noninteractive"]);
    Outcome::default()
}

fn mk(id: &'static str, cat: &'static str, name: &'static str, tier: u8, risk: Risk, desc: &'static str, run: fn() -> Outcome) -> Item {
    Item { id, category: cat, name, tier, risk, desc, procs: &[], delta: false, slow: false, run }
}

pub fn items() -> Vec<Item> {
    let mut v = vec![
        mk("thumbs", "System junk", "Thumbnail cache", 1, Risk::Safe, "~/.cache/thumbnails - rebuilt by your file manager.", thumbnails),
        mk(
            "cache",
            "System junk",
            "User cache (~/.cache, older than 7 days)",
            1,
            Risk::Safe,
            "Per-app caches. Browser and developer caches are handled by their own items.",
            user_cache,
        ),
        mk("trash", "System junk", "Trash", 1, Risk::Moderate, "Empties your desktop Trash.", trash),
        mk("tmp", "System junk", "Temporary files older than 7 days", 2, Risk::Safe, "/tmp and /var/tmp.", tmp),
        mk("crash", "System junk", "Crash dumps", 1, Risk::Safe, "/var/crash and systemd coredumps (needs sudo).", crash),
        mk(
            "journal",
            "System junk",
            "Shrink systemd journal to 200 MB / 4 weeks",
            2,
            Risk::Safe,
            "journalctl --vacuum-size=200M --vacuum-time=4weeks (needs sudo).",
            journal,
        ),
        mk(
            "pkgcache",
            "Updates & installers",
            "Package manager download cache",
            1,
            Risk::Safe,
            "apt clean / dnf clean all / paccache -rk1 / zypper clean (needs sudo).",
            pkg_cache,
        ),
        mk(
            "orphans",
            "Updates & installers",
            "Unused dependencies",
            2,
            Risk::Moderate,
            "apt autoremove --purge / dnf autoremove / pacman -Rns orphans (needs sudo).",
            orphans,
        ),
        mk("snap", "Updates & installers", "Old snap revisions", 2, Risk::Safe, "Disabled snap revisions kept for rollback.", snap_old),
        mk("flatpak", "Updates & installers", "Unused Flatpak runtimes", 2, Risk::Safe, "flatpak uninstall --unused.", flatpak_unused),
    ];
    for it in v.iter_mut() {
        if ["journal", "pkgcache", "orphans", "snap", "flatpak"].contains(&it.id) {
            it.delta = true;
            it.slow = true;
        }
    }
    v
}
