//! Registry access with automatic `.reg` backups.
//!
//! Paths may be written as `HKLM:\...`, `HKLM\...` or `HKEY_LOCAL_MACHINE\...`.
//! All access uses the native (64-bit) view, also on ARM64.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Val {
    Dword(u32),
    Qword(u64),
    Sz(String),
    ExpandSz(String),
    MultiSz(Vec<String>),
    Binary(Vec<u8>),
}

impl Val {
    pub fn as_string(&self) -> Option<String> {
        match self {
            Val::Sz(s) | Val::ExpandSz(s) => Some(s.clone()),
            Val::Dword(d) => Some(d.to_string()),
            Val::Qword(q) => Some(q.to_string()),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Val::Dword(d) => Some(*d as u64),
            Val::Qword(q) => Some(*q),
            Val::Sz(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
    /// Build from winutil-style (Type, Value) strings.
    pub fn from_typed(ty: &str, value: &str) -> Option<Val> {
        Some(match ty.to_ascii_lowercase().as_str() {
            "dword" => Val::Dword(parse_num(value)? as u32),
            "qword" => Val::Qword(parse_num(value)?),
            "string" | "sz" => Val::Sz(value.to_string()),
            "expandstring" | "expandsz" => Val::ExpandSz(value.to_string()),
            "multistring" => Val::MultiSz(value.split('\n').map(String::from).collect()),
            "binary" => Val::Binary(
                value
                    .split([',', ' '])
                    .filter(|s| !s.is_empty())
                    .filter_map(|h| u8::from_str_radix(h.trim_start_matches("0x"), 16).ok())
                    .collect(),
            ),
            _ => return None,
        })
    }
}

fn parse_num(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse::<i64>().map(|v| v as u64).ok().or_else(|| s.parse::<u64>().ok())
    }
}

fn split(path: &str) -> Option<(RegKey, String, &'static str)> {
    let p = path.trim().trim_end_matches('\\');
    let (root, rest) = p.split_once('\\').unwrap_or((p, ""));
    let root = root.trim_end_matches(':').to_ascii_uppercase();
    let (hkey, long) = match root.as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => (HKEY_LOCAL_MACHINE, "HKEY_LOCAL_MACHINE"),
        "HKCU" | "HKEY_CURRENT_USER" => (HKEY_CURRENT_USER, "HKEY_CURRENT_USER"),
        "HKU" | "HKEY_USERS" => (HKEY_USERS, "HKEY_USERS"),
        "HKCR" | "HKEY_CLASSES_ROOT" => (HKEY_CLASSES_ROOT, "HKEY_CLASSES_ROOT"),
        _ => return None,
    };
    Some((RegKey::predef(hkey), rest.to_string(), long))
}

/// `HKLM:\X` -> `HKEY_LOCAL_MACHINE\X` (the form reg.exe and humans read).
pub fn long_name(path: &str) -> String {
    match split(path) {
        Some((_, rest, long)) if rest.is_empty() => long.to_string(),
        Some((_, rest, long)) => format!("{long}\\{rest}"),
        None => path.to_string(),
    }
}

fn open(path: &str, write: bool) -> Option<RegKey> {
    let (root, rest, _) = split(path)?;
    let flags = if write { KEY_ALL_ACCESS } else { KEY_READ } | KEY_WOW64_64KEY;
    if rest.is_empty() {
        return Some(root);
    }
    root.open_subkey_with_flags(&rest, flags).ok()
}

pub fn key_exists(path: &str) -> bool {
    open(path, false).is_some()
}

fn to_val(rv: &RegValue) -> Option<Val> {
    let b: &[u8] = &rv.bytes;
    let utf16 = |b: &[u8]| -> String {
        let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&w).trim_end_matches('\0').to_string()
    };
    Some(match rv.vtype {
        REG_DWORD if b.len() >= 4 => Val::Dword(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        REG_QWORD if b.len() >= 8 => Val::Qword(u64::from_le_bytes(b[..8].try_into().ok()?)),
        REG_SZ => Val::Sz(utf16(b)),
        REG_EXPAND_SZ => Val::ExpandSz(utf16(b)),
        REG_MULTI_SZ => Val::MultiSz(utf16(b).split('\0').filter(|s| !s.is_empty()).map(String::from).collect()),
        _ => Val::Binary(b.to_vec()),
    })
}

fn from_val(v: &Val) -> RegValue<'static> {
    let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect() };
    let (bytes, vtype) = match v {
        Val::Dword(d) => (d.to_le_bytes().to_vec(), REG_DWORD),
        Val::Qword(q) => (q.to_le_bytes().to_vec(), REG_QWORD),
        Val::Sz(s) => (utf16(s), REG_SZ),
        Val::ExpandSz(s) => (utf16(s), REG_EXPAND_SZ),
        Val::MultiSz(list) => {
            let mut b: Vec<u8> = list.iter().flat_map(|s| utf16(s)).collect();
            b.extend_from_slice(&[0, 0]);
            (b, REG_MULTI_SZ)
        }
        Val::Binary(b) => (b.clone(), REG_BINARY),
    };
    RegValue { bytes: bytes.into(), vtype }
}

/// Read a value. REG_EXPAND_SZ is returned unexpanded; use `util::expand_env`.
pub fn get(path: &str, name: &str) -> Option<Val> {
    to_val(&open(path, false)?.get_raw_value(name).ok()?)
}

pub fn get_str(path: &str, name: &str) -> Option<String> {
    get(path, name).and_then(|v| v.as_string()).map(|s| super::expand_env(&s))
}

pub fn subkeys(path: &str) -> Vec<String> {
    open(path, false).map(|k| k.enum_keys().flatten().collect()).unwrap_or_default()
}

pub fn values(path: &str) -> Vec<(String, Val)> {
    let Some(k) = open(path, false) else { return vec![] };
    k.enum_values().flatten().filter_map(|(n, v)| Some((n, to_val(&v)?))).collect()
}

pub fn value_names(path: &str) -> Vec<String> {
    values(path).into_iter().map(|(n, _)| n).collect()
}

// ---------------------------------------------------------------- writes (backed up, dry-run aware)

static BACKUP: Mutex<Option<(PathBuf, HashSet<String>, usize)>> = Mutex::new(None);

/// Start a new backup set (one folder per run).
pub fn new_backup_set() -> PathBuf {
    let dir = super::sub_dir("Backups").join(super::now_stamp());
    *BACKUP.lock().unwrap() = Some((dir.clone(), HashSet::new(), 0));
    dir
}

pub fn current_backup_dir() -> Option<PathBuf> {
    BACKUP.lock().unwrap().as_ref().filter(|b| b.2 > 0).map(|b| b.0.clone())
}

/// Export a key once per backup set before it is changed.
pub fn backup(path: &str) {
    let name = long_name(path);
    let mut g = BACKUP.lock().unwrap();
    if g.is_none() {
        *g = Some((super::sub_dir("Backups").join(super::now_stamp()), HashSet::new(), 0));
    }
    let (dir, seen, n) = g.as_mut().unwrap();
    if !seen.insert(name.to_lowercase()) || !key_exists(path) {
        return;
    }
    let _ = std::fs::create_dir_all(&*dir);
    *n += 1;
    let file = dir.join(format!("{:04}.reg", *n));
    let _ = super::run_status("reg.exe", &["export", &name, &file.to_string_lossy(), "/y"]);
}

pub fn set(path: &str, name: &str, v: &Val) -> anyhow::Result<()> {
    if super::fs::dry() {
        return Ok(());
    }
    backup(path);
    let (root, rest, _) = split(path).ok_or_else(|| anyhow::anyhow!("bad registry path {path}"))?;
    let (k, _) = root.create_subkey_with_flags(&rest, KEY_ALL_ACCESS | KEY_WOW64_64KEY)?;
    k.set_raw_value(name, &from_val(v))?;
    Ok(())
}

pub fn delete_value(path: &str, name: &str) -> anyhow::Result<()> {
    if super::fs::dry() {
        return Ok(());
    }
    backup(path);
    if let Some(k) = open(path, true) {
        match k.delete_value(name) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub fn delete_key(path: &str) -> anyhow::Result<()> {
    if super::fs::dry() {
        return Ok(());
    }
    backup(path);
    let (root, rest, _) = split(path).ok_or_else(|| anyhow::anyhow!("bad registry path {path}"))?;
    if rest.is_empty() {
        anyhow::bail!("refusing to delete a hive root");
    }
    let (parent, child) = rest.rsplit_once('\\').unwrap_or(("", &rest));
    let p = if parent.is_empty() { root } else { root.open_subkey_with_flags(parent, KEY_ALL_ACCESS | KEY_WOW64_64KEY)? };
    p.delete_subkey_all(child)?;
    Ok(())
}

/// Loaded user hives (`HKU\S-1-5-21-...`), i.e. every signed-in user.
pub fn user_sids() -> Vec<String> {
    subkeys("HKU").into_iter().filter(|s| s.starts_with("S-1-5-21-") && !s.ends_with("_Classes")).collect()
}

/// `HKCU:\X` for every loaded user hive, plus HKCU itself (de-duplicated by SID).
pub fn all_user_roots() -> Vec<String> {
    let mut v: Vec<String> = user_sids().into_iter().map(|s| format!("HKU\\{s}")).collect();
    if v.is_empty() {
        v.push("HKCU".into());
    }
    v
}

pub fn import(file: &std::path::Path) -> bool {
    super::run_status("reg.exe", &["import", &file.to_string_lossy()]).0 == 0
}
