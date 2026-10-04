//! Linux tweaks: kernel parameters (persisted), systemd units and GNOME settings.

use super::{Op, Tweak};
use crate::util::Risk;

fn sysctl(k: &str, v: &str) -> Op {
    Op::Sysctl { key: k.into(), value: v.into() }
}
fn gs(schema: &str, key: &str, v: &str) -> Op {
    Op::Gsettings { schema: schema.into(), key: key.into(), value: v.into() }
}
fn cmd(p: &str, a: &[&str]) -> Op {
    Op::Cmd { prog: p.into(), args: a.iter().map(|s| s.to_string()).collect() }
}

fn t(id: &str, cat: &str, name: &str, risk: Risk, desc: &str, apply: Vec<Op>, undo: Vec<Op>) -> Tweak {
    Tweak {
        id: id.into(),
        name: name.into(),
        category: cat.into(),
        desc: desc.into(),
        risk,
        source: "Broom",
        link: None,
        apply,
        undo,
        restart: false,
        hint: None,
    }
}

pub fn tweaks() -> Vec<Tweak> {
    use Risk::*;
    const I: &str = "org.gnome.desktop.interface";
    vec![
        t(
            "swappiness",
            "Performance",
            "Swappiness 10 (keep apps in RAM)",
            Safe,
            "Swap later; desktops feel snappier under memory pressure. Needs sudo.",
            vec![sysctl("vm.swappiness", "10")],
            vec![],
        ),
        t(
            "vfs-cache",
            "Performance",
            "Keep directory cache longer (vfs_cache_pressure 50)",
            Safe,
            "Faster repeated file browsing. Needs sudo.",
            vec![sysctl("vm.vfs_cache_pressure", "50")],
            vec![],
        ),
        t(
            "inotify",
            "Performance",
            "More file watchers (IDEs, sync tools)",
            Safe,
            "Raises fs.inotify.max_user_watches to 524288 - fixes 'too many open files' in VS Code, webpack, Syncthing. Needs sudo.",
            vec![sysctl("fs.inotify.max_user_watches", "524288")],
            vec![],
        ),
        t(
            "fstrim",
            "System",
            "Weekly SSD TRIM (fstrim.timer)",
            Safe,
            "Keeps SSDs fast. Needs sudo.",
            vec![cmd("systemctl", &["enable", "--now", "fstrim.timer"])],
            vec![cmd("systemctl", &["disable", "--now", "fstrim.timer"])],
        ),
        t(
            "journal-limit",
            "System",
            "Cap the systemd journal at 200 MB",
            Safe,
            "Stops logs from slowly eating gigabytes. Needs sudo.",
            vec![
                Op::File {
                    path: "/etc/systemd/journald.conf.d/50-broom.conf".into(),
                    content: Some("[Journal]\nSystemMaxUse=200M\n".into()),
                },
                cmd("systemctl", &["restart", "systemd-journald"]),
            ],
            vec![],
        ),
        t(
            "anim-off",
            "Desktop",
            "GNOME: animations off",
            Safe,
            "Windows and overview appear instantly.",
            vec![gs(I, "enable-animations", "false")],
            vec![],
        ),
        t(
            "battery-pct",
            "Desktop",
            "GNOME: battery percentage in the top bar",
            Safe,
            "Shows the exact charge.",
            vec![gs(I, "show-battery-percentage", "true")],
            vec![],
        ),
        t(
            "weekday",
            "Desktop",
            "GNOME: weekday and seconds in the clock",
            Safe,
            "More useful top-bar clock.",
            vec![gs(I, "clock-show-weekday", "true"), gs(I, "clock-show-seconds", "true")],
            vec![],
        ),
        t(
            "hot-corner-off",
            "Desktop",
            "GNOME: hot corner off",
            Safe,
            "No accidental Activities overview when the mouse hits the corner.",
            vec![gs(I, "enable-hot-corners", "false")],
            vec![],
        ),
        t(
            "tracker-off",
            "Performance",
            "GNOME: file indexing (Tracker/LocalSearch) off",
            Moderate,
            "Less background disk/CPU; search in Files becomes slower.",
            vec![gs("org.freedesktop.Tracker3.Miner.Files", "index-recursive-directories", "@as []")],
            vec![],
        ),
        t(
            "problem-reports-off",
            "Privacy",
            "GNOME: automatic problem reports off",
            Safe,
            "Crash reports are not sent automatically.",
            vec![gs("org.gnome.desktop.privacy", "report-technical-problems", "false")],
            vec![],
        ),
        t(
            "recent-files-off",
            "Privacy",
            "GNOME: don't remember recent files",
            Safe,
            "No recently-used list in Files and apps.",
            vec![gs("org.gnome.desktop.privacy", "remember-recent-files", "false")],
            vec![],
        ),
    ]
}
