//! Windows doctor: facts via one batched PowerShell/CIM query + native registry checks.

use super::{Action, Battery, Finding, Fix, Hardware, PhysDisk, Severity};
use crate::util::{self, reg, Risk};
use serde::Deserialize;

/// PowerShell sometimes emits `{}` for "nothing": treat anything unexpected as None.
mod lenient {
    use serde::{Deserialize, Deserializer};
    pub fn u64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        Ok(serde_json::Value::deserialize(d)?.as_u64())
    }
    pub fn u32<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
        Ok(serde_json::Value::deserialize(d)?.as_u64().map(|v| v as u32))
    }
    pub fn string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        Ok(serde_json::Value::deserialize(d)?.as_str().map(String::from))
    }
    pub fn boolean<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
        Ok(serde_json::Value::deserialize(d)?.as_bool())
    }
    pub fn strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        Ok(match serde_json::Value::deserialize(d)? {
            serde_json::Value::Array(a) => a.into_iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            serde_json::Value::String(s) => vec![s],
            _ => vec![],
        })
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
struct Gpu {
    name: String,
    #[serde(deserialize_with = "lenient::string")]
    driver_version: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    driver_date: Option<String>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
struct Disk {
    model: String,
    media: String,
    bus: String,
    health: String,
    size: u64,
    #[serde(deserialize_with = "lenient::u64")]
    temp: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    wear: Option<u64>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
struct Facts {
    #[serde(deserialize_with = "lenient::string")]
    manufacturer: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    model: Option<String>,
    #[serde(deserialize_with = "lenient::u32")]
    pc_system_type: Option<u32>,
    chassis: Vec<u32>,
    #[serde(deserialize_with = "lenient::boolean")]
    auto_pagefile: Option<bool>,
    #[serde(deserialize_with = "lenient::u64")]
    ram_rated: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    ram_configured: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    ram_sticks: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    ram_slots: Option<u64>,
    gpus: Vec<Gpu>,
    #[serde(deserialize_with = "lenient::u64")]
    bat_design: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    bat_full: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    bat_cycles: Option<u64>,
    disks: Vec<Disk>,
    #[serde(deserialize_with = "lenient::string")]
    sys_media: Option<String>,
    #[serde(deserialize_with = "lenient::boolean")]
    secure_boot: Option<bool>,
    #[serde(deserialize_with = "lenient::boolean")]
    tpm_present: Option<bool>,
    #[serde(deserialize_with = "lenient::u32")]
    vbs: Option<u32>,
    #[serde(deserialize_with = "lenient::boolean")]
    hvci: Option<bool>,
    #[serde(deserialize_with = "lenient::boolean")]
    def_realtime: Option<bool>,
    #[serde(deserialize_with = "lenient::u64")]
    def_sig_age: Option<u64>,
    #[serde(deserialize_with = "lenient::strings")]
    av: Vec<String>,
    #[serde(deserialize_with = "lenient::strings")]
    fw_off: Vec<String>,
    #[serde(deserialize_with = "lenient::string")]
    bit_locker: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    last_hotfix: Option<String>,
    #[serde(deserialize_with = "lenient::strings")]
    problem_devices: Vec<String>,
    #[serde(deserialize_with = "lenient::u64")]
    unexpected_shutdowns: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    bugchecks: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    disk_errors: Option<u64>,
    #[serde(deserialize_with = "lenient::u64")]
    app_crashes: Option<u64>,
    #[serde(deserialize_with = "lenient::strings")]
    top_crash: Vec<String>,
    #[serde(deserialize_with = "lenient::boolean")]
    activated: Option<bool>,
    #[serde(deserialize_with = "lenient::u64")]
    restore_points: Option<u64>,
    #[serde(deserialize_with = "lenient::string")]
    firmware: Option<String>,
}

const FACTS_PS: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$r=[ordered]@{}
$cs=Get-CimInstance Win32_ComputerSystem
$r.Manufacturer=$cs.Manufacturer; $r.Model=$cs.Model; $r.PCSystemType=[int]$cs.PCSystemType; $r.AutoPagefile=[bool]$cs.AutomaticManagedPagefile
$r.Chassis=@((Get-CimInstance Win32_SystemEnclosure).ChassisTypes | ForEach-Object {[int]$_})
$mem=@(Get-CimInstance Win32_PhysicalMemory)
$r.RamRated=[uint64](($mem|Measure-Object Speed -Maximum).Maximum); $r.RamConfigured=[uint64](($mem|Measure-Object ConfiguredClockSpeed -Minimum).Minimum); $r.RamSticks=[uint64]$mem.Count
$r.RamSlots=[uint64]((Get-CimInstance Win32_PhysicalMemoryArray|Measure-Object MemoryDevices -Sum).Sum)
$r.Gpus=@(Get-CimInstance Win32_VideoController | ForEach-Object { [pscustomobject]@{Name=$_.Name;DriverVersion=$_.DriverVersion;DriverDate=$(if($_.DriverDate){$_.DriverDate.ToString('yyyy-MM-dd')}else{$null})} })
$bs=Get-CimInstance -Namespace root/wmi -ClassName BatteryStaticData | Select-Object -First 1
if($bs){ $r.BatDesign=[uint64]$bs.DesignedCapacity; $r.BatFull=[uint64](Get-CimInstance -Namespace root/wmi -ClassName BatteryFullChargedCapacity|Select-Object -First 1).FullChargedCapacity; $c=(Get-CimInstance -Namespace root/wmi -ClassName BatteryCycleCount|Select-Object -First 1).CycleCount; if($c){$r.BatCycles=[uint64]$c} }
$r.Disks=@(Get-PhysicalDisk | ForEach-Object { $rc=$_|Get-StorageReliabilityCounter; [pscustomobject]@{Model=$_.FriendlyName;Media=[string]$_.MediaType;Bus=[string]$_.BusType;Health=[string]$_.HealthStatus;Size=[uint64]$_.Size;Temp=$(if($rc.Temperature){[uint64]$rc.Temperature}else{$null});Wear=$(if($rc.Wear -ne $null){[uint64]$rc.Wear}else{$null})} })
$sd=(Get-Partition -DriveLetter $env:SystemDrive[0] | Get-Disk).Number
$pd=Get-PhysicalDisk | Where-Object DeviceId -eq "$sd"; $r.SysMedia="$($pd.MediaType)|$($pd.BusType)"
try{ $r.SecureBoot=[bool](Confirm-SecureBootUEFI -ErrorAction Stop) }catch{}
$r.Firmware=$(if($env:firmware_type){$env:firmware_type}else{(Get-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Control -Name PEFirmwareType).PEFirmwareType})
$t=Get-Tpm; if($t){$r.TpmPresent=[bool]$t.TpmPresent}
$dg=Get-CimInstance -Namespace root/Microsoft/Windows/DeviceGuard -ClassName Win32_DeviceGuard; if($dg){$r.Vbs=[int]$dg.VirtualizationBasedSecurityStatus; $r.Hvci=(@($dg.SecurityServicesRunning) -contains 2)}
$mp=Get-MpComputerStatus; if($mp){$r.DefRealtime=[bool]$mp.RealTimeProtectionEnabled; $r.DefSigAge=[uint64]$mp.AntivirusSignatureAge}
$r.Av=@(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | ForEach-Object {$_.displayName})
$r.FwOff=@(Get-NetFirewallProfile | Where-Object Enabled -eq $false | ForEach-Object {[string]$_.Name})
$bl=Get-BitLockerVolume -MountPoint $env:SystemDrive; if($bl){$r.BitLocker=[string]$bl.ProtectionStatus}
$h=Get-HotFix | Where-Object InstalledOn | Sort-Object InstalledOn -Descending | Select-Object -First 1; if($h){$r.LastHotfix=$h.InstalledOn.ToString('yyyy-MM-dd')}
$r.ProblemDevices=@(Get-CimInstance Win32_PnPEntity | Where-Object { $_.ConfigManagerErrorCode -and $_.ConfigManagerErrorCode -ne 22 -and $_.ConfigManagerErrorCode -ne 45 } | ForEach-Object { "$($_.Name) (code $($_.ConfigManagerErrorCode))" })
$since=(Get-Date).AddDays(-30)
$r.UnexpectedShutdowns=[uint64]@(Get-WinEvent -FilterHashtable @{LogName='System';ProviderName='Microsoft-Windows-Kernel-Power';Id=41;StartTime=$since} -MaxEvents 300).Count
$r.Bugchecks=[uint64]@(Get-WinEvent -FilterHashtable @{LogName='System';Id=1001;ProviderName='Microsoft-Windows-WER-SystemErrorReporting';StartTime=$since} -MaxEvents 300).Count
$r.DiskErrors=[uint64]@(Get-WinEvent -FilterHashtable @{LogName='System';ProviderName='disk','stornvme','storahci','Ntfs';Level=1,2;StartTime=$since} -MaxEvents 500).Count
$ac=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=(Get-Date).AddDays(-7)} -MaxEvents 500)
$r.AppCrashes=[uint64]$ac.Count; $r.TopCrash=@($ac | ForEach-Object {[string]$_.Properties[0].Value} | Group-Object | Sort-Object Count -Descending | Select-Object -First 3 | ForEach-Object {"$($_.Name) x$($_.Count)"})
$r.Activated=[bool](Get-CimInstance SoftwareLicensingProduct -Filter "PartialProductKey IS NOT NULL AND ApplicationID='55c92734-d682-4d71-983e-d6ec3f16059f'" | Where-Object LicenseStatus -eq 1)
$rp=@(Get-ComputerRestorePoint); if($?){$r.RestorePoints=[uint64]$rp.Count}
[Console]::Out.Write(($r | ConvertTo-Json -Depth 4 -Compress))
"#;

fn facts() -> Facts {
    let out = util::ps(FACTS_PS).unwrap_or_default();
    let json = out.trim().trim_start_matches(|c| c != '{');
    serde_json::from_str(json).unwrap_or_else(|e| {
        util::log(format!("doctor facts parse error: {e}"));
        Facts::default()
    })
}

static FACTS: std::sync::Mutex<Option<std::sync::Arc<Facts>>> = std::sync::Mutex::new(None);

fn cached_facts(refresh: bool) -> std::sync::Arc<Facts> {
    let mut g = FACTS.lock().unwrap();
    if refresh || g.is_none() {
        *g = Some(std::sync::Arc::new(facts()));
    }
    g.as_ref().unwrap().clone()
}

fn gpu_vendor(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    [
        ("nvidia", "NVIDIA"),
        ("geforce", "NVIDIA"),
        ("radeon", "AMD"),
        ("amd", "AMD"),
        ("intel", "Intel"),
        ("arc", "Intel"),
        ("adreno", "Qualcomm"),
        ("qualcomm", "Qualcomm"),
    ]
    .iter()
    .find(|(k, _)| n.contains(k))
    .map(|(_, v)| *v)
}

pub fn hardware() -> Hardware {
    let f = cached_facts(true);
    let mut hw = super::base_hardware();
    hw.manufacturer = f.manufacturer.clone().unwrap_or_default();
    hw.model = f.model.clone().unwrap_or_default();
    // PCSystemType 2 = mobile; chassis 8-10,14,30-32 = portable/laptop/notebook/sub-notebook/tablet/convertible/detachable
    hw.is_laptop =
        f.pc_system_type == Some(2) || f.chassis.iter().any(|c| [8, 9, 10, 14, 30, 31, 32].contains(c)) || f.bat_design.is_some();
    hw.ram_speed = match (f.ram_rated, f.ram_configured) {
        (Some(r), Some(c)) if r > 0 => Some((r, c)),
        _ => None,
    };
    hw.ram_slots = match (f.ram_sticks, f.ram_slots) {
        (Some(s), Some(t)) if t > 0 => Some((s, t)),
        _ => None,
    };
    hw.gpus = f.gpus.iter().map(|g| g.name.clone()).filter(|n| !n.to_lowercase().contains("basic display") && !n.is_empty()).collect();
    hw.gpu_vendors = hw.gpus.iter().filter_map(|g| gpu_vendor(g)).map(String::from).collect();
    hw.gpu_vendors.dedup();
    hw.battery =
        f.bat_design.filter(|d| *d > 0).map(|d| Battery { design_mwh: d, full_mwh: f.bat_full.unwrap_or(0), cycles: f.bat_cycles });
    hw.disks = f
        .disks
        .iter()
        .map(|d| PhysDisk {
            model: d.model.clone(),
            media: if d.bus.eq_ignore_ascii_case("NVMe") { "NVMe".into() } else { d.media.clone() },
            size: d.size,
            health: d.health.clone(),
            temperature: d.temp,
            wear: d.wear,
        })
        .collect();
    hw.system_disk_ssd = f.sys_media.as_deref().map(|m| m.contains("SSD") || m.contains("NVMe") || m.contains("|SD")).or(None);
    hw.firmware = match f.firmware.as_deref() {
        Some("2") | Some("UEFI") => "UEFI".into(),
        Some("1") | Some("Legacy") => "Legacy BIOS".into(),
        _ => String::new(),
    };
    hw
}

fn days_since(date: &str) -> Option<i64> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some((chrono::Local::now().date_naive() - d).num_days())
}

pub fn findings(hw: &Hardware) -> Vec<Finding> {
    let f = cached_facts(false);
    let mut v = Vec::new();
    let admin = util::is_admin();
    if !admin {
        v.push(Finding::new(
            "Doctor",
            Severity::Advice,
            "Limited scan (not administrator)",
            "Disk health, BitLocker, restore points and the component store can only be checked with admin rights.",
        ));
    }

    // ---------------- stability
    let restart_keys = [
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending", None),
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired", None),
        (r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager", Some("PendingFileRenameOperations")),
    ];
    if restart_keys.iter().any(|(k, val)| match val {
        None => reg::key_exists(k),
        Some(n) => reg::get(k, n).is_some(),
    }) {
        v.push(Finding::new(
            "Stability",
            Severity::Warning,
            "Restart pending",
            "Updates or installers are waiting for a restart to finish. Some repairs fail until then.",
        ));
    }
    let crashes = f.bugchecks.unwrap_or(0);
    let unexpected = f.unexpected_shutdowns.unwrap_or(0);
    if crashes > 0 {
        v.push(
            Finding::new(
                "Stability",
                Severity::Critical,
                format!("{crashes} blue screen(s) in the last 30 days"),
                "Common causes: drivers, RAM, overheating. Run the memory test and check problem devices.",
            )
            .action("memtest"),
        );
    }
    if unexpected > 2 {
        v.push(
            Finding::new(
                "Stability",
                Severity::Warning,
                format!("{unexpected} unexpected shutdowns in 30 days"),
                "Power loss, freezes or forced power-offs. Check PSU/battery, temperatures and drivers.",
            )
            .action("reliability"),
        );
    }
    if f.app_crashes.unwrap_or(0) >= 5 {
        v.push(
            Finding::new(
                "Stability",
                Severity::Advice,
                format!("{} app crashes in the last 7 days", f.app_crashes.unwrap_or(0)),
                format!("Most frequent: {}. Updating or reinstalling these apps usually helps.", f.top_crash.join(", ")),
            )
            .open("uninstall"),
        );
    }
    if !f.problem_devices.is_empty() {
        v.push(
            Finding::new(
                "Drivers",
                Severity::Warning,
                format!("{} device(s) with driver problems", f.problem_devices.len()),
                f.problem_devices.iter().take(4).cloned().collect::<Vec<_>>().join("; "),
            )
            .action("devmgmt"),
        );
    }
    if f.disk_errors.unwrap_or(0) > 0 {
        v.push(
            Finding::new(
                "Storage",
                Severity::Critical,
                format!("{} disk/file-system errors in 30 days", f.disk_errors.unwrap_or(0)),
                "Back up important data, then run a disk check.",
            )
            .action("chkdsk"),
        );
    }
    if admin {
        let (_, out) = util::run_status("dism.exe", &["/Online", "/English", "/Cleanup-Image", "/CheckHealth"]);
        if out.contains("repairable") {
            v.push(
                Finding::new(
                    "System files",
                    Severity::Critical,
                    "Windows component store is damaged",
                    "Repair with DISM RestoreHealth, then run SFC.",
                )
                .action("dism"),
            );
        } else if out.contains("No component store corruption") {
            v.push(Finding::new(
                "System files",
                Severity::Ok,
                "Windows component store is healthy",
                "DISM CheckHealth found no corruption.",
            ));
        }
    }

    // ---------------- updates & activation
    if let Some(d) = f.last_hotfix.as_deref().and_then(days_since) {
        if d > 45 {
            v.push(
                Finding::new(
                    "Updates",
                    Severity::Warning,
                    format!("Last Windows update {d} days ago"),
                    "Security fixes are missing. Open Windows Update; if it is stuck, reset its components.",
                )
                .action("wureset"),
            );
        } else {
            v.push(Finding::new("Updates", Severity::Ok, format!("Updates current (last {d} days ago)"), ""));
        }
    }
    if f.activated == Some(false) {
        v.push(Finding::new(
            "Windows",
            Severity::Warning,
            "Windows is not activated",
            "Personalisation is limited and you'll see watermarks. Settings > System > Activation.",
        ));
    }

    // ---------------- security
    let av_other: Vec<&String> = f.av.iter().filter(|a| !a.to_lowercase().contains("defender")).collect();
    if f.def_realtime == Some(false) && av_other.is_empty() {
        v.push(Finding::new(
            "Security",
            Severity::Critical,
            "No real-time virus protection",
            "Microsoft Defender real-time protection is off and no other antivirus is registered.",
        ));
    } else if let Some(age) = f.def_sig_age.filter(|a| *a > 7 && av_other.is_empty()) {
        v.push(Finding::new(
            "Security",
            Severity::Warning,
            format!("Virus definitions are {age} days old"),
            "Run Windows Update or open Windows Security to refresh them.",
        ));
    } else {
        v.push(Finding::new(
            "Security",
            Severity::Ok,
            "Antivirus active",
            if av_other.is_empty() {
                "Microsoft Defender".to_string()
            } else {
                av_other.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            },
        ));
    }
    if !f.fw_off.is_empty() {
        v.push(Finding::new(
            "Security",
            Severity::Warning,
            format!("Firewall off for: {}", f.fw_off.join(", ")),
            "Turn it back on in Windows Security unless another firewall replaces it.",
        ));
    }
    if reg::get(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableLUA").and_then(|x| x.as_u64()) == Some(0) {
        v.push(Finding::new(
            "Security",
            Severity::Critical,
            "User Account Control is disabled",
            "Every program runs with full admin rights. Re-enable UAC (restart required).",
        ));
    }
    let winlogon = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon";
    if reg::get_str(winlogon, "DefaultPassword").filter(|p| !p.is_empty()).is_some() {
        v.push(Finding::new(
            "Security",
            Severity::Critical,
            "Your password is stored in plain text",
            "Auto-logon keeps the password unencrypted in the registry. Use Sysinternals Autologon instead or disable auto-logon.",
        ));
    }
    let smb1 =
        reg::get(r"HKLM\SYSTEM\CurrentControlSet\Services\mrxsmb10", "Start").and_then(|x| x.as_u64()).map(|s| s != 4).unwrap_or(false);
    if smb1 {
        v.push(
            Finding::new(
                "Security",
                Severity::Warning,
                "SMBv1 is enabled",
                "A 30-year-old file-sharing protocol used by WannaCry. Disable it unless an old NAS needs it.",
            )
            .action("smb1off"),
        );
    }
    if reg::get(r"HKLM\SYSTEM\CurrentControlSet\Control\Terminal Server", "fDenyTSConnections").and_then(|x| x.as_u64()) == Some(0) {
        v.push(Finding::new(
            "Security",
            Severity::Advice,
            "Remote Desktop is enabled",
            "Fine if you use it; otherwise turn it off to reduce attack surface.",
        ));
    }
    if f.secure_boot == Some(false) {
        v.push(Finding::new(
            "Security",
            Severity::Advice,
            "Secure Boot is off",
            "Enable it in UEFI settings to block boot-level malware (needed for some anti-cheat games).",
        ));
    }
    if admin && f.tpm_present == Some(false) {
        v.push(Finding::new(
            "Security",
            Severity::Advice,
            "No TPM detected",
            "Enable fTPM / PTT in UEFI. Required for Windows 11 features and BitLocker.",
        ));
    }
    if hw.is_laptop && matches!(f.bit_locker.as_deref(), Some("Off")) {
        v.push(Finding::new(
            "Security",
            Severity::Advice,
            "Laptop drive is not encrypted",
            "If the laptop is lost, anyone can read your files. Turn on BitLocker / Device encryption.",
        ));
    }
    match f.restore_points {
        Some(0) => v.push(
            Finding::new(
                "Recovery",
                Severity::Warning,
                "No restore points",
                "System Protection seems off, so there is no way back after a bad driver or update.",
            )
            .action("restorepoint"),
        ),
        Some(n) => v.push(Finding::new("Recovery", Severity::Ok, format!("{n} restore point(s) available"), "")),
        None => {}
    }

    // ---------------- performance, hardware-aware
    let hags = reg::get(r"HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode").and_then(|x| x.as_u64());
    let dgpu = hw.gpu_vendors.iter().any(|g| g == "NVIDIA" || g == "AMD");
    if dgpu && hags != Some(2) {
        v.push(
            Finding::new(
                "Performance",
                Severity::Advice,
                "GPU scheduling is off",
                format!("Your {} supports hardware-accelerated GPU scheduling: lower input latency.", hw.gpus.join(", ")),
            )
            .tweak("hags-on"),
        );
    }
    if dgpu && reg::get(r"HKCU\System\GameConfigStore", "GameDVR_Enabled").and_then(|x| x.as_u64()) != Some(0) {
        v.push(
            Finding::new("Performance", Severity::Advice, "Background game recording is on", "Game DVR records constantly and costs FPS.")
                .tweak("game-dvr-off"),
        );
    }
    if f.hvci == Some(true) && dgpu && !hw.is_laptop {
        v.push(Finding::new("Performance", Severity::Advice, "Memory Integrity (HVCI) is on",
            "It protects the kernel and costs a few % in some games. Only turn off if you accept the security trade-off (Windows Security > Device security)."));
    }
    for g in &f.gpus {
        if let Some(age) = g.driver_date.as_deref().and_then(days_since).filter(|d| *d > 400) {
            if gpu_vendor(&g.name).is_some_and(|v| v != "Qualcomm") || dgpu {
                v.push(Finding::new(
                    "Drivers",
                    Severity::Advice,
                    format!("GPU driver is {} months old", age / 30),
                    format!(
                        "{} driver {}. Get the latest from the vendor for fixes and performance.",
                        g.name,
                        g.driver_version.clone().unwrap_or_default()
                    ),
                ));
            }
        }
    }
    let scheme = util::run("powercfg.exe", &["/getactivescheme"]).unwrap_or_default().to_lowercase();
    let high = scheme.contains("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c") || scheme.contains("e9a42b02-d5df-448d-aa00-03f14749eb61");
    let saver = scheme.contains("a1841308-3541-4fab-bc81-f71556f20b4a");
    if hw.is_laptop && high {
        v.push(
            Finding::new(
                "Power",
                Severity::Advice,
                "Laptop on a high-performance power plan",
                "Drains the battery fast. Balanced lets Windows boost when needed.",
            )
            .action("powerbalanced"),
        );
    }
    if !hw.is_laptop && saver {
        v.push(
            Finding::new(
                "Power",
                Severity::Advice,
                "Desktop on Power saver",
                "Your CPU is held back. Switch to Balanced or High performance.",
            )
            .action("powerbalanced"),
        );
    }
    if hw.system_disk_ssd == Some(false) {
        v.push(Finding::new(
            "Storage",
            Severity::Advice,
            "Windows runs from a hard disk (HDD)",
            "The single biggest speed-up for this PC would be an SSD. Keep SysMain enabled until then.",
        ));
    }
    if hw.system_disk_ssd == Some(true) {
        let trim = util::run("fsutil.exe", &["behavior", "query", "DisableDeleteNotify"]).unwrap_or_default();
        if trim.contains("NTFS DisableDeleteNotify = 1") {
            v.push(
                Finding::new(
                    "Storage",
                    Severity::Warning,
                    "TRIM is disabled for your SSD",
                    "Without TRIM the SSD slows down and wears faster.",
                )
                .action("trim"),
            );
        }
    }
    if f.auto_pagefile == Some(false) && hw.ram_total < (16u64 << 30) {
        v.push(
            Finding::new(
                "Memory",
                Severity::Advice,
                "Page file is managed manually",
                "With less than 16 GB RAM a too-small page file causes 'out of memory' crashes.",
            )
            .action("pagefile"),
        );
    }
    if let Some((sticks, slots)) = hw.ram_slots {
        if sticks == 1 && slots >= 2 && !hw.is_laptop {
            v.push(Finding::new(
                "Memory",
                Severity::Advice,
                "Single RAM stick (single channel)",
                "A second identical stick doubles memory bandwidth - noticeable with integrated graphics and in games.",
            ));
        }
    }
    let enabled = crate::uninstall::startup::list().iter().filter(|s| s.enabled).count();
    if enabled > 10 {
        v.push(
            Finding::new(
                "Startup",
                Severity::Advice,
                format!("{enabled} apps start with Windows"),
                "Each one slows boot and uses RAM. Disable what you don't need right away.",
            )
            .open("startup"),
        );
    }
    if reg::get(r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled").and_then(|x| x.as_u64()) == Some(1)
        && hw.uptime_secs > 7 * 86_400
    {
        v.push(Finding::new(
            "Stability",
            Severity::Advice,
            "Fast Startup keeps Windows from fully restarting",
            "'Shut down' only hibernates the kernel, which is why uptime keeps growing. Use Restart regularly, or turn Fast Startup off.",
        ));
    }
    if hw.arch == "ARM64" {
        v.push(Finding::new(
            "Performance",
            Severity::Ok,
            "ARM64 PC detected",
            "Prefer native ARM64 versions of apps where available; x64 apps run emulated.",
        ));
    }
    // DNS speed
    let cur = crate::tweaks::dns::current();
    if let Some(c) = cur.first().filter(|c| !c.starts_with("192.168.") && !c.starts_with("10.") && !c.starts_with("fe80")) {
        if let (Some(mine), Some(cf)) = (crate::tweaks::dns::measure(c), crate::tweaks::dns::measure("1.1.1.1")) {
            if mine > 2.0 * cf && mine > 40.0 {
                v.push(
                    Finding::new(
                        "Network",
                        Severity::Advice,
                        format!("Your DNS ({c}) answers in {mine:.0} ms"),
                        format!("Cloudflare answers in {cf:.0} ms here. Faster DNS makes every new website load quicker."),
                    )
                    .open("dns"),
                );
            }
        }
    }
    v
}

fn run(prog: &str, args: &[&str]) -> Result<String, String> {
    let (c, out) = util::run_status(prog, args);
    let out = out.replace('\0', "");
    let last: Vec<&str> = out.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let summary = last.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join(" ");
    if c == 0 {
        Ok(summary)
    } else {
        Err(format!("exit {c}: {summary}"))
    }
}

fn ps(script: &str) -> Result<String, String> {
    util::ps(&format!("$ErrorActionPreference='Stop'; try {{ {script}; 'done' }} catch {{ 'ERR: ' + $_.Exception.Message }}"))
        .map_err(|e| e.to_string())
        .and_then(
            |o| if o.contains("ERR:") { Err(o.trim().to_string()) } else { Ok(o.trim().lines().last().unwrap_or("done").to_string()) },
        )
}

fn open(target: &str) -> Result<String, String> {
    std::process::Command::new("cmd.exe").args(["/c", "start", "", target]).spawn().map(|_| "opened".into()).map_err(|e| e.to_string())
}

pub fn actions() -> Vec<Action> {
    use Risk::*;
    vec![
        Action {
            id: "sfc",
            name: "Repair system files (SFC)",
            desc: "sfc /scannow - checks and repairs protected Windows files. 5-15 min.",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || run("sfc.exe", &["/scannow"]),
        },
        Action {
            id: "dism",
            name: "Repair Windows image (DISM RestoreHealth)",
            desc: "Downloads clean copies of damaged components from Windows Update. 10-30 min. Run SFC afterwards.",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || run("dism.exe", &["/Online", "/English", "/Cleanup-Image", "/RestoreHealth"]),
        },
        Action {
            id: "chkdsk",
            name: "Check system drive (online scan)",
            desc: "chkdsk /scan - finds file-system errors without a reboot. Fixes are scheduled if needed.",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || {
                run(
                    "chkdsk.exe",
                    &[&util::env_path("SystemDrive").map(|p| p.display().to_string()).unwrap_or("C:".into()).to_string(), "/scan"],
                )
            },
        },
        Action {
            id: "netreset",
            name: "Reset network stack",
            desc: "Winsock + TCP/IP reset, DNS flush, renew IP. Fixes 'connected but no internet'. Restart required.",
            risk: Moderate,
            slow: false,
            needs_admin: true,
            run: || {
                let _ = run("netsh.exe", &["winsock", "reset"]);
                let _ = run("netsh.exe", &["int", "ip", "reset"]);
                let _ = run("ipconfig.exe", &["/flushdns"]);
                let _ = run("ipconfig.exe", &["/renew"]);
                Ok("reset - restart Windows to finish".into())
            },
        },
        Action {
            id: "wureset",
            name: "Reset Windows Update",
            desc: "Stops update services, renames SoftwareDistribution and catroot2, restarts services. Fixes stuck updates.",
            risk: Moderate,
            slow: false,
            needs_admin: true,
            run: || {
                ps("$s='wuauserv','bits','cryptsvc','msiserver'; Stop-Service $s -Force -ErrorAction SilentlyContinue; $t=Get-Date -Format yyyyMMddHHmmss; Rename-Item \"$env:windir\\SoftwareDistribution\" \"SoftwareDistribution.broom$t\" -ErrorAction SilentlyContinue; Rename-Item \"$env:windir\\System32\\catroot2\" \"catroot2.broom$t\" -ErrorAction SilentlyContinue; Start-Service $s -ErrorAction SilentlyContinue")
            },
        },
        Action {
            id: "iconcache",
            name: "Rebuild icon & thumbnail cache",
            desc: "Fixes wrong or blank icons. Explorer restarts.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || {
                ps("Stop-Process -Name explorer -Force; Start-Sleep -Milliseconds 600; Remove-Item \"$env:LOCALAPPDATA\\Microsoft\\Windows\\Explorer\\iconcache_*.db\",\"$env:LOCALAPPDATA\\Microsoft\\Windows\\Explorer\\thumbcache_*.db\",\"$env:LOCALAPPDATA\\IconCache.db\" -Force -ErrorAction SilentlyContinue; Start-Sleep 2; if(-not (Get-Process explorer -ErrorAction SilentlyContinue)){Start-Process explorer}")
            },
        },
        Action {
            id: "explorer",
            name: "Restart Explorer",
            desc: "Fixes a frozen taskbar, Start menu or desktop.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || {
                ps("Stop-Process -Name explorer -Force; Start-Sleep 2; if(-not (Get-Process explorer -ErrorAction SilentlyContinue)){Start-Process explorer}")
            },
        },
        Action {
            id: "audio",
            name: "Restart audio services",
            desc: "Fixes 'no sound' without a reboot.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || ps("Restart-Service AudioEndpointBuilder -Force; Restart-Service Audiosrv -Force"),
        },
        Action {
            id: "search",
            name: "Rebuild Windows Search index",
            desc: "Fixes missing search results. Re-indexing runs in the background for a while.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                ps("Stop-Service WSearch -Force; Set-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows Search' -Name SetupCompletedSuccessfully -Value 0; Start-Service WSearch")
            },
        },
        Action {
            id: "timesync",
            name: "Sync clock",
            desc: "Forces a time sync - fixes certificate errors caused by a wrong clock.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                let _ = run("net.exe", &["start", "w32time"]);
                run("w32tm.exe", &["/resync", "/force"])
            },
        },
        Action {
            id: "storereset",
            name: "Reset Microsoft Store cache",
            desc: "wsreset - fixes Store downloads that hang.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || run("wsreset.exe", &["-i"]),
        },
        Action {
            id: "spooler",
            name: "Clear stuck print jobs",
            desc: "Stops the print spooler, deletes queued jobs, restarts it.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                ps("Stop-Service Spooler -Force; Remove-Item \"$env:windir\\System32\\spool\\PRINTERS\\*\" -Force -ErrorAction SilentlyContinue; Start-Service Spooler")
            },
        },
        Action {
            id: "restorepoint",
            name: "Turn on System Protection + create restore point",
            desc: "Enables System Restore on the system drive and creates a restore point now.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                ps("Enable-ComputerRestore -Drive \"$env:SystemDrive\\\"; Set-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore' -Name SystemRestorePointCreationFrequency -Value 0 -Type DWord; Checkpoint-Computer -Description 'Broom' -RestorePointType MODIFY_SETTINGS")
            },
        },
        Action {
            id: "battery",
            name: "Battery report",
            desc: "Creates Windows' detailed battery history report and opens it.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || {
                let p = util::sub_dir("Reports").join("battery-report.html");
                run("powercfg.exe", &["/batteryreport", "/output", &p.to_string_lossy()])?;
                open(&p.to_string_lossy())
            },
        },
        Action {
            id: "powerbalanced",
            name: "Power plan: Balanced",
            desc: "Windows' recommended plan: boosts when needed, saves energy otherwise.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || run("powercfg.exe", &["/setactive", "381b4222-f694-41f0-9685-ff5bb260df2e"]),
        },
        Action {
            id: "powerhigh",
            name: "Power plan: High performance",
            desc: "Keeps the CPU at higher clocks. Desktops only.",
            risk: Moderate,
            slow: false,
            needs_admin: false,
            run: || run("powercfg.exe", &["/setactive", "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"]),
        },
        Action {
            id: "trim",
            name: "Enable SSD TRIM",
            desc: "fsutil behavior set DisableDeleteNotify 0",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || run("fsutil.exe", &["behavior", "set", "DisableDeleteNotify", "0"]),
        },
        Action {
            id: "optimize",
            name: "Optimize drives now (TRIM / defrag)",
            desc: "Runs Windows' drive optimizer on all volumes: TRIM for SSDs, defrag for HDDs.",
            risk: Safe,
            slow: true,
            needs_admin: true,
            run: || run("defrag.exe", &["/C", "/O", "/H"]),
        },
        Action {
            id: "pagefile",
            name: "Let Windows manage the page file",
            desc: "Restores automatic page-file sizing.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || {
                ps("$c=Get-CimInstance Win32_ComputerSystem; Set-CimInstance -InputObject $c -Property @{AutomaticManagedPagefile=$true}")
            },
        },
        Action {
            id: "smb1off",
            name: "Disable SMBv1",
            desc: "Turns off the obsolete SMB1 protocol (client and server).",
            risk: Moderate,
            slow: false,
            needs_admin: true,
            run: || {
                ps("Disable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol -NoRestart | Out-Null; Set-SmbServerConfiguration -EnableSMB1Protocol $false -Force")
            },
        },
        Action {
            id: "gpurestart",
            name: "Restart graphics driver",
            desc: "Restarts the display adapter(s). Screen goes black for a moment. Fixes glitches without a reboot.",
            risk: Moderate,
            slow: false,
            needs_admin: true,
            run: || {
                ps("Get-PnpDevice -Class Display -Status OK | ForEach-Object { pnputil /restart-device \"$($_.InstanceId)\" | Out-Null }")
            },
        },
        Action {
            id: "memtest",
            name: "Memory test (on next restart)",
            desc: "Opens Windows Memory Diagnostic to test the RAM at the next restart.",
            risk: Safe,
            slow: false,
            needs_admin: true,
            run: || open("mdsched.exe"),
        },
        Action {
            id: "devmgmt",
            name: "Open Device Manager",
            desc: "See and update devices with driver problems.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || open("devmgmt.msc"),
        },
        Action {
            id: "reliability",
            name: "Open Reliability Monitor",
            desc: "Windows' timeline of crashes, failed updates and errors.",
            risk: Safe,
            slow: false,
            needs_admin: false,
            run: || run("perfmon.exe", &["/rel"]).or(Ok("opened".into())),
        },
    ]
}

#[allow(dead_code)]
fn _fix_kinds(_: Fix) {}
