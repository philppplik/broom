//! macOS cleaning items.

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

/// User caches, except the ones that hold data or are owned by the browser items.
fn user_caches() -> Outcome {
    let keep = [
        "com.apple.HomeKit",
        "com.apple.Safari",
        "CloudKit",
        "com.apple.bird",
        "com.apple.akd",
        "Google",
        "Microsoft Edge",
        "BraveSoftware",
        "Firefox",
        "com.apple.containermanagerd",
        "com.apple.nsurlsessiond",
        "FamilyCircle",
    ];
    let mut o = Outcome::default();
    for h in util::profiles() {
        let Ok(rd) = std::fs::read_dir(h.join("Library/Caches")) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if keep.iter().any(|k| n.starts_with(k)) {
                continue;
            }
            o += bfs::clear_all(e.path());
        }
    }
    o
}
fn user_logs() -> Outcome {
    clear_many(homes(&["Library/Logs"]), &Walk::all().older(3))
}
fn system_logs() -> Outcome {
    clear_many(vec!["/Library/Logs".into(), "/private/var/log/asl".into()], &Walk::pattern(&["*.log", "*.asl", "*.gz", "*.bz2"]).older(7))
}
fn crash_reports() -> Outcome {
    clear_many(homes(&["Library/Logs/DiagnosticReports"]), &Walk::all())
}
fn trash() -> Outcome {
    let mut o = clear_many(homes(&[".Trash"]), &Walk::all());
    for d in util::sys::disks() {
        if let Ok(uid) = util::run("id", &["-u"]) {
            o += bfs::clear_all(d.mount.join(".Trashes").join(uid.trim()));
        }
    }
    o
}
fn xcode() -> Outcome {
    clear_many(
        homes(&[
            "Library/Developer/Xcode/DerivedData",
            "Library/Developer/Xcode/Archives",
            "Library/Developer/Xcode/iOS DeviceSupport",
            "Library/Developer/Xcode/watchOS DeviceSupport",
            "Library/Developer/CoreSimulator/Caches",
            "Library/Caches/com.apple.dt.Xcode",
        ]),
        &Walk::all(),
    )
}
fn simulators() -> Outcome {
    if !util::has_cmd("xcrun") {
        return Outcome::default().note("Xcode not installed");
    }
    if bfs::dry() {
        return Outcome::default().note("removes simulators of uninstalled runtimes");
    }
    let _ = util::run_status("xcrun", &["simctl", "delete", "unavailable"]);
    Outcome::default().note("unavailable simulators deleted")
}
fn brew() -> Outcome {
    if !util::has_cmd("brew") {
        return Outcome::default().note("Homebrew not installed");
    }
    if bfs::dry() {
        let out = util::run("brew", &["cleanup", "-n", "--prune=all"]).unwrap_or_default();
        return Outcome::default().note(out.lines().last().unwrap_or("nothing to clean").to_string());
    }
    let _ = util::run_status("brew", &["cleanup", "--prune=all", "-s"]);
    let _ = util::run_status("brew", &["autoremove"]);
    Outcome::default()
}
fn ios_backups_info() -> Outcome {
    let size: u64 = homes(&["Library/Application Support/MobileSync/Backup"]).iter().map(|p| bfs::size_of(p)).sum();
    Outcome::default().note(if size > 0 {
        format!("{} of old iPhone/iPad backups - review in Finder > your device", util::fmt_size(size))
    } else {
        "none".into()
    })
}
fn mail_downloads() -> Outcome {
    clear_many(homes(&["Library/Containers/com.apple.mail/Data/Library/Mail Downloads"]), &Walk::all())
}
fn quicklook() -> Outcome {
    if !bfs::dry() {
        let _ = util::run_status("qlmanage", &["-r", "cache"]);
    }
    Outcome::default().note("thumbnail cache reset")
}
fn dns_flush() -> Outcome {
    if bfs::dry() {
        return Outcome::default().note("would flush");
    }
    let _ = util::run_status("dscacheutil", &["-flushcache"]);
    let _ = util::run_status("killall", &["-HUP", "mDNSResponder"]);
    Outcome::default().note("flushed")
}
fn safari_cache() -> Outcome {
    clear_many(homes(&["Library/Caches/com.apple.Safari", "Library/Containers/com.apple.Safari/Data/Library/Caches"]), &Walk::all())
}

fn mk(id: &'static str, cat: &'static str, name: &'static str, tier: u8, risk: Risk, desc: &'static str, run: fn() -> Outcome) -> Item {
    Item { id, category: cat, name, tier, risk, desc, procs: &[], delta: false, slow: false, run }
}

pub fn items() -> Vec<Item> {
    let mut v = vec![
        mk(
            "caches",
            "System junk",
            "User caches (~/Library/Caches)",
            1,
            Risk::Safe,
            "App caches in your Library. Apps rebuild them; iCloud and keychain caches are kept.",
            user_caches,
        ),
        mk("logs", "System junk", "User logs older than 3 days", 1, Risk::Safe, "~/Library/Logs.", user_logs),
        mk(
            "syslogs",
            "System junk",
            "System logs older than 7 days",
            2,
            Risk::Safe,
            "/Library/Logs and ASL archives (needs sudo).",
            system_logs,
        ),
        mk("crash", "System junk", "Crash reports", 1, Risk::Safe, "~/Library/Logs/DiagnosticReports.", crash_reports),
        mk("trash", "System junk", "Trash (all volumes)", 1, Risk::Moderate, "Empties the Trash. Files can no longer be put back.", trash),
        mk("quicklook", "System junk", "Quick Look thumbnail cache", 2, Risk::Safe, "qlmanage -r cache. Rebuilt automatically.", quicklook),
        mk("dns", "System junk", "DNS cache", 1, Risk::Safe, "Flushes macOS's resolver cache.", dns_flush),
        mk(
            "maildl",
            "Browsers & apps",
            "Mail attachments cache",
            2,
            Risk::Moderate,
            "Attachments Mail extracted when you opened them. Still in the mails themselves.",
            mail_downloads,
        ),
        mk(
            "safari",
            "Browsers & apps",
            "Safari cache",
            1,
            Risk::Safe,
            "Web cache only - history, cookies and passwords stay.",
            safari_cache,
        ),
        mk(
            "xcode",
            "Developer",
            "Xcode DerivedData, archives & device support",
            2,
            Risk::Moderate,
            "Rebuildable build output and debug symbols for old iOS versions. Archives of shipped builds are removed too.",
            xcode,
        ),
        mk("simulators", "Developer", "Unavailable iOS simulators", 2, Risk::Safe, "xcrun simctl delete unavailable.", simulators),
        mk(
            "brew",
            "Developer",
            "Homebrew cleanup",
            1,
            Risk::Safe,
            "brew cleanup --prune=all and autoremove: old versions and downloads.",
            brew,
        ),
        mk(
            "iosbackups",
            "Developer",
            "Old iPhone/iPad backups (report only)",
            3,
            Risk::Safe,
            "Shows how much space device backups use. Broom never deletes them - manage them in Finder.",
            ios_backups_info,
        ),
    ];
    for it in v.iter_mut() {
        if it.id == "brew" || it.id == "simulators" {
            it.slow = true;
        }
        if it.id == "safari" {
            it.procs = &["safari"];
        }
    }
    v
}
