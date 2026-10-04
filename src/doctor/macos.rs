//! macOS doctor: system_profiler, diskutil, security tools.

use super::{Action, Battery, Finding, Hardware, PhysDisk, Severity};
use crate::util::{self, Risk};

fn sp(kind: &str) -> serde_json::Value {
    util::run("system_profiler", &[kind, "-json", "-detailLevel", "mini"])
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn s(v: &serde_json::Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

pub fn hardware() -> Hardware {
    let mut hw = super::base_hardware();
    let hwj = sp("SPHardwareDataType");
    if let Some(h) = hwj.get("SPHardwareDataType").and_then(|a| a.get(0)) {
        hw.manufacturer = "Apple".into();
        hw.model = format!("{} ({})", s(h, "machine_name"), s(h, "machine_model"));
        if hw.cpu.is_empty() || hw.cpu == "Unknown CPU" {
            hw.cpu = s(h, "chip_type");
        }
    }
    hw.is_laptop = hw.model.contains("Book");
    let pow = sp("SPPowerDataType");
    if let Some(items) = pow.get("SPPowerDataType").and_then(|a| a.as_array()) {
        for it in items {
            if let Some(b) = it.get("sppower_battery_health_info") {
                let max = s(b, "sppower_battery_health_maximum_capacity").trim_end_matches('%').parse::<u64>().ok();
                let cycles = b.get("sppower_battery_cycle_count").and_then(|c| c.as_u64());
                hw.battery = Some(Battery { design_mwh: 100, full_mwh: max.unwrap_or(0), cycles });
                if max.is_none() {
                    hw.battery = None;
                }
            }
        }
    }
    let gpu = sp("SPDisplaysDataType");
    if let Some(items) = gpu.get("SPDisplaysDataType").and_then(|a| a.as_array()) {
        hw.gpus = items.iter().map(|g| s(g, "sppci_model")).filter(|n| !n.is_empty()).collect();
        hw.gpu_vendors = hw
            .gpus
            .iter()
            .map(|g| {
                if g.contains("Apple") {
                    "Apple"
                } else if g.contains("AMD") || g.contains("Radeon") {
                    "AMD"
                } else {
                    "Intel"
                }
                .to_string()
            })
            .collect();
    }
    let st = sp("SPNVMeDataType");
    for key in ["SPNVMeDataType", "SPSerialATADataType"] {
        let data = if key == "SPNVMeDataType" { st.clone() } else { sp(key) };
        if let Some(ctrls) = data.get(key).and_then(|a| a.as_array()) {
            for c in ctrls {
                for d in c.get("_items").and_then(|i| i.as_array()).cloned().unwrap_or_default() {
                    hw.disks.push(PhysDisk {
                        model: s(&d, "_name"),
                        media: if key == "SPNVMeDataType" { "NVMe".into() } else { s(&d, "spsata_medium_type") },
                        size: d.get("size_in_bytes").and_then(|x| x.as_u64()).unwrap_or(0),
                        health: s(&d, "smart_status"),
                        temperature: None,
                        wear: None,
                    });
                }
            }
        }
    }
    hw.system_disk_ssd = Some(true);
    hw.firmware = "Apple firmware".into();
    hw
}

pub fn findings(hw: &Hardware) -> Vec<Finding> {
    let mut v = Vec::new();
    let fv = util::run("fdesetup", &["status"]).unwrap_or_default();
    if fv.contains("Off") {
        v.push(Finding::new(
            "Security",
            if hw.is_laptop { Severity::Warning } else { Severity::Advice },
            "FileVault is off",
            "Your disk isn't encrypted. Turn it on in System Settings > Privacy & Security.",
        ));
    } else if fv.contains("On") {
        v.push(Finding::new("Security", Severity::Ok, "FileVault encryption on", ""));
    }
    let sip = util::run("csrutil", &["status"]).unwrap_or_default();
    if sip.contains("disabled") {
        v.push(Finding::new(
            "Security",
            Severity::Warning,
            "System Integrity Protection is disabled",
            "Re-enable it from Recovery (csrutil enable) unless you really need it off.",
        ));
    }
    let gk = util::run("spctl", &["--status"]).unwrap_or_default();
    if gk.contains("disabled") {
        v.push(Finding::new(
            "Security",
            Severity::Warning,
            "Gatekeeper is disabled",
            "Any downloaded app can run without checks. Re-enable: sudo spctl --master-enable",
        ));
    }
    let fw = util::run("/usr/libexec/ApplicationFirewall/socketfilterfw", &["--getglobalstate"]).unwrap_or_default();
    if fw.contains("disabled") {
        v.push(Finding::new(
            "Security",
            Severity::Advice,
            "Firewall is off",
            "Turn it on in System Settings > Network > Firewall, especially on public Wi-Fi.",
        ));
    }
    let tm = util::run("tmutil", &["latestbackup"]).unwrap_or_default();
    if tm.trim().is_empty() {
        v.push(Finding::new(
            "Recovery",
            Severity::Warning,
            "No Time Machine backup found",
            "Set up Time Machine (or another backup) before cleaning aggressively.",
        ));
    }
    // kernel panics in the last 30 days
    let month = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86400);
    let panics = std::fs::read_dir("/Library/Logs/DiagnosticReports")
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name().to_string_lossy().contains(".panic")
                        && e.metadata().and_then(|m| m.modified()).map(|t| t > month).unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    if panics > 0 {
        v.push(Finding::new(
            "Stability",
            Severity::Critical,
            format!("{panics} kernel panic(s) in 30 days"),
            "Often caused by kernel extensions, peripherals or failing hardware. Run Apple Diagnostics (hold D / power button at startup).",
        ));
    }
    let pressure = util::run("memory_pressure", &[]).unwrap_or_default();
    if let Some(p) = pressure
        .lines()
        .find_map(|l| l.strip_prefix("System-wide memory free percentage: "))
        .and_then(|p| p.trim_end_matches('%').trim().parse::<u32>().ok())
    {
        if p < 10 {
            v.push(
                Finding::new(
                    "Memory",
                    Severity::Warning,
                    format!("Memory pressure high ({p}% free)"),
                    "Close heavy apps or check login items.",
                )
                .open("startup"),
            );
        }
    }
    let agents = crate::uninstall::startup::list().iter().filter(|s| s.enabled).count();
    if agents > 15 {
        v.push(
            Finding::new(
                "Startup",
                Severity::Advice,
                format!("{agents} launch agents/daemons enabled"),
                "Background helpers from apps you may no longer use.",
            )
            .open("startup"),
        );
    }
    let broken = crate::uninstall::startup::list().iter().filter(|s| s.broken).count();
    if broken > 0 {
        v.push(
            Finding::new(
                "Startup",
                Severity::Advice,
                format!("{broken} launch agent(s) point to deleted programs"),
                "Leftovers of uninstalled apps - delete them in Uninstall > Startup.",
            )
            .open("startup"),
        );
    }
    let _ = hw;
    v
}

pub fn actions() -> Vec<Action> {
    use Risk::*;
    vec![
        Action {
            id: "dnsflush",
            name: "Flush DNS cache",
            desc: "dscacheutil -flushcache + restart mDNSResponder.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                let _ = util::run_status("dscacheutil", &["-flushcache"]);
                let (c, o) = util::run_status("killall", &["-HUP", "mDNSResponder"]);
                if c == 0 {
                    Ok("flushed".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "spotlight",
            name: "Rebuild Spotlight index",
            desc: "mdutil -E / - fixes missing search results. Re-indexing takes a while.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                let (c, o) = util::run_status("mdutil", &["-E", "/"]);
                if c == 0 {
                    Ok("re-indexing started".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "verify",
            name: "Verify startup disk",
            desc: "diskutil verifyVolume / - read-only check of the file system.",
            risk: Safe,
            slow: true,
            needs_admin: false,
            run: || {
                let (c, o) = util::run_status("diskutil", &["verifyVolume", "/"]);
                if c == 0 {
                    Ok(o.lines().last().unwrap_or("ok").into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "periodic",
            name: "Run maintenance scripts",
            desc: "periodic daily weekly monthly - rotates logs and cleans temp files.",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || {
                let (c, o) = util::run_status("periodic", &["daily", "weekly", "monthly"]);
                if c == 0 {
                    Ok("done".into())
                } else {
                    Err(o)
                }
            },
        },
        Action {
            id: "dock",
            name: "Restart Dock & Finder",
            desc: "Fixes a stuck Dock, Launchpad or Finder.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || {
                let _ = util::run_status("killall", &["Dock"]);
                let _ = util::run_status("killall", &["Finder"]);
                Ok("restarted".into())
            },
        },
        Action {
            id: "launchservices",
            name: "Rebuild 'Open With' database",
            desc: "Fixes duplicate or missing apps in Open With menus.",
            risk: Safe,
            slow: true,
            needs_admin: false,
            run: || {
                let (c, o) = util::run_status(
                    "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
                    &["-kill", "-r", "-domain", "local", "-domain", "system", "-domain", "user"],
                );
                if c == 0 {
                    Ok("rebuilt".into())
                } else {
                    Err(o)
                }
            },
        },
    ]
}
