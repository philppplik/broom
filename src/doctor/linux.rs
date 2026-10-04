//! Linux doctor: /proc, /sys, systemd, package managers.

use super::{Action, Battery, Finding, Hardware, PhysDisk, Severity};
use crate::util::{self, Risk};
use std::path::Path;

fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_default().trim().to_string()
}

pub fn hardware() -> Hardware {
    let mut hw = super::base_hardware();
    hw.manufacturer = read("/sys/class/dmi/id/sys_vendor");
    hw.model = read("/sys/class/dmi/id/product_name");
    let chassis: u32 = read("/sys/class/dmi/id/chassis_type").parse().unwrap_or(0);
    hw.is_laptop = [8, 9, 10, 14, 30, 31, 32].contains(&chassis);
    if let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") {
        for e in rd.flatten() {
            let p = e.path();
            if !e.file_name().to_string_lossy().starts_with("BAT") {
                continue;
            }
            let r = |f: &str| std::fs::read_to_string(p.join(f)).ok().and_then(|s| s.trim().parse::<u64>().ok());
            let design = r("energy_full_design").or_else(|| r("charge_full_design")).unwrap_or(0) / 1000;
            let full = r("energy_full").or_else(|| r("charge_full")).unwrap_or(0) / 1000;
            if design > 0 {
                hw.battery = Some(Battery { design_mwh: design, full_mwh: full, cycles: r("cycle_count").filter(|c| *c > 0) });
                hw.is_laptop = true;
            }
        }
    }
    let lspci = util::run("lspci", &[]).unwrap_or_default();
    for l in lspci.lines().filter(|l| l.contains("VGA") || l.contains("3D controller") || l.contains("Display controller")) {
        let name = l.split(": ").nth(1).unwrap_or(l).to_string();
        let v = if name.contains("NVIDIA") {
            "NVIDIA"
        } else if name.contains("AMD") || name.contains("ATI") {
            "AMD"
        } else if name.contains("Intel") {
            "Intel"
        } else {
            "Other"
        };
        hw.gpu_vendors.push(v.into());
        hw.gpus.push(name);
    }
    let lsblk = util::run("lsblk", &["-J", "-b", "-d", "-o", "NAME,ROTA,SIZE,MODEL,TRAN,TYPE"]).unwrap_or_default();
    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&lsblk) {
        for d in j.get("blockdevices").and_then(|b| b.as_array()).cloned().unwrap_or_default() {
            if d.get("type").and_then(|t| t.as_str()) != Some("disk") {
                continue;
            }
            let rota = d.get("rota").map(|r| r.as_bool().unwrap_or(r.as_str() == Some("1"))).unwrap_or(false);
            let tran = d.get("tran").and_then(|t| t.as_str()).unwrap_or("");
            let name = d.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let health = if util::is_admin() && util::has_cmd("smartctl") {
                let o = util::run("smartctl", &["-H", &format!("/dev/{name}")]).unwrap_or_default();
                if o.contains("PASSED") || o.contains(": OK") {
                    "Healthy".into()
                } else if o.contains("FAILED") {
                    "FAILED".into()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            hw.disks.push(PhysDisk {
                model: d.get("model").and_then(|m| m.as_str()).unwrap_or(name).trim().to_string(),
                media: if tran == "nvme" {
                    "NVMe".into()
                } else if rota {
                    "HDD".into()
                } else {
                    "SSD".into()
                },
                size: d.get("size").and_then(|s| s.as_u64().or_else(|| s.as_str().and_then(|x| x.parse().ok()))).unwrap_or(0),
                health,
                temperature: None,
                wear: None,
            });
        }
    }
    let root_dev = util::run("findmnt", &["-n", "-o", "SOURCE", "/"]).unwrap_or_default();
    hw.system_disk_ssd =
        hw.disks.iter().find(|d| !d.model.is_empty() && root_dev.contains("nvme") == (d.media == "NVMe")).map(|d| d.media != "HDD");
    hw.firmware = if Path::new("/sys/firmware/efi").exists() { "UEFI".into() } else { "Legacy BIOS".into() };
    hw
}

pub fn findings(hw: &Hardware) -> Vec<Finding> {
    let mut v = Vec::new();
    let failed = util::run("systemctl", &["--failed", "--no-legend", "--plain"]).unwrap_or_default();
    let units: Vec<&str> = failed.lines().filter_map(|l| l.split_whitespace().next()).collect();
    if !units.is_empty() {
        v.push(
            Finding::new("Stability", Severity::Warning, format!("{} failed system service(s)", units.len()), units.join(", "))
                .action("failedunits"),
        );
    } else {
        v.push(Finding::new("Stability", Severity::Ok, "All system services running", ""));
    }
    if Path::new("/var/run/reboot-required").exists() {
        v.push(Finding::new("Updates", Severity::Warning, "Restart required", "A kernel or library update waits for a reboot."));
    }
    let errors = util::run("journalctl", &["-p", "3", "-b", "--no-pager", "-q", "-o", "cat"]).unwrap_or_default().lines().count();
    if errors > 50 {
        v.push(Finding::new(
            "Stability",
            Severity::Advice,
            format!("{errors} error messages since boot"),
            "Check `journalctl -p 3 -b` for the noisy ones.",
        ));
    }
    let oom = util::run("journalctl", &["-k", "-b", "--no-pager", "-q", "-o", "cat", "--grep", "Out of memory"])
        .unwrap_or_default()
        .lines()
        .count();
    if oom > 0 {
        v.push(Finding::new(
            "Memory",
            Severity::Warning,
            format!("{oom} out-of-memory kill(s) since boot"),
            "Apps were killed for lack of RAM. Add swap/zram or close heavy apps.",
        ));
    }
    let swap = read("/proc/swaps").lines().count() > 1;
    if !swap {
        v.push(Finding::new(
            "Memory",
            Severity::Advice,
            "No swap configured",
            "Without swap, running out of RAM kills apps immediately. Consider zram (e.g. zram-generator).",
        ));
    }
    if hw.system_disk_ssd == Some(true) {
        let t = util::run("systemctl", &["is-enabled", "fstrim.timer"]).unwrap_or_default();
        if !t.contains("enabled") {
            v.push(
                Finding::new("Storage", Severity::Advice, "Periodic SSD TRIM is off", "Enable fstrim.timer to keep the SSD fast.")
                    .tweak("fstrim"),
            );
        }
    }
    let swappiness: u32 = read("/proc/sys/vm/swappiness").parse().unwrap_or(60);
    if swappiness >= 60 && hw.ram_total >= (8u64 << 30) && swap {
        v.push(
            Finding::new(
                "Performance",
                Severity::Advice,
                format!("Swappiness is {swappiness}"),
                "With enough RAM, a lower value keeps apps responsive.",
            )
            .tweak("swappiness"),
        );
    }
    let ufw = util::run("ufw", &["status"]).unwrap_or_default();
    let firewalld = util::run("firewall-cmd", &["--state"]).unwrap_or_default();
    if ufw.contains("inactive") && !firewalld.contains("running") {
        v.push(Finding::new(
            "Security",
            Severity::Advice,
            "Firewall inactive",
            "Enable it with `sudo ufw enable` (allows outgoing, blocks incoming).",
        ));
    }
    let sb = util::run("mokutil", &["--sb-state"]).unwrap_or_default();
    if sb.contains("disabled") {
        v.push(Finding::new("Security", Severity::Advice, "Secure Boot is off", "Enable it in UEFI if your distribution supports it."));
    }
    let ntp = util::run("timedatectl", &["show", "-p", "NTPSynchronized", "--value"]).unwrap_or_default();
    if ntp.trim() == "no" {
        v.push(
            Finding::new("System", Severity::Advice, "Clock not synchronized", "Enable time sync: sudo timedatectl set-ntp true")
                .action("timesync"),
        );
    }
    if util::has_cmd("apt") {
        let up = util::run("apt", &["list", "--upgradable"]).unwrap_or_default().lines().filter(|l| l.contains("upgradable")).count();
        if up > 30 {
            v.push(Finding::new(
                "Updates",
                Severity::Warning,
                format!("{up} packages can be updated"),
                "Run your update manager or `sudo apt full-upgrade`.",
            ));
        }
    }
    let gov = read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");
    if gov == "powersave"
        && !hw.is_laptop
        && !Path::new("/sys/devices/system/cpu/intel_pstate").exists()
        && !Path::new("/sys/devices/system/cpu/amd_pstate").exists()
    {
        v.push(Finding::new(
            "Performance",
            Severity::Advice,
            "CPU governor is 'powersave' on a desktop",
            "Use 'schedutil' or 'performance' via your power profile daemon.",
        ));
    }
    v
}

pub fn actions() -> Vec<Action> {
    use Risk::*;
    vec![
        Action {
            id: "failedunits",
            name: "Reset failed services",
            desc: "systemctl reset-failed - clears the failed state so they can start again.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                let (c, o) = util::run_status("systemctl", &["reset-failed"]);
                if c == 0 {
                    Ok("reset".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "trimnow",
            name: "Trim SSDs now",
            desc: "fstrim -av",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || {
                let (c, o) = util::run_status("fstrim", &["-av"]);
                if c == 0 {
                    Ok(o.lines().last().unwrap_or("done").into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "dnsflush",
            name: "Flush DNS cache",
            desc: "resolvectl flush-caches",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                let (c, o) = util::run_status("resolvectl", &["flush-caches"]);
                if c == 0 {
                    Ok("flushed".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "timesync",
            name: "Enable time sync",
            desc: "timedatectl set-ntp true",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                let (c, o) = util::run_status("timedatectl", &["set-ntp", "true"]);
                if c == 0 {
                    Ok("enabled".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "fontcache",
            name: "Rebuild font cache",
            desc: "fc-cache -f - fixes missing or garbled fonts.",
            risk: Safe,
            slow: true,
            needs_admin: false,
            run: || {
                let (c, o) = util::run_status("fc-cache", &["-f"]);
                if c == 0 {
                    Ok("rebuilt".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "dpkgfix",
            name: "Repair interrupted package installs",
            desc: "dpkg --configure -a && apt-get -f install (Debian/Ubuntu).",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || {
                let _ = util::run_status("dpkg", &["--configure", "-a"]);
                let (c, o) = util::run_status("apt-get", &["-f", "install", "-y"]);
                if c == 0 {
                    Ok("repaired".into())
                } else {
                    Err(o)
                }
            },
        },
    ]
}
