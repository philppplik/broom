//! Doctor: knows the machine, finds problems, recommends hardware-aware changes and repairs.
//!
//! A scan collects `Hardware` facts and produces `Finding`s. A finding may carry a `Fix`, which is
//! either a tweak from the Tweaks catalog or a repair action from `actions()`.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use crate::util::{self, Risk};

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Battery {
    pub design_mwh: u64,
    pub full_mwh: u64,
    pub cycles: Option<u64>,
}

impl Battery {
    pub fn health_pct(&self) -> Option<f64> {
        (self.design_mwh > 0).then(|| (self.full_mwh as f64 / self.design_mwh as f64 * 100.0).min(100.0))
    }
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct PhysDisk {
    pub model: String,
    /// SSD, HDD, NVMe, Unknown
    pub media: String,
    pub size: u64,
    pub health: String,
    pub temperature: Option<u64>,
    /// percentage of rated endurance used (SSD)
    pub wear: Option<u64>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Hardware {
    pub manufacturer: String,
    pub model: String,
    pub os: String,
    pub arch: String,
    pub cpu: String,
    pub cores: usize,
    pub ram_total: u64,
    pub ram_available: u64,
    /// (rated MHz, configured MHz)
    pub ram_speed: Option<(u64, u64)>,
    pub ram_slots: Option<(u64, u64)>,
    pub gpus: Vec<String>,
    pub gpu_vendors: Vec<String>,
    pub is_laptop: bool,
    pub battery: Option<Battery>,
    pub disks: Vec<PhysDisk>,
    pub system_disk_ssd: Option<bool>,
    pub uptime_secs: u64,
    pub firmware: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Severity {
    Critical,
    Warning,
    Advice,
    Ok,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Critical => "CRITICAL",
            Severity::Warning => "WARNING",
            Severity::Advice => "ADVICE",
            Severity::Ok => "OK",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub enum Fix {
    /// apply this tweak id from the Tweaks catalog
    Tweak(String),
    /// run this repair action id
    Action(String),
    /// switch to a Broom tab (clean / uninstall / startup) - for things the user should review
    Open(String),
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Finding {
    pub area: &'static str,
    pub title: String,
    pub detail: String,
    pub severity: Severity,
    pub fix: Option<Fix>,
}

impl Finding {
    pub fn new(area: &'static str, severity: Severity, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Finding { area, title: title.into(), detail: detail.into(), severity, fix: None }
    }
    pub fn fix(mut self, f: Fix) -> Self {
        self.fix = Some(f);
        self
    }
    pub fn action(self, id: &str) -> Self {
        self.fix(Fix::Action(id.into()))
    }
    pub fn tweak(self, id: &str) -> Self {
        self.fix(Fix::Tweak(id.into()))
    }
    pub fn open(self, tab: &str) -> Self {
        self.fix(Fix::Open(tab.into()))
    }
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Report {
    pub hardware: Hardware,
    pub findings: Vec<Finding>,
    /// 0-100
    pub score: u32,
}

/// A repair or maintenance tool.
pub struct Action {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub risk: Risk,
    pub slow: bool,
    pub needs_admin: bool,
    pub run: fn() -> Result<String, String>,
}

/// Facts every OS can provide without special tools.
pub fn base_hardware() -> Hardware {
    let mem = util::sys::memory();
    Hardware {
        os: util::sys::os_pretty(),
        arch: util::native_arch(),
        cpu: util::sys::cpu_name(),
        cores: util::sys::cpu_cores(),
        ram_total: mem.total,
        ram_available: mem.available,
        uptime_secs: util::sys::uptime_secs(),
        ..Default::default()
    }
}

pub fn hardware() -> Hardware {
    #[cfg(windows)]
    return windows::hardware();
    #[cfg(target_os = "macos")]
    return macos::hardware();
    #[cfg(target_os = "linux")]
    return linux::hardware();
}

/// Full scan. Takes a few seconds (one batched query per OS).
pub fn scan() -> Report {
    let hw = hardware();
    let mut f = common_findings(&hw);
    #[cfg(windows)]
    f.extend(windows::findings(&hw));
    #[cfg(target_os = "macos")]
    f.extend(macos::findings(&hw));
    #[cfg(target_os = "linux")]
    f.extend(linux::findings(&hw));
    f.sort_by_key(|x| x.severity);
    let penalty: u32 = f
        .iter()
        .map(|x| match x.severity {
            Severity::Critical => 15,
            Severity::Warning => 6,
            Severity::Advice => 2,
            Severity::Ok => 0,
        })
        .sum();
    util::log(format!("doctor scan: {} findings, score {}", f.len(), 100u32.saturating_sub(penalty)));
    Report { score: 100u32.saturating_sub(penalty), hardware: hw, findings: f }
}

pub fn actions() -> Vec<Action> {
    #[allow(unused_mut)]
    let mut v: Vec<Action> = Vec::new();
    #[cfg(windows)]
    v.extend(windows::actions());
    #[cfg(target_os = "macos")]
    v.extend(macos::actions());
    #[cfg(target_os = "linux")]
    v.extend(linux::actions());
    v
}

pub fn run_action(id: &str) -> Result<String, String> {
    let Some(a) = actions().into_iter().find(|a| a.id == id) else { return Err(format!("unknown action {id}")) };
    if crate::util::fs::dry() {
        return Ok("dry run".into());
    }
    util::log(format!("doctor action {}", a.name));
    (a.run)()
}

/// Checks that read the same on every OS.
fn common_findings(hw: &Hardware) -> Vec<Finding> {
    let mut v = Vec::new();
    // disk space on every fixed volume
    for d in util::sys::disks().into_iter().filter(|d| !d.removable && d.total > 8 << 30) {
        let pct = d.free as f64 / d.total as f64 * 100.0;
        let label = format!("{} {} free of {} ({pct:.0}%)", d.mount.display(), util::fmt_size(d.free), util::fmt_size(d.total));
        if pct < 5.0 {
            v.push(
                Finding::new(
                    "Storage",
                    Severity::Critical,
                    "Disk almost full",
                    format!("{label}. Updates and apps may fail. Run Clean and check Uninstall."),
                )
                .open("clean"),
            );
        } else if pct < 12.0 {
            v.push(
                Finding::new(
                    "Storage",
                    Severity::Warning,
                    "Disk space is getting low",
                    format!("{label}. SSDs also slow down when nearly full."),
                )
                .open("clean"),
            );
        } else {
            v.push(Finding::new("Storage", Severity::Ok, "Enough free space", label));
        }
    }
    // memory
    let gb = hw.ram_total as f64 / (1u64 << 30) as f64;
    let used_pct = 100.0 - hw.ram_available as f64 / hw.ram_total.max(1) as f64 * 100.0;
    if gb < 7.5 {
        v.push(
            Finding::new(
                "Memory",
                Severity::Advice,
                format!("Only {gb:.0} GB RAM"),
                "Keep startup apps and browser tabs lean; consider an upgrade if the PC feels slow.",
            )
            .open("startup"),
        );
    }
    if used_pct > 90.0 {
        v.push(
            Finding::new(
                "Memory",
                Severity::Warning,
                format!("Memory {used_pct:.0}% in use right now"),
                "Something is using almost all RAM. Check running apps and startup items.",
            )
            .open("startup"),
        );
    }
    if let Some((rated, configured)) = hw.ram_speed {
        if !hw.is_laptop && rated > 0 && configured > 0 && configured + 200 < rated {
            v.push(Finding::new(
                "Memory",
                Severity::Advice,
                format!("RAM runs at {configured} MT/s, rated for {rated}"),
                "The memory profile (XMP/EXPO) is probably off. Enable it in the BIOS/UEFI for free performance.",
            ));
        }
    }
    // uptime
    let days = hw.uptime_secs / 86_400;
    if days >= 14 {
        v.push(Finding::new(
            "Stability",
            Severity::Advice,
            format!("No restart for {days} days"),
            "A restart finishes updates and clears leaked memory.",
        ));
    }
    // battery
    if let Some(b) = &hw.battery {
        if let Some(h) = b.health_pct() {
            let cycles = b.cycles.map(|c| format!(", {c} cycles")).unwrap_or_default();
            if h < 70.0 {
                v.push(Finding::new(
                    "Battery",
                    Severity::Warning,
                    format!("Battery worn: {h:.0}% of original capacity{cycles}"),
                    "Expect noticeably shorter runtime. Consider a replacement battery.",
                ));
            } else {
                v.push(Finding::new("Battery", Severity::Ok, format!("Battery health {h:.0}%{cycles}"), "Within normal wear."));
            }
        }
    }
    // physical disks
    for d in &hw.disks {
        let h = d.health.to_lowercase();
        if !h.is_empty() && !["healthy", "ok", "passed", "verified", ""].contains(&h.as_str()) {
            v.push(Finding::new(
                "Storage",
                Severity::Critical,
                format!("Drive problem: {}", d.model),
                format!("Health reported as '{}'. Back up your data now.", d.health),
            ));
        }
        if let Some(w) = d.wear.filter(|w| *w >= 80) {
            v.push(Finding::new(
                "Storage",
                Severity::Warning,
                format!("SSD {} has used {w}% of its rated endurance", d.model),
                "Plan a replacement and keep backups current.",
            ));
        }
        if let Some(t) = d.temperature.filter(|t| *t >= 65) {
            v.push(Finding::new(
                "Storage",
                Severity::Warning,
                format!("Drive {} is hot: {t} °C", d.model),
                "Check airflow; NVMe drives throttle above ~70 °C.",
            ));
        }
    }
    v
}
