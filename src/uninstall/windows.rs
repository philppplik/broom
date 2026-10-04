//! Windows: registry Uninstall hives (machine, WOW64, every loaded user) + Store (Appx) packages.

use super::{folder_leftovers, norm_name, Confidence, Kind, Leftover, LeftoverKind, Program, UninstallMode};
use crate::clean::windows_paths::{exe_from_command, path_missing};
use crate::util::{self, fs as bfs, reg};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn hives() -> Vec<String> {
    let rel = r"Microsoft\Windows\CurrentVersion\Uninstall";
    let mut v = vec![format!(r"HKLM\SOFTWARE\{rel}"), format!(r"HKLM\SOFTWARE\WOW6432Node\{rel}")];
    v.extend(reg::all_user_roots().into_iter().map(|u| format!(r"{u}\Software\{rel}")));
    v
}

fn is_guid(s: &str) -> bool {
    s.len() == 38 && s.starts_with('{') && s.ends_with('}')
}

/// Known silent switches by installer technology (what BCU calls "quiet uninstall string generation").
fn quiet_for(uninstall: &str, key: &str, msi: bool) -> Option<String> {
    let l = uninstall.to_lowercase();
    if msi || l.contains("msiexec") {
        let code = if is_guid(key) {
            key.to_string()
        } else {
            let i = l.find('{')?;
            uninstall[i..].split('}').next().map(|g| format!("{g}}}"))?
        };
        return Some(format!("msiexec.exe /x {code} /qn /norestart"));
    }
    let exe = exe_from_command(uninstall)?;
    let file = Path::new(&exe).file_name()?.to_string_lossy().to_lowercase();
    let quoted = format!("\"{exe}\"");
    if file.starts_with("unins") && file.ends_with(".exe") && file.chars().filter(|c| c.is_ascii_digit()).count() == 3 {
        return Some(format!("{quoted} /VERYSILENT /SUPPRESSMSGBOXES /NORESTART"));
        // Inno Setup
    }
    if file == "update.exe" && l.contains("--uninstall") {
        return Some(format!("{quoted} --uninstall -s")); // Squirrel
    }
    if let Ok(bytes) = std::fs::read(&exe) {
        let head = &bytes[..bytes.len().min(4 * 1024 * 1024)];
        let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
        if has(b"Nullsoft") || has(b"NSIS Error") {
            return Some(format!("{quoted} /S")); // NSIS
        }
        if has(b"Inno Setup") {
            return Some(format!("{quoted} /VERYSILENT /SUPPRESSMSGBOXES /NORESTART"));
        }
    }
    None
}

pub fn list() -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in hives() {
        for key in reg::subkeys(&root) {
            let k = format!(r"{root}\{key}");
            let get = |n: &str| reg::get_str(&k, n).filter(|s| !s.trim().is_empty());
            let Some(name) = get("DisplayName") else { continue };
            let num = |n: &str| reg::get(&k, n).and_then(|v| v.as_u64());
            if num("SystemComponent") == Some(1) || get("ParentKeyName").is_some() {
                continue;
            }
            if matches!(get("ReleaseType").as_deref(), Some("Update" | "Hotfix" | "Security Update")) {
                continue;
            }
            let version = get("DisplayVersion").unwrap_or_default();
            if !seen.insert(format!("{}|{}", name.to_lowercase(), version)) {
                continue;
            }
            let msi = num("WindowsInstaller") == Some(1);
            let uninstall = get("UninstallString");
            let mut quiet = get("QuietUninstallString");
            if quiet.is_none() {
                quiet = uninstall.as_deref().and_then(|u| quiet_for(u, &key, msi));
            }
            let mut location = get("InstallLocation").map(|s| PathBuf::from(s.trim().trim_matches('"')));
            if location.as_ref().map(|l| !l.is_dir()).unwrap_or(true) {
                // fall back to the folder of the uninstaller / icon when it lives in a program folder
                location = uninstall
                    .as_deref()
                    .and_then(exe_from_command)
                    .or_else(|| get("DisplayIcon").map(|i| i.split(',').next().unwrap_or("").trim_matches('"').to_string()))
                    .and_then(|e| Path::new(&e).parent().map(|p| p.to_path_buf()))
                    .filter(|p| {
                        let s = p.to_string_lossy().to_lowercase();
                        p.is_dir() && !s.contains(r"\windows\") && !s.contains("package cache") && bfs::is_safe(p)
                    });
            }
            let size = num("EstimatedSize").unwrap_or(0) * 1024;
            out.push(Program {
                id: k.clone(),
                name: name.trim().to_string(),
                version,
                publisher: get("Publisher").unwrap_or_default(),
                kind: if msi { Kind::Msi } else { Kind::Program },
                size,
                measured: false,
                installed: get("InstallDate").map(|d| fmt_date(&d)).unwrap_or_default(),
                location,
                uninstall_cmd: uninstall,
                quiet_cmd: quiet,
                reg_key: Some(k),
                protected: false,
                running: false,
            });
        }
    }
    out.extend(store_apps());
    out
}

fn fmt_date(d: &str) -> String {
    if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..])
    } else {
        d.to_string()
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Appx {
    name: String,
    package_full_name: String,
    publisher: Option<String>,
    version: Option<String>,
    install_location: Option<String>,
    display_name: Option<String>,
    non_removable: Option<bool>,
    signature_kind: Option<i64>,
}

fn store_apps() -> Vec<Program> {
    let all = if util::is_admin() { "-AllUsers" } else { "" };
    let list: Vec<Appx> = util::ps_json(&format!(
        "Get-AppxPackage {all} | Where-Object {{ -not $_.IsFramework -and -not $_.IsResourcePackage }} | ForEach-Object {{ \
           $dn=$null; try {{ $dn=(Get-AppxPackageManifest $_ -ErrorAction Stop).Package.Properties.DisplayName }} catch {{}}; \
           [pscustomobject]@{{Name=$_.Name;PackageFullName=$_.PackageFullName;Publisher=$_.Publisher;Version=[string]$_.Version;\
           InstallLocation=$_.InstallLocation;DisplayName=$dn;NonRemovable=[bool]$_.NonRemovable;SignatureKind=[int]$_.SignatureKind}} }}"
    ));
    list.into_iter()
        .map(|a| {
            let pretty = a
                .display_name
                .filter(|d| !d.starts_with("ms-resource:") && !d.is_empty())
                .unwrap_or_else(|| a.name.rsplit('.').next().unwrap_or(&a.name).to_string());
            let publisher = cert_name(&a.publisher.unwrap_or_default());
            Program {
                id: a.package_full_name.clone(),
                name: pretty,
                version: a.version.unwrap_or_default(),
                publisher,
                kind: Kind::Store,
                size: 0,
                measured: false,
                installed: String::new(),
                location: a.install_location.map(PathBuf::from),
                uninstall_cmd: None,
                quiet_cmd: Some(a.package_full_name),
                reg_key: None,
                // SignatureKind: 0 None, 1 Developer, 2 Enterprise, 3 Store, 4 System
                protected: a.non_removable.unwrap_or(false) || a.signature_kind == Some(4),
                running: false,
            }
        })
        .collect()
}

/// `CN="Anthropic, PBC", O=..., C=US` -> `Anthropic, PBC`; GUID-only subjects -> "".
fn cert_name(subject: &str) -> String {
    let Some(rest) = subject.split("CN=").nth(1) else { return String::new() };
    let name = if let Some(q) = rest.strip_prefix('"') { q.split('"').next().unwrap_or("") } else { rest.split(',').next().unwrap_or("") };
    let guid_like = name.len() == 36 && name.chars().filter(|c| *c == '-').count() == 4;
    if guid_like {
        String::new()
    } else {
        name.trim().to_string()
    }
}

pub fn still_installed(p: &Program) -> bool {
    match p.kind {
        Kind::Store => {
            let out = util::ps(&format!("[bool](Get-AppxPackage -AllUsers | Where-Object PackageFullName -eq {})", util::ps_quote(&p.id)))
                .unwrap_or_default();
            out.trim().eq_ignore_ascii_case("true")
        }
        _ => p.reg_key.as_deref().map(reg::key_exists).unwrap_or(false),
    }
}

/// Split "\"C:\x y\u.exe\" /a /b" into program + raw argument string.
fn split_cmd(cmd: &str) -> (String, String) {
    let c = util::expand_env(cmd.trim());
    if let Some(exe) = exe_from_command(&c) {
        let rest = c[c.find(&exe).map(|i| i + exe.len()).unwrap_or(0)..].trim_start_matches('"').trim().to_string();
        return (exe, rest);
    }
    let mut it = c.splitn(2, ' ');
    (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string())
}

fn run_and_wait(cmd: &str, p: &Program) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let (prog, args) = split_cmd(cmd);
    let mut child = std::process::Command::new(&prog).raw_arg(&args).spawn().map_err(|e| format!("could not start {prog}: {e}"))?;
    let status = child.wait().map_err(|e| e.to_string())?;
    // Many uninstallers relaunch themselves from %TEMP% and exit immediately: wait for the entry to vanish.
    let start = Instant::now();
    while still_installed(p) && start.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(1000));
    }
    if still_installed(p) {
        Err(format!("still installed (exit code {})", status.code().unwrap_or(-1)))
    } else {
        Ok("removed".into())
    }
}

pub fn uninstall(p: &Program, mode: UninstallMode) -> Result<String, String> {
    if p.kind == Kind::Store {
        let all = if util::is_admin() { "-AllUsers" } else { "" };
        let (code, out) = (
            0,
            util::ps(&format!("Remove-AppxPackage -Package {} {all} -ErrorAction Stop; 'ok'", util::ps_quote(&p.id))).unwrap_or_default(),
        );
        let _ = code;
        return if out.contains("ok") && !still_installed(p) {
            Ok("removed".into())
        } else {
            Err(out.trim().lines().last().unwrap_or("failed").to_string())
        };
    }
    let cmd = match (&mode, &p.quiet_cmd, &p.uninstall_cmd) {
        (UninstallMode::Auto, Some(q), _) => q.clone(),
        (_, _, Some(u)) => {
            if p.kind == Kind::Msi && !u.to_lowercase().contains("msiexec") {
                u.clone()
            } else if p.kind == Kind::Msi {
                // normalise "/I{GUID}" (repair) to "/X{GUID}" (remove)
                u.replace("/I{", "/X{").replace("/i{", "/x{")
            } else {
                u.clone()
            }
        }
        (_, Some(q), None) => q.clone(),
        _ => return Err("no uninstaller registered".into()),
    };
    run_and_wait(&cmd, p)
}

pub fn delete_service(name: &str) -> bool {
    if bfs::dry() {
        return true;
    }
    let _ = util::run_status("sc.exe", &["stop", name]);
    util::run_status("sc.exe", &["delete", name]).0 == 0
}

fn program_files_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "CommonProgramFiles", "CommonProgramFiles(x86)"]
        .iter()
        .filter_map(|e| util::env_path(e))
        .collect();
    v.push(PathBuf::from(r"C:\Program Files (Arm)"));
    if let Some(pd) = util::env_path("ProgramData") {
        v.push(pd.join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    for h in util::profiles() {
        for r in [
            "AppData/Local",
            "AppData/Roaming",
            "AppData/LocalLow",
            "AppData/Local/Programs",
            "AppData/Roaming/Microsoft/Windows/Start Menu/Programs",
        ] {
            v.push(h.join(r));
        }
    }
    v
}

pub fn leftovers(p: &Program, others: &[&Program]) -> Vec<Leftover> {
    let mut out = Vec::new();
    if p.kind == Kind::Store {
        // Store apps keep their data in Packages\<FamilyName>
        let family = family_name(&p.id);
        for h in util::profiles() {
            let d = h.join("AppData/Local/Packages").join(&family);
            if d.is_dir() {
                out.push(Leftover {
                    kind: LeftoverKind::Folder,
                    size: bfs::size_of(&d),
                    path: d.to_string_lossy().into(),
                    confidence: Confidence::High,
                    reason: "app data of the Store package".into(),
                });
            }
        }
        return out;
    }
    folder_leftovers(p, others, &program_files_roots(), &mut out);

    // Registry: Software\<Vendor>\<Product> and Software\<Product>
    let name = norm_name(&p.name);
    let publisher = norm_name(&p.publisher);
    let vendor_shared = publisher.is_empty() || others.iter().any(|o| norm_name(&o.publisher) == publisher);
    let mut roots = vec![r"HKLM\SOFTWARE".to_string(), r"HKLM\SOFTWARE\WOW6432Node".to_string()];
    roots.extend(reg::all_user_roots().into_iter().map(|u| format!(r"{u}\Software")));
    let other_names: Vec<String> = others.iter().map(|o| norm_name(&o.name)).collect();
    let skip =
        ["microsoft", "classes", "policies", "wow6432node", "windows", "clients", "registeredapplications", "intel", "nvidia corporation"];
    for r in &roots {
        for sub in reg::subkeys(r) {
            let n = norm_name(&sub);
            if skip.contains(&sub.to_lowercase().as_str()) || n.len() < 3 {
                continue;
            }
            let k = format!(r"{r}\{sub}");
            if n == name && !other_names.contains(&n) {
                out.push(Leftover {
                    kind: LeftoverKind::RegKey,
                    size: 0,
                    path: k,
                    confidence: Confidence::High,
                    reason: "registry key named after the program".into(),
                });
            } else if n == publisher {
                for inner in reg::subkeys(&k) {
                    if norm_name(&inner) == name {
                        out.push(Leftover {
                            kind: LeftoverKind::RegKey,
                            size: 0,
                            path: format!(r"{k}\{inner}"),
                            confidence: Confidence::High,
                            reason: "vendor\\product registry key".into(),
                        });
                    }
                }
                if !vendor_shared && reg::subkeys(&k).is_empty() {
                    out.push(Leftover {
                        kind: LeftoverKind::RegKey,
                        size: 0,
                        path: k,
                        confidence: Confidence::Medium,
                        reason: "vendor key, no other product of this vendor".into(),
                    });
                }
            }
        }
    }
    if let Some(k) = p.reg_key.as_deref().filter(|k| reg::key_exists(k)) {
        out.push(Leftover {
            kind: LeftoverKind::RegKey,
            size: 0,
            path: k.into(),
            confidence: Confidence::High,
            reason: "uninstall entry left behind".into(),
        });
    }

    // Anything that still points into the install folder: autostart values, services, scheduled tasks
    let Some(loc) = p.location.as_ref().map(|l| l.to_string_lossy().to_lowercase()).filter(|l| l.len() > 8) else { return out };
    let mut run_roots = vec!["HKLM".to_string()];
    run_roots.extend(reg::all_user_roots());
    for h in run_roots {
        for rel in [r"Software\Microsoft\Windows\CurrentVersion\Run", r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run"] {
            let k = format!(r"{h}\{rel}");
            for (vn, v) in reg::values(&k) {
                if v.as_string().map(|s| util::expand_env(&s).to_lowercase().contains(&loc)).unwrap_or(false) {
                    out.push(Leftover {
                        kind: LeftoverKind::RegValue { name: vn.clone() },
                        size: 0,
                        path: k.clone(),
                        confidence: Confidence::High,
                        reason: format!("autostart \"{vn}\""),
                    });
                }
            }
        }
    }
    let svc = r"HKLM\SYSTEM\CurrentControlSet\Services";
    for s in reg::subkeys(svc) {
        if let Some(img) = reg::get_str(&format!(r"{svc}\{s}"), "ImagePath") {
            if img.to_lowercase().contains(&loc) {
                out.push(Leftover {
                    kind: LeftoverKind::Service,
                    size: 0,
                    path: s,
                    confidence: Confidence::High,
                    reason: "service runs from the install folder".into(),
                });
            }
        }
    }
    if let Some(w) = util::env_path("windir") {
        let tasks = w.join(r"System32\Tasks");
        bfs::walk_files(&tasks, &bfs::Walk::all(), &mut |f, md| {
            if md.len() > 256 * 1024 {
                return;
            }
            let Ok(bytes) = std::fs::read(f) else { return };
            let text = if bytes.starts_with(&[0xFF, 0xFE]) {
                String::from_utf16_lossy(&bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            if text.to_lowercase().contains(&loc) {
                let tn = f.strip_prefix(&tasks).map(|r| format!("\\{}", r.to_string_lossy())).unwrap_or_default();
                out.push(Leftover {
                    kind: LeftoverKind::Task,
                    size: 0,
                    path: tn,
                    confidence: Confidence::High,
                    reason: "scheduled task runs from the install folder".into(),
                });
            }
        });
    }
    out
}

/// Microsoft.App_1.2.3.0_x64__8wekyb3d8bbwe -> Microsoft.App_8wekyb3d8bbwe
fn family_name(full: &str) -> String {
    let parts: Vec<&str> = full.split('_').collect();
    if parts.len() >= 5 {
        format!("{}_{}", parts[0], parts[parts.len() - 1])
    } else {
        full.to_string()
    }
}

/// Is an "Installed apps" entry orphaned (uninstaller gone)? Used to flag broken entries in the list.
#[allow(dead_code)]
pub fn is_orphan(p: &Program) -> bool {
    p.uninstall_cmd.as_deref().and_then(exe_from_command).map(|e| path_missing(&e)).unwrap_or(false)
}
