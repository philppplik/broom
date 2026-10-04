//! Small cross-platform helpers: commands, formatting, paths, logging.

pub mod fs;
#[cfg(windows)]
pub mod reg;
pub mod sys;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How risky an action is. Drives colors, presets and confirmations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Risk {
    Safe,
    Moderate,
    Aggressive,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Safe => "SAFE",
            Risk::Moderate => "MODERATE",
            Risk::Aggressive => "AGGRESSIVE",
        }
    }
}

pub fn fmt_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    match i {
        0 => format!("{bytes} B"),
        1 => format!("{v:.0} KB"),
        2 => format!("{v:.1} MB"),
        _ => format!("{v:.2} {}", UNITS[i]),
    }
}

pub fn now_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Native CPU architecture (not the one we were compiled for when emulated).
pub fn native_arch() -> String {
    #[cfg(windows)]
    {
        if let Some(reg::Val::Sz(a)) =
            reg::get(r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "PROCESSOR_ARCHITECTURE")
        {
            return match a.as_str() {
                "ARM64" => "ARM64".into(),
                "AMD64" => "x64".into(),
                o => o.into(),
            };
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(o) = run("uname", &["-m"]) {
            return match o.trim() {
                "arm64" | "aarch64" => "ARM64".into(),
                "x86_64" => "x64".into(),
                o => o.into(),
            };
        }
    }
    std::env::consts::ARCH.into()
}

pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        run("id", &["-u"]).map(|s| s.trim() == "0").unwrap_or(false)
    }
}

/// Re-launch this executable elevated (UAC). Returns true if a new process was started.
#[cfg(windows)]
pub fn relaunch_elevated() -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let args: Vec<String> = std::env::args().skip(1).map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a }).collect();
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file, params) = (w("runas"), w(&exe.to_string_lossy()), w(&args.join(" ")));
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), SW_SHOWNORMAL) };
    r as isize > 32
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Run a program, return stdout (lossy UTF-8). Errors only if it could not start.
pub fn run(prog: &str, args: &[&str]) -> anyhow::Result<String> {
    let out = hidden(Command::new(prog).args(args).stdin(Stdio::null())).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run a program, return (exit code, stdout + stderr).
pub fn run_status(prog: &str, args: &[&str]) -> (i32, String) {
    match hidden(Command::new(prog).args(args).stdin(Stdio::null())).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.code().unwrap_or(-1), s)
        }
        Err(e) => (-1, e.to_string()),
    }
}

pub fn has_cmd(prog: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    run_status(finder, &[prog]).0 == 0
}

/// Run a PowerShell script (Windows PowerShell 5.1, always present). Output forced to UTF-8.
#[cfg(windows)]
pub fn ps(script: &str) -> anyhow::Result<String> {
    let full = format!("$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=[Text.Encoding]::UTF8;{script}");
    let utf16: Vec<u8> = full.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    run("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &base64(&utf16)])
}

/// Run PowerShell and parse the result as a JSON array of T. `expr` must produce objects.
#[cfg(windows)]
pub fn ps_json<T: serde::de::DeserializeOwned>(expr: &str) -> Vec<T> {
    let script = format!("ConvertTo-Json -Compress -Depth 4 -InputObject @({expr})");
    match ps(&script) {
        Ok(s) => serde_json::from_str::<Vec<T>>(s.trim()).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Escape a string for a single-quoted PowerShell literal.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Every real user profile on this machine (Windows: all, Unix: $HOME and the sudo user's home).
pub fn profiles() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        let list = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";
        for sid in reg::subkeys(list) {
            if !sid.starts_with("S-1-5-21-") || sid.ends_with(".bak") {
                continue;
            }
            if let Some(reg::Val::Sz(p) | reg::Val::ExpandSz(p)) = reg::get(&format!(r"{list}\{sid}"), "ProfileImagePath") {
                let p = PathBuf::from(expand_env(&p));
                if p.is_dir() {
                    v.push(p);
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(u) = std::env::var("SUDO_USER") {
            let p = if cfg!(target_os = "macos") { PathBuf::from("/Users").join(&u) } else { PathBuf::from("/home").join(&u) };
            if p.is_dir() {
                v.push(p);
            }
        }
    }
    let h = home();
    if h.is_dir() && !v.iter().any(|p| p == &h) {
        v.push(h);
    }
    v
}

/// Create a System Restore point (Windows; lifts the 1-per-24h limit for this call).
#[cfg(windows)]
pub fn create_restore_point(desc: &str) -> bool {
    if fs::dry() {
        return true;
    }
    let script = format!(
        r#"$k='HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'; $n='SystemRestorePointCreationFrequency';
$o=(Get-ItemProperty $k -Name $n -ErrorAction SilentlyContinue).$n; New-ItemProperty $k -Name $n -Value 0 -PropertyType DWord -Force | Out-Null;
$r=Invoke-CimMethod -Namespace root/default -ClassName SystemRestore -MethodName CreateRestorePoint -Arguments @{{Description={};RestorePointType=[uint32]12;EventType=[uint32]100}};
if($null -eq $o){{Remove-ItemProperty $k -Name $n -Force}}else{{Set-ItemProperty $k -Name $n -Value $o}}; $r.ReturnValue"#,
        ps_quote(desc)
    );
    ps(&script).map(|o| o.trim() == "0").unwrap_or(false)
}
#[cfg(not(windows))]
pub fn create_restore_point(_desc: &str) -> bool {
    false
}

/// Expand %VAR% (Windows style) environment references.
pub fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Where Broom keeps logs, backups, journal and quarantine.
pub fn data_dir() -> PathBuf {
    let base = if let Some(over) = env_path("BROOM_DATA_DIR") {
        over
    } else if cfg!(windows) {
        env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join("Broom")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/Broom")
    } else {
        env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join("broom")
    };
    let _ = std::fs::create_dir_all(&base);
    base
}

pub fn sub_dir(name: &str) -> PathBuf {
    let p = data_dir().join(name);
    let _ = std::fs::create_dir_all(&p);
    p
}

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn log_path() -> PathBuf {
    let mut g = LOG.lock().unwrap();
    g.get_or_insert_with(|| sub_dir("Logs").join(format!("broom-{}.log", now_stamp()))).clone()
}

pub fn log(msg: impl AsRef<str>) {
    let path = log_path();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg.as_ref());
    }
}

pub fn open_in_file_manager(p: &Path) {
    let prog = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(prog).arg(p).spawn();
}

/// Truncate to `max` chars with an ellipsis.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
