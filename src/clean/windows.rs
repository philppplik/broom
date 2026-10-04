//! Windows cleaning items (port of Broom 1.x plus extras).

use super::{Item, Outcome};
use crate::util::fs::{self as bfs, Walk};
use crate::util::reg::{self, Val};
use crate::util::{self, Risk};
use std::path::{Path, PathBuf};

fn windir() -> PathBuf {
    util::env_path("windir").unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}
fn sysdrive() -> PathBuf {
    bfs::system_root()
}
fn program_data() -> PathBuf {
    util::env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}
fn user_paths(rel: &[&str]) -> Vec<PathBuf> {
    util::profiles().iter().flat_map(|p| rel.iter().map(move |r| p.join(r))).collect()
}
fn clear_many(paths: impl IntoIterator<Item = PathBuf>, w: &Walk) -> Outcome {
    let mut o = Outcome::default();
    for p in paths {
        o += bfs::clear(p, w);
    }
    o
}

/// True only if `path` is absolute, on a present drive, readable, and really gone,
/// also after trying System32/SysWOW64/SysArm32 and Program Files/(x86)/(Arm) variants.
pub fn path_missing(path: &str) -> bool {
    let p = util::expand_env(path.trim().trim_matches('"').trim());
    if p.contains('%') || p.len() < 4 || !p.as_bytes()[0].is_ascii_alphabetic() || &p[1..3] != ":\\" {
        return false;
    }
    let lower = p.to_lowercase();
    if lower.contains("\\windowsapps\\") {
        return false;
    }
    if !Path::new(&p[..3]).is_dir() || Path::new(&p).exists() {
        return false;
    }
    let mut alts: Vec<String> = Vec::new();
    for s in ["System32", "SysWOW64", "SysNative", "SysArm32"] {
        for f in ["\\System32\\", "\\SysWOW64\\", "\\SysNative\\", "\\SysArm32\\"] {
            if let Some(i) = lower.find(&f.to_lowercase()) {
                alts.push(format!("{}\\{s}\\{}", &p[..i], &p[i + f.len()..]));
            }
        }
    }
    for s in ["Program Files", "Program Files (x86)", "Program Files (Arm)"] {
        for f in ["\\program files (x86)\\", "\\program files (arm)\\", "\\program files\\"] {
            if let Some(i) = lower.find(f) {
                alts.push(format!("{}\\{s}\\{}", &p[..i], &p[i + f.len()..]));
                break;
            }
        }
    }
    if alts.iter().any(|a| Path::new(a).exists()) {
        return false;
    }
    // "Doesn't exist" may also mean "access denied": trust it only if the parent can be listed.
    let mut parent = Path::new(&p).parent();
    while let Some(d) = parent {
        if d.is_dir() {
            return std::fs::read_dir(d).is_ok();
        }
        parent = d.parent();
    }
    false
}

/// `"C:\x\y.exe" /arg` or `C:\x y\z.exe -arg` -> the executable path.
pub fn exe_from_command(cmd: &str) -> Option<String> {
    let c = util::expand_env(cmd.trim());
    if let Some(rest) = c.strip_prefix('"') {
        return rest.find('"').map(|e| rest[..e].to_string()).filter(|s| !s.is_empty());
    }
    let lower = c.to_lowercase();
    if !(c.len() > 3 && &c[1..3] == ":\\") {
        return None;
    }
    for ext in [".exe", ".com", ".bat", ".cmd", ".msc", ".cpl", ".lnk"] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(ext) {
            let end = from + i + ext.len();
            if end == c.len() || c[end..].starts_with(' ') || c[end..].starts_with(',') {
                return Some(c[..end].to_string());
            }
            from = end;
        }
    }
    None
}

// ---------------------------------------------------------------- system junk

fn usertemp() -> Outcome {
    clear_many(user_paths(&["AppData/Local/Temp"]), &Walk::all())
}
fn wintemp() -> Outcome {
    clear_many([windir().join("Temp")], &Walk::all())
}
fn wer() -> Outcome {
    let pd = program_data().join("Microsoft/Windows/WER");
    let mut v = vec![pd.join("ReportArchive"), pd.join("ReportQueue"), pd.join("Temp")];
    v.extend(user_paths(&["AppData/Local/Microsoft/Windows/WER"]));
    clear_many(v, &Walk::all())
}
fn dumps() -> Outcome {
    let w = windir();
    let mut v = vec![w.join("MEMORY.DMP"), w.join("Minidump"), w.join("LiveKernelReports")];
    v.extend(user_paths(&["AppData/Local/CrashDumps"]));
    clear_many(v, &Walk::all())
}
fn logs() -> Outcome {
    let w = windir();
    let mut o = clear_many(
        [
            "Logs/CBS",
            "Logs/DISM",
            "Logs/MoSetup",
            "Logs/WindowsUpdate",
            "Logs/waasmedic",
            "Logs/SIH",
            "Logs/NetSetup",
            "System32/LogFiles/setupcln",
            "SoftwareDistribution/DataStore/Logs",
        ]
        .iter()
        .map(|r| w.join(r)),
        &Walk::all(),
    );
    o += clear_many([w.join("Panther"), w.join("debug")], &Walk::pattern(&["*.log", "*.etl", "*.txt"])).into_freed();
    o += bfs::clear(&w, &Walk::pattern(&["*.log"]).flat());
    o
}
fn recycle() -> Outcome {
    let mut o = Outcome::default();
    for d in util::sys::disks() {
        if d.removable {
            continue;
        }
        let rb = d.mount.join("$Recycle.Bin");
        let Ok(rd) = std::fs::read_dir(&rb) else { continue };
        for e in rd.flatten() {
            o += bfs::clear(e.path(), &Walk::all().except(&["desktop.ini"]));
        }
    }
    o
}
fn thumbs() -> Outcome {
    let dirs = user_paths(&["AppData/Local/Microsoft/Windows/Explorer"]);
    let w = Walk::pattern(&["thumbcache_*.db", "iconcache_*.db"]).flat();
    if bfs::dry() {
        let mut o = clear_many(dirs, &w);
        o += clear_many(user_paths(&["AppData/Local/IconCache.db"]), &Walk::all()).into_freed();
        return o;
    }
    let _ = util::run_status("taskkill.exe", &["/f", "/im", "explorer.exe"]);
    std::thread::sleep(std::time::Duration::from_millis(600));
    let mut o = clear_many(dirs, &w);
    o += clear_many(user_paths(&["AppData/Local/IconCache.db"]), &Walk::all()).into_freed();
    std::thread::sleep(std::time::Duration::from_secs(2));
    if super::running(&["explorer"]).is_empty() {
        let _ = std::process::Command::new(windir().join("explorer.exe")).spawn();
    }
    o
}
fn netcache() -> Outcome {
    if bfs::dry() {
        return Outcome::default().note("would flush");
    }
    for (p, a) in [("ipconfig.exe", vec!["/flushdns"]), ("arp.exe", vec!["-d", "*"]), ("nbtstat.exe", vec!["-R"])] {
        let _ = util::run_status(p, &a);
    }
    Outcome::default().note("flushed")
}
fn prefetch() -> Outcome {
    bfs::clear(windir().join("Prefetch"), &Walk::pattern(&["*.pf"]).flat()).into()
}
fn fontcache() -> Outcome {
    let dir = windir().join("ServiceProfiles/LocalService/AppData/Local/FontCache");
    if bfs::dry() {
        return bfs::clear(&dir, &Walk::pattern(&["*.dat"]).flat()).into();
    }
    with_services(&["FontCache"], || bfs::clear(&dir, &Walk::pattern(&["*.dat"]).flat()).into())
}

// ---------------------------------------------------------------- updates & installers

fn with_services(names: &[&str], f: impl FnOnce() -> Outcome) -> Outcome {
    let mut stopped = Vec::new();
    if !bfs::dry() {
        for n in names {
            let (_, out) = util::run_status("sc.exe", &["query", n]);
            if out.contains("RUNNING") {
                let _ = util::run_status("net.exe", &["stop", n, "/y"]);
                stopped.push(*n);
            }
        }
    }
    let o = f();
    for n in stopped {
        let _ = util::run_status("net.exe", &["start", n]);
    }
    o
}

fn wucache() -> Outcome {
    let d = windir().join("SoftwareDistribution/Download");
    with_services(&["wuauserv", "bits"], || bfs::clear_all(&d).into())
}
fn dopt() -> Outcome {
    let dir = windir().join("ServiceProfiles/NetworkService/AppData/Local/Microsoft/Windows/DeliveryOptimization/Cache");
    if bfs::dry() {
        return Outcome { bytes: bfs::size_of(&dir), ..Default::default() };
    }
    let _ = util::ps("Delete-DeliveryOptimizationCache -Force -ErrorAction SilentlyContinue");
    Outcome::default()
}

/// Run Windows' own Disk Cleanup. "DownloadsFolder" is never enabled.
pub fn cleanmgr(only: &[&str]) {
    if bfs::dry() {
        return;
    }
    let root = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches";
    let flag = "StateFlags0777";
    let mut set = Vec::new();
    for h in reg::subkeys(root) {
        let path = format!(r"{root}\{h}");
        if h.eq_ignore_ascii_case("DownloadsFolder") {
            let _ = reg::delete_value(&path, flag);
            continue;
        }
        if !only.is_empty() && !only.iter().any(|o| o.eq_ignore_ascii_case(&h)) {
            continue;
        }
        if reg::set(&path, flag, &Val::Dword(2)).is_ok() {
            set.push(path);
        }
    }
    if !set.is_empty() {
        let _ = util::run_status("cleanmgr.exe", &["/sagerun:777"]);
    }
    for p in set {
        let _ = reg::delete_value(&p, flag);
    }
}

fn upgrade() -> Outcome {
    let root = sysdrive();
    let dirs: Vec<PathBuf> =
        ["Windows.old", "$Windows.~BT", "$Windows.~WS", "$WinREAgent", "$GetCurrent", "$SysReset", "ESD/Windows", "ESD/Download"]
            .iter()
            .map(|d| root.join(d))
            .filter(|d| d.is_dir())
            .collect();
    if dirs.is_empty() {
        return Outcome::default().note("nothing found");
    }
    if bfs::dry() {
        return Outcome { bytes: dirs.iter().map(|d| bfs::size_of_fast(d)).sum(), ..Default::default() };
    }
    cleanmgr(&[
        "Previous Installations",
        "Temporary Setup Files",
        "Windows Upgrade Log Files",
        "Windows ESD installation files",
        "Setup Log Files",
    ]);
    for d in dirs.iter().filter(|d| d.is_dir()) {
        let s = d.to_string_lossy().to_string();
        let _ = util::run_status("icacls.exe", &[&s, "/setowner", "*S-1-5-32-544", "/T", "/C", "/L", "/Q"]);
        let _ = util::run_status("icacls.exe", &[&s, "/grant", "*S-1-5-32-544:F", "/T", "/C", "/L", "/Q"]);
        let _ = util::run_status("cmd.exe", &["/c", "rd", "/s", "/q", &s]);
    }
    Outcome::default()
}
fn cleanmgr_all() -> Outcome {
    if bfs::dry() {
        return Outcome::default().note("measured on a real run");
    }
    cleanmgr(&[]);
    Outcome::default()
}
fn winsxs() -> Outcome {
    if bfs::dry() {
        return Outcome::default().note("measured on a real run");
    }
    let (code, _) = util::run_status("dism.exe", &["/Online", "/English", "/Cleanup-Image", "/StartComponentCleanup", "/ResetBase"]);
    if code != 0 {
        return Outcome::default().note(format!("DISM code {code} (restart pending?)"));
    }
    Outcome::default()
}
fn leftovers() -> Outcome {
    let root = sysdrive();
    let mut o = Outcome::default();
    for d in ["AMD", "NVIDIA", "MSOCache"] {
        o += bfs::remove_dir_all_safe(&root.join(d));
    }
    let pf86 = util::env_path("ProgramFiles(x86)").unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let pf = util::env_path("ProgramFiles").unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    o += clear_many(
        [
            root.join("Intel/Logs"),
            program_data().join("NVIDIA Corporation/Downloader"),
            pf86.join("Microsoft/EdgeUpdate/Download"),
            pf86.join("Google/Update/Download"),
            pf.join("Google/Update/Download"),
            windir().join("Installer/$PatchCache$"),
        ],
        &Walk::all(),
    )
    .into_freed();
    o += clear_many(user_paths(&["AppData/Local/Downloaded Installations", "AppData/Local/Microsoft/EdgeUpdate/Download"]), &Walk::all())
        .into_freed();
    // Squirrel apps (Discord, Slack, Teams classic...) keep every old full package; keep only the newest
    for app in user_paths(&["AppData/Local/*"]).iter().flat_map(|p| bfs::expand(p)) {
        if !app.join("Update.exe").is_file() {
            continue;
        }
        let mut pk: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        bfs::walk_files(&app.join("packages"), &Walk::pattern(&["*.nupkg"]).flat(), &mut |p, md| {
            pk.push((md.modified().unwrap_or(std::time::UNIX_EPOCH), p.to_path_buf()))
        });
        pk.sort();
        pk.pop();
        for (_, p) in pk {
            o += bfs::remove_file(&p);
        }
    }
    o
}

#[derive(serde::Deserialize)]
struct LocalPkg {
    #[serde(rename = "P")]
    p: Option<String>,
}

fn msi_orphans() -> Outcome {
    let mut used = std::collections::HashSet::new();
    let ud = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\UserData";
    for sid in reg::subkeys(ud) {
        for prod in reg::subkeys(&format!(r"{ud}\{sid}\Products")) {
            if let Some(lp) = reg::get_str(&format!(r"{ud}\{sid}\Products\{prod}\InstallProperties"), "LocalPackage") {
                used.insert(file_name_lower(&lp));
            }
        }
        for pat in reg::subkeys(&format!(r"{ud}\{sid}\Patches")) {
            if let Some(lp) = reg::get_str(&format!(r"{ud}\{sid}\Patches\{pat}"), "LocalPackage") {
                used.insert(file_name_lower(&lp));
            }
        }
    }
    // Second, independent source: the Windows Installer API.
    let api: Vec<LocalPkg> = util::ps_json(
        "$wi=New-Object -ComObject WindowsInstaller.Installer;$t=$wi.GetType();\
         foreach($p in $t.InvokeMember('ProductsEx','GetProperty',$null,$wi,@('','s-1-1-0',7))){try{[pscustomobject]@{P=$p.GetType().InvokeMember('InstallProperty','GetProperty',$null,$p,@('LocalPackage'))}}catch{}};\
         try{foreach($x in $t.InvokeMember('PatchesEx','GetProperty',$null,$wi,@('','s-1-1-0',7,15))){try{[pscustomobject]@{P=$x.GetType().InvokeMember('PatchProperty','GetProperty',$null,$x,@('LocalPackage'))}}catch{}}}catch{}",
    );
    if api.is_empty() || used.len() < 3 {
        return Outcome::default().note("could not verify - skipped for safety");
    }
    for a in api.into_iter().filter_map(|a| a.p) {
        used.insert(file_name_lower(&a));
    }
    let mut o = Outcome::default();
    let mut n = 0;
    let mut victims = Vec::new();
    bfs::walk_files(&windir().join("Installer"), &Walk::pattern(&["*.msi", "*.msp"]).flat(), &mut |p, _| {
        if !used.contains(&file_name_lower(&p.to_string_lossy())) {
            victims.push(p.to_path_buf());
        }
    });
    for v in victims {
        util::log(format!("orphaned installer package {}", v.display()));
        n += 1;
        o += bfs::remove_file(&v);
    }
    o.note(format!("{n} packages"))
}
fn file_name_lower(p: &str) -> String {
    Path::new(p).file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Drv {
    driver: String,
    original_file_name: Option<String>,
    provider_name: Option<String>,
    class_name: Option<String>,
    version: Option<String>,
}

fn old_drivers() -> Outcome {
    let list: Vec<Drv> = util::ps_json(
        "Get-WindowsDriver -Online | Where-Object { $_.Driver -like 'oem*.inf' } | Select-Object Driver,OriginalFileName,ProviderName,ClassName,@{n='Version';e={[string]$_.Version}}",
    );
    let mut groups: std::collections::BTreeMap<String, Vec<Drv>> = Default::default();
    for d in list {
        let Some(orig) = d.original_file_name.clone() else { continue };
        let key = format!(
            "{}|{}|{}",
            file_name_lower(&orig),
            d.provider_name.clone().unwrap_or_default(),
            d.class_name.clone().unwrap_or_default()
        );
        groups.entry(key).or_default().push(d);
    }
    let ver = |s: &Option<String>| -> Vec<u64> { s.clone().unwrap_or_default().split('.').map(|x| x.parse().unwrap_or(0)).collect() };
    let mut o = Outcome::default();
    let mut n = 0;
    for (_, mut g) in groups {
        if g.len() < 2 {
            continue;
        }
        g.sort_by(|a, b| ver(&b.version).cmp(&ver(&a.version)));
        for old in g.iter().skip(1) {
            let folder = Path::new(old.original_file_name.as_deref().unwrap_or("")).parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let size = bfs::size_of(&folder);
            if bfs::dry() || util::run_status("pnputil.exe", &["/delete-driver", &old.driver]).0 == 0 {
                n += 1;
                o.bytes += size;
                util::log(format!("old driver {} {:?} {:?}", old.driver, old.provider_name, old.version));
            }
        }
    }
    o.note(format!("{n} packages"))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Shadow {
    id: String,
    volume_name: String,
    install_date: Option<String>,
}

fn shadows() -> Outcome {
    let list: Vec<Shadow> =
        util::ps_json("Get-CimInstance Win32_ShadowCopy | Select-Object ID,VolumeName,@{n='InstallDate';e={$_.InstallDate.ToString('o')}}");
    let mut by_vol: std::collections::BTreeMap<String, Vec<Shadow>> = Default::default();
    for s in list {
        by_vol.entry(s.volume_name.clone()).or_default().push(s);
    }
    let mut old = Vec::new();
    for (_, mut v) in by_vol {
        v.sort_by(|a, b| b.install_date.cmp(&a.install_date));
        old.extend(v.into_iter().skip(1));
    }
    let n = old.len();
    if !bfs::dry() {
        for s in &old {
            let _ =
                util::ps(&format!("Get-CimInstance Win32_ShadowCopy | Where-Object ID -eq {} | Remove-CimInstance", util::ps_quote(&s.id)));
        }
    }
    Outcome::default().note(format!("{n} old snapshots"))
}
fn hibernate() -> Outcome {
    let hf = sysdrive().join("hiberfil.sys");
    let Ok(md) = std::fs::metadata(&hf) else { return Outcome::default().note("already off") };
    if bfs::dry() {
        return Outcome { bytes: md.len(), ..Default::default() };
    }
    let _ = util::run_status("powercfg.exe", &["/hibernate", "off"]);
    Outcome::default()
}
fn reserved() -> Outcome {
    if bfs::dry() {
        return Outcome::default().note("up to ~7 GB after restart");
    }
    let (c, _) = util::run_status("dism.exe", &["/Online", "/English", "/Set-ReservedStorageState", "/State:Disabled"]);
    Outcome::default().note(if c == 0 { "disabled (freed after restart)" } else { "not possible now (update pending?)" })
}
fn eventlogs() -> Outcome {
    let logs: Vec<String> =
        util::run("wevtutil.exe", &["el"]).unwrap_or_default().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if bfs::dry() {
        let mut est = 0;
        bfs::walk_files(&windir().join("System32/winevt/Logs"), &Walk::pattern(&["*.evtx"]), &mut |_, md| {
            est += md.len().saturating_sub(69_632)
        });
        return Outcome { bytes: est, files: 0, note: Some(format!("{} logs", logs.len())) };
    }
    for l in &logs {
        let _ = util::run_status("wevtutil.exe", &["cl", l]);
    }
    Outcome::default().note(format!("{} logs cleared", logs.len()))
}

// ---------------------------------------------------------------- apps

fn gpucache() -> Outcome {
    let mut o = clear_many(
        user_paths(&[
            "AppData/Local/D3DSCache",
            "AppData/Local/NVIDIA/DXCache",
            "AppData/Local/NVIDIA/GLCache",
            "AppData/Local/NVIDIA Corporation/NV_Cache",
            "AppData/LocalLow/NVIDIA/PerDriverVersion/DXCache",
            "AppData/Local/AMD/DxCache",
            "AppData/Local/AMD/DxcCache",
            "AppData/Local/AMD/VkCache",
            "AppData/Local/AMD/GLCache",
            "AppData/Local/AMD/OglCache",
            "AppData/Local/Intel/ShaderCache",
            "AppData/LocalLow/Intel/ShaderCache",
            "AppData/Local/Qualcomm/ShaderCache",
        ]),
        &Walk::all(),
    );
    o += bfs::clear_all(program_data().join("NVIDIA Corporation/NV_Cache"));
    o
}
fn uwp() -> Outcome {
    let mut o = clear_many(
        user_paths(&["AppData/Local/Packages/*/AC/INetCache", "AppData/Local/Packages/*/AC/Temp", "AppData/Local/Packages/*/TempState"]),
        &Walk::all(),
    );
    o += clear_many(user_paths(&["AppData/Local/Microsoft/Windows/INetCache"]), &Walk::all()).into_freed();
    o
}
fn shortcuts() -> Outcome {
    let mut roots = vec![program_data().join("Microsoft/Windows/Start Menu"), PathBuf::from(util::expand_env("%PUBLIC%")).join("Desktop")];
    roots.extend(user_paths(&["AppData/Roaming/Microsoft/Windows/Start Menu", "Desktop"]));
    let mut lnks = Vec::new();
    for r in &roots {
        bfs::walk_files(r, &Walk::pattern(&["*.lnk"]), &mut |p, _| lnks.push(p.to_path_buf()));
    }
    if lnks.is_empty() {
        return Outcome::default().note("0 shortcuts");
    }
    // Resolve all targets in one PowerShell call (WScript.Shell).
    let list = lnks.iter().map(|p| util::ps_quote(&p.to_string_lossy())).collect::<Vec<_>>().join(",");
    #[derive(serde::Deserialize)]
    struct T {
        #[serde(rename = "P")]
        p: String,
        #[serde(rename = "T")]
        t: Option<String>,
    }
    let targets: Vec<T> = util::ps_json(&format!(
        "$s=New-Object -ComObject WScript.Shell; foreach($f in @({list})){{ [pscustomobject]@{{P=$f;T=$s.CreateShortcut($f).TargetPath}} }}"
    ));
    let win_installer = windir().join("Installer").to_string_lossy().to_lowercase();
    let mut o = Outcome::default();
    let mut n = 0;
    for t in targets {
        let Some(target) = t.t.filter(|s| !s.is_empty()) else { continue };
        if target.to_lowercase().starts_with(&win_installer) || !path_missing(&target) {
            continue;
        }
        util::log(format!("broken shortcut {} -> {target}", t.p));
        n += 1;
        o += bfs::remove_file(Path::new(&t.p));
    }
    o.note(format!("{n} shortcuts"))
}
fn empty_dirs() -> Outcome {
    let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramData"].iter().filter_map(|v| util::env_path(v)).collect();
    roots.push(PathBuf::from(r"C:\Program Files (Arm)"));
    roots.extend(user_paths(&["AppData/Local", "AppData/Roaming", "AppData/LocalLow", "AppData/Local/Programs"]));
    const SKIP: &[&str] = &[
        "temp",
        "packages",
        "microsoft",
        "programs",
        "windowsapps",
        "modifiablewindowsapps",
        "ssh",
        "windows defender",
        "windows defender advanced threat protection",
        "windows nt",
        "common files",
        "internet explorer",
        "reference assemblies",
        "windows mail",
        "windows media player",
        "windows photo viewer",
        "windows sidebar",
        "windowspowershell",
        "windows security",
        "uninstall information",
        "microsoft update health tools",
        "packagemanagement",
        "package cache",
        "comms",
        "connecteddevicesplatform",
        "desktop",
        "documents",
        "start menu",
        "templates",
        "application data",
        "history",
        "temporary internet files",
        "virtualstore",
        "publishers",
        "peerdistrepub",
        "placeholdertilelogofolder",
        "regid.1991-06.com.microsoft",
        "usoprivate",
        "usoshared",
        "softwaredistribution",
        "dotnet",
        "msbuild",
        "broom",
    ];
    let mut n = 0;
    for r in roots {
        let Ok(rd) = std::fs::read_dir(&r) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !md.is_dir() || bfs::is_link(&md) || SKIP.contains(&name.as_str()) || !bfs::tree_is_empty(&p) {
                continue;
            }
            util::log(format!("empty folder {}", p.display()));
            if bfs::dry() || std::fs::remove_dir_all(&p).is_ok() {
                n += 1;
            }
        }
    }
    Outcome::default().note(format!("{n} folders"))
}
fn privacy() -> Outcome {
    let mut o = clear_many(
        user_paths(&[
            "AppData/Roaming/Microsoft/Windows/Recent/AutomaticDestinations",
            "AppData/Roaming/Microsoft/Windows/Recent/CustomDestinations",
        ]),
        &Walk::all(),
    );
    o += clear_many(user_paths(&["AppData/Roaming/Microsoft/Windows/Recent"]), &Walk::pattern(&["*.lnk"]).flat()).into_freed();
    let mut n = 0;
    for root in reg::all_user_roots() {
        for r in [
            "RecentDocs",
            "RunMRU",
            "TypedPaths",
            "WordWheelQuery",
            r"ComDlg32\OpenSavePidlMRU",
            r"ComDlg32\LastVisitedPidlMRU",
            r"ComDlg32\LastVisitedPidlMRULegacy",
        ] {
            let k = format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\Explorer\{r}");
            if reg::key_exists(&k) && reg::delete_key(&k).is_ok() {
                n += 1;
            }
        }
    }
    o.note(format!("{n} history keys"))
}

// ---------------------------------------------------------------- registry

fn hives(rel: &str) -> Vec<String> {
    let mut v = vec![format!(r"HKLM\SOFTWARE\{rel}"), format!(r"HKLM\SOFTWARE\WOW6432Node\{rel}")];
    v.extend(reg::all_user_roots().into_iter().map(|u| format!(r"{u}\Software\{rel}")));
    v
}

fn reg_uninstall() -> Outcome {
    let mut n = 0;
    for root in hives(r"Microsoft\Windows\CurrentVersion\Uninstall") {
        for sub in reg::subkeys(&root) {
            let k = format!(r"{root}\{sub}");
            let Some(u) = reg::get_str(&k, "UninstallString") else { continue };
            let is1 = |name: &str| reg::get(&k, name).and_then(|v| v.as_u64()) == Some(1);
            if is1("WindowsInstaller") || is1("SystemComponent") || u.to_lowercase().contains("msiexec") {
                continue;
            }
            let Some(exe) = exe_from_command(&u) else { continue };
            if !path_missing(&exe) {
                continue;
            }
            if let Some(loc) = reg::get_str(&k, "InstallLocation").filter(|s| !s.trim().is_empty()) {
                if !path_missing(&loc) {
                    continue;
                }
            }
            util::log(format!("REG key {k}  uninstaller missing: {exe}"));
            if reg::delete_key(&k).is_ok() {
                n += 1;
            }
        }
    }
    Outcome::default().note(format!("{n} entries"))
}
fn reg_apppaths() -> Outcome {
    let mut n = 0;
    for root in hives(r"Microsoft\Windows\CurrentVersion\App Paths") {
        for sub in reg::subkeys(&root) {
            let k = format!(r"{root}\{sub}");
            let Some(d) = reg::get_str(&k, "").filter(|s| !s.is_empty()) else { continue };
            let exe = exe_from_command(&d).unwrap_or(d);
            if path_missing(&exe) {
                util::log(format!("REG key {k}  target missing: {exe}"));
                if reg::delete_key(&k).is_ok() {
                    n += 1;
                }
            }
        }
    }
    Outcome::default().note(format!("{n} entries"))
}
fn reg_run() -> Outcome {
    let mut n = 0;
    let mut roots = vec!["HKLM".to_string()];
    roots.extend(reg::all_user_roots());
    for h in roots {
        for (rel, approved) in [
            (r"Software\Microsoft\Windows\CurrentVersion\Run", "Run"),
            (r"Software\Microsoft\Windows\CurrentVersion\RunOnce", "Run"),
            (r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "Run32"),
            (r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce", "Run32"),
        ] {
            let k = format!(r"{h}\{rel}");
            for (name, v) in reg::values(&k) {
                let Some(cmd) = v.as_string() else { continue };
                let Some(exe) = exe_from_command(&cmd) else { continue };
                if !path_missing(&exe) {
                    continue;
                }
                util::log(format!("REG value {k} :: {name}  missing: {exe}"));
                if reg::delete_value(&k, &name).is_ok() {
                    n += 1;
                }
                let sa = format!(r"{h}\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{approved}");
                if reg::value_names(&sa).iter().any(|x| x == &name) {
                    let _ = reg::delete_value(&sa, &name);
                }
            }
        }
    }
    Outcome::default().note(format!("{n} entries"))
}
fn reg_mui() -> Outcome {
    let mut n = 0;
    for sid in reg::user_sids() {
        let k = format!(r"HKU\{sid}_Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache");
        for name in reg::value_names(&k) {
            let mut p = name.trim_start_matches('@').to_string();
            for suf in [".FriendlyAppName", ".ApplicationCompany"] {
                if let Some(s) = p.strip_suffix(suf) {
                    p = s.to_string();
                }
            }
            if let Some(i) = p.rfind(",-") {
                p.truncate(i);
            }
            if path_missing(&p) && reg::delete_value(&k, &name).is_ok() {
                n += 1;
            }
        }
    }
    Outcome::default().note(format!("{n} entries"))
}
fn reg_shared() -> Outcome {
    let mut n = 0;
    for k in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\SharedDLLs",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\SharedDLLs",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\Folders",
    ] {
        for name in reg::value_names(k) {
            if path_missing(&name) && reg::delete_value(k, &name).is_ok() {
                n += 1;
            }
        }
    }
    Outcome::default().note(format!("{n} entries"))
}
fn reg_com() -> Outcome {
    let mut n = 0;
    let mut roots = vec![r"HKLM\SOFTWARE\Classes\CLSID".to_string(), r"HKLM\SOFTWARE\Classes\WOW6432Node\CLSID".to_string()];
    roots.extend(reg::user_sids().into_iter().map(|s| format!(r"HKU\{s}_Classes\CLSID")));
    for root in roots {
        for clsid in reg::subkeys(&root) {
            let k = format!(r"{root}\{clsid}");
            let servers: Vec<String> = ["InprocServer32", "LocalServer32", "InprocHandler32"]
                .iter()
                .filter_map(|s| reg::get(&format!(r"{k}\{s}"), "").and_then(|v| v.as_string()))
                .collect();
            if servers.is_empty() {
                continue;
            }
            let orphan = servers.iter().all(|s| path_missing(&exe_from_command(s).unwrap_or_else(|| s.clone())));
            if orphan {
                util::log(format!("REG key {k}  server missing: {}", servers.join(" | ")));
                if reg::delete_key(&k).is_ok() {
                    n += 1;
                }
            }
        }
    }
    Outcome::default().note(format!("{n} entries"))
}

trait IntoFreed {
    fn into_freed(self) -> bfs::Freed;
}
impl IntoFreed for Outcome {
    fn into_freed(self) -> bfs::Freed {
        bfs::Freed { bytes: self.bytes, files: self.files }
    }
}

macro_rules! item {
    ($id:literal, $cat:literal, $name:literal, $tier:literal, $risk:ident, $desc:literal, $run:ident $(, $flag:ident)*) => {{
        #[allow(unused_mut)]
        let mut it = Item { id: $id, category: $cat, name: $name, tier: $tier, risk: Risk::$risk, desc: $desc, procs: &[], delta: false, slow: false, run: $run };
        $( it.$flag = true; )*
        it
    }};
}

pub fn items() -> Vec<Item> {
    const S: &str = "System junk";
    let _ = S;
    vec![
        item!(
            "usertemp",
            "System junk",
            "User temp files",
            1,
            Safe,
            "Everything in AppData\\Local\\Temp of every user profile. Files in use are skipped.",
            usertemp
        ),
        item!(
            "wintemp",
            "System junk",
            "Windows temp files",
            1,
            Safe,
            "C:\\Windows\\Temp - leftovers from installers, updates and services.",
            wintemp
        ),
        item!("wer", "System junk", "Error reports (WER)", 1, Safe, "Windows Error Reporting archives and queues.", wer),
        item!(
            "dumps",
            "System junk",
            "Crash & memory dumps",
            1,
            Safe,
            "MEMORY.DMP, minidumps, live kernel reports and app crash dumps. Only needed to debug crashes.",
            dumps
        ),
        item!(
            "logs",
            "System junk",
            "Windows log files",
            1,
            Safe,
            "CBS, DISM, setup, upgrade and update logs. Active logs are skipped.",
            logs
        ),
        item!(
            "recycle",
            "System junk",
            "Recycle Bin (all drives, all users)",
            1,
            Moderate,
            "Empties the Recycle Bin on every fixed drive. Deleted files can no longer be restored.",
            recycle
        ),
        item!(
            "netcache",
            "System junk",
            "DNS, ARP & NetBIOS caches",
            1,
            Safe,
            "Flushes stale name-resolution caches. Frees no disk space.",
            netcache
        ),
        item!(
            "thumbs",
            "System junk",
            "Thumbnail & icon cache",
            2,
            Moderate,
            "Rebuilt automatically. Explorer restarts for a second.",
            thumbs
        ),
        item!("fontcache", "System junk", "Font cache", 3, Moderate, "Rebuilt on next boot. Fixes garbled fonts.", fontcache),
        item!(
            "prefetch",
            "System junk",
            "Prefetch data",
            3,
            Moderate,
            "App launch traces. Rebuilt automatically; apps start a bit slower once.",
            prefetch
        ),
        item!(
            "wucache",
            "Updates & installers",
            "Windows Update download cache",
            1,
            Safe,
            "Already-installed update packages. Update services pause briefly.",
            wucache
        ),
        item!(
            "dopt",
            "Updates & installers",
            "Delivery Optimization cache",
            1,
            Safe,
            "Update pieces Windows keeps to share with other PCs.",
            dopt,
            delta
        ),
        item!(
            "upgrade",
            "Updates & installers",
            "Old Windows installations (Windows.old...)",
            2,
            Moderate,
            "Windows.old, $Windows.~BT/~WS, $WinREAgent, ESD files. You lose the option to roll back to the previous Windows version.",
            upgrade,
            delta,
            slow
        ),
        item!(
            "cleanmgr",
            "Updates & installers",
            "Windows Disk Cleanup (every category)",
            2,
            Moderate,
            "Runs the built-in Disk Cleanup silently with all categories. Your Downloads are always excluded.",
            cleanmgr_all,
            delta,
            slow
        ),
        item!(
            "winsxs",
            "Updates & installers",
            "Component store cleanup (WinSxS /ResetBase)",
            2,
            Moderate,
            "Removes superseded system components via DISM. 5-20 min. Installed updates can no longer be uninstalled.",
            winsxs,
            delta,
            slow
        ),
        item!(
            "leftovers",
            "Updates & installers",
            "Installer & updater leftovers",
            2,
            Moderate,
            "C:\\AMD, C:\\NVIDIA, MSOCache, Edge/Google updater downloads, MSI patch cache, old Squirrel packages (Discord, Slack...).",
            leftovers
        ),
        item!(
            "msiorphans",
            "Updates & installers",
            "Orphaned Windows Installer packages",
            3,
            Aggressive,
            "Unreferenced .msi/.msp in C:\\Windows\\Installer, cross-checked against the registry AND the Windows Installer API.",
            msi_orphans
        ),
        item!(
            "olddrivers",
            "Updates & installers",
            "Old driver versions (DriverStore)",
            3,
            Aggressive,
            "Removes superseded third-party driver packages, keeps the newest. Drivers in use are never removed.",
            old_drivers,
            slow
        ),
        item!(
            "shadows",
            "Updates & installers",
            "Old restore points & shadow copies",
            3,
            Aggressive,
            "Deletes every restore point except the newest per drive.",
            shadows,
            delta
        ),
        item!(
            "hibernate",
            "Updates & installers",
            "Hibernation file (hiberfil.sys)",
            3,
            Aggressive,
            "Turns hibernation off (also disables Fast Startup). Re-enable: powercfg /h on",
            hibernate,
            delta
        ),
        item!(
            "reserved",
            "Updates & installers",
            "Reserved storage",
            3,
            Aggressive,
            "Space Windows reserves for updates (~7 GB). Freed after a restart.",
            reserved,
            delta
        ),
        item!(
            "eventlogs",
            "Updates & installers",
            "Event logs (all channels)",
            3,
            Aggressive,
            "Clears every Windows event log. Removes troubleshooting history.",
            eventlogs,
            delta
        ),
        item!(
            "gpucache",
            "Browsers & apps",
            "GPU shader caches (DirectX, NVIDIA, AMD, Intel, Qualcomm)",
            2,
            Safe,
            "Compiled shader caches. Rebuilt automatically; games may stutter briefly once.",
            gpucache
        ),
        item!(
            "uwp",
            "Browsers & apps",
            "Store app temp & web caches",
            2,
            Safe,
            "TempState, AC\\Temp and AC\\INetCache of Store apps plus the legacy IE cache.",
            uwp
        ),
        item!(
            "shortcuts",
            "Browsers & apps",
            "Broken shortcuts (Start menu & Desktop)",
            2,
            Safe,
            ".lnk files whose target no longer exists.",
            shortcuts
        ),
        item!(
            "emptydirs",
            "Browsers & apps",
            "Empty leftover folders of uninstalled apps",
            2,
            Moderate,
            "Top-level folders in Program Files, ProgramData and AppData that contain no files at all.",
            empty_dirs
        ),
        item!(
            "privacy",
            "Privacy",
            "Recent files, jump lists & Explorer history",
            3,
            Moderate,
            "Recent items, jump lists, Run box / address bar / search history, Open-Save dialog history.",
            privacy
        ),
        item!(
            "reguninstall",
            "Registry",
            "Ghost entries in \"Installed apps\"",
            2,
            Moderate,
            "Uninstall entries whose uninstaller AND install folder are gone. MSI entries are never touched.",
            reg_uninstall
        ),
        item!(
            "regapppaths",
            "Registry",
            "Dead \"App Paths\" registrations",
            2,
            Safe,
            "App Paths that point to programs that no longer exist.",
            reg_apppaths
        ),
        item!(
            "regrun",
            "Registry",
            "Dead startup entries (Run / RunOnce)",
            2,
            Moderate,
            "Autostart entries whose program was uninstalled, plus their Task Manager toggle.",
            reg_run
        ),
        item!(
            "regmui",
            "Registry",
            "MUI cache of deleted programs",
            2,
            Safe,
            "Cached display names of programs that no longer exist.",
            reg_mui
        ),
        item!(
            "regshared",
            "Registry",
            "Missing shared DLL & installer folder references",
            2,
            Moderate,
            "SharedDLLs and Installer\\Folders entries for files/folders that are gone.",
            reg_shared
        ),
        item!(
            "regcom",
            "Registry",
            "Orphaned COM / ActiveX registrations",
            3,
            Aggressive,
            "CLSID entries whose every server DLL/EXE is gone. Only absolute paths on present, readable drives are judged.",
            reg_com,
            slow
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exe_parse() {
        assert_eq!(exe_from_command(r#""C:\a b\c.exe" /x"#).as_deref(), Some(r"C:\a b\c.exe"));
        assert_eq!(exe_from_command(r"C:\a b\c.exe -x").as_deref(), Some(r"C:\a b\c.exe"));
        assert_eq!(exe_from_command("rundll32 x"), None);
    }
    #[test]
    fn missing_rules() {
        assert!(!path_missing(r"C:\Windows\System32\notepad.exe"));
        assert!(!path_missing(r"Z:\nope\x.exe")); // drive absent -> never judged
        assert!(!path_missing("shell32.dll"));
        assert!(path_missing(r"C:\Windows\definitely-not-here-broom.exe"));
    }
}
