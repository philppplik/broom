//! Safe file-system primitives.
//!
//! Rules every deletion in Broom goes through:
//! * never descend into or delete through reparse points / symlinks / junctions
//! * never touch cloud placeholders (OneDrive "files on demand" are reparse points too)
//! * never empty a protected root or anything inside the user's personal folders
//! * files in use are skipped, never forced

use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// Global dry-run switch. When set, nothing on disk or in the registry is modified.
pub static DRY_RUN: AtomicBool = AtomicBool::new(false);

thread_local! {
    static THREAD_DRY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// True when nothing may be changed: either the global switch (F2 / --dry-run) is on,
/// or this thread runs an analysis. Thread-local so an analysis never affects a real job elsewhere.
pub fn dry() -> bool {
    DRY_RUN.load(Ordering::Relaxed) || THREAD_DRY.with(|c| c.get())
}

pub fn set_thread_dry(on: bool) {
    THREAD_DRY.with(|c| c.set(on));
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Freed {
    pub bytes: u64,
    pub files: u64,
}

impl std::ops::AddAssign for Freed {
    fn add_assign(&mut self, o: Self) {
        self.bytes += o.bytes;
        self.files += o.files;
    }
}

pub fn is_link(md: &Metadata) -> bool {
    if md.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        const RECALL_ON_OPEN: u32 = 0x40000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
        if md.file_attributes() & (FILE_ATTRIBUTE_REPARSE_POINT | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0 {
            return true;
        }
    }
    false
}

/// Case-insensitive wildcard match supporting `*` and `?`.
pub fn wildmatch(pat: &str, name: &str) -> bool {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni, mut star, mut mark) = (0usize, 0usize, usize::MAX, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ni;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Expand a path whose components may contain `*`/`?` (e.g. `C:\Users\x\AppData\Local\Packages\*\TempState`).
/// Wildcard components never match links.
pub fn expand(pattern: &Path) -> Vec<PathBuf> {
    let s = pattern.to_string_lossy();
    if !s.contains('*') && !s.contains('?') {
        return if fs::symlink_metadata(pattern).is_ok() { vec![pattern.to_path_buf()] } else { vec![] };
    }
    let mut current: Vec<PathBuf> = vec![PathBuf::new()];
    for comp in pattern.components() {
        let part = comp.as_os_str().to_string_lossy().to_string();
        let mut next = Vec::new();
        for base in &current {
            if part.contains('*') || part.contains('?') {
                if let Ok(rd) = fs::read_dir(if base.as_os_str().is_empty() { Path::new(".") } else { base }) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().to_string();
                        if wildmatch(&part, &name) {
                            if let Ok(md) = fs::symlink_metadata(e.path()) {
                                if !is_link(&md) {
                                    next.push(base.join(&name));
                                }
                            }
                        }
                    }
                }
            } else {
                next.push(base.join(comp.as_os_str()));
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    current.into_iter().filter(|p| fs::symlink_metadata(p).is_ok()).collect()
}

fn protected_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        for var in ["SystemDrive", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "PUBLIC"] {
            if let Some(p) = super::env_path(var) {
                v.push(p);
            }
        }
        if let Some(w) = super::env_path("windir") {
            v.push(w.join("System32"));
            v.push(w.join("SysWOW64"));
            v.push(w.join("WinSxS"));
        }
        v.push(PathBuf::from(r"C:\Program Files (Arm)"));
        v.push(PathBuf::from(r"C:\Users"));
    }
    #[cfg(not(windows))]
    {
        for p in [
            "/",
            "/bin",
            "/boot",
            "/dev",
            "/etc",
            "/home",
            "/lib",
            "/opt",
            "/proc",
            "/root",
            "/sbin",
            "/sys",
            "/usr",
            "/var",
            "/Applications",
            "/Library",
            "/System",
            "/Users",
            "/private",
            "/private/var",
        ] {
            v.push(PathBuf::from(p));
        }
    }
    for pr in super::profiles() {
        for s in
            ["", "AppData", "AppData/Local", "AppData/Roaming", "AppData/LocalLow", "Desktop", "Library", ".cache", ".local", ".config"]
        {
            v.push(if s.is_empty() { pr.clone() } else { pr.join(s) });
        }
    }
    v
}

fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) || cfg!(target_os = "macos") {
        s.to_lowercase()
    } else {
        s
    }
}

/// May this path be emptied / deleted?
pub fn is_safe(p: &Path) -> bool {
    let n = norm(p);
    if n.len() < 4 {
        return false;
    }
    if protected_roots().iter().any(|r| norm(r) == n) {
        return false;
    }
    let personal = [
        "/documents",
        "/downloads",
        "/pictures",
        "/videos",
        "/movies",
        "/music",
        "/favorites",
        "/contacts",
        "/saved games",
        "/onedrive",
        "/icloud drive",
        "/dropbox",
        "/google drive",
        // localized XDG / macOS folder names (de, fr, es, it, pt, nl)
        "/dokumente",
        "/bilder",
        "/musik",
        "/schreibtisch",
        "/documenti",
        "/immagini",
        "/musica",
        "/documentos",
        "/imágenes",
        "/música",
        "/imagens",
        "/vidéos",
        "/images",
        "/musique",
        "/téléchargements",
        "/descargas",
        "/scaricati",
        "/documenten",
        "/afbeeldingen",
    ];
    let lower = n.to_lowercase();
    if xdg_user_dirs().iter().any(|d| lower == *d || lower.starts_with(&format!("{d}/"))) {
        return false;
    }
    // personal-folder names are matched case-insensitively on every OS (Linux paths are case-sensitive, the guard must not be)
    let l = n.to_lowercase();
    let in_app_data = l.contains("/appdata/") || l.contains("/library/caches") || l.contains("/.cache/");
    if !in_app_data {
        for seg in personal {
            if l.contains(&format!("{seg}/")) || l.ends_with(seg) || l.contains(&format!("{seg} ")) {
                return false;
            }
        }
    }
    true
}

/// The user's real personal folders on Linux (~/.config/user-dirs.dirs), lower-cased.
fn xdg_user_dirs() -> Vec<String> {
    let mut v = Vec::new();
    if cfg!(target_os = "linux") {
        for h in super::profiles() {
            let Ok(text) = fs::read_to_string(h.join(".config/user-dirs.dirs")) else { continue };
            for line in text.lines().filter(|l| l.starts_with("XDG_") && !l.starts_with("XDG_DESKTOP")) {
                if let Some(val) = line.split_once('=').map(|x| x.1.trim().trim_matches('"')) {
                    let p = val.replace("$HOME", &h.to_string_lossy());
                    if p.trim_end_matches('/') != h.to_string_lossy().trim_end_matches('/') {
                        v.push(norm(Path::new(&p)).to_lowercase());
                    }
                }
            }
        }
    }
    v
}

#[derive(Clone, Default)]
pub struct Walk {
    pub recurse: bool,
    pub older_than_days: u64,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Walk {
    pub fn all() -> Self {
        Walk { recurse: true, ..Default::default() }
    }
    pub fn pattern(pats: &[&str]) -> Self {
        Walk { recurse: true, include: pats.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }
    pub fn flat(mut self) -> Self {
        self.recurse = false;
        self
    }
    pub fn older(mut self, days: u64) -> Self {
        self.older_than_days = days;
        self
    }
    pub fn except(mut self, names: &[&str]) -> Self {
        self.exclude = names.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Visit every regular file below `root` (or `root` itself if it is a file). Never follows links.
pub fn walk_files(root: &Path, w: &Walk, f: &mut dyn FnMut(&Path, &Metadata)) {
    let Ok(md) = fs::symlink_metadata(root) else { return };
    if is_link(&md) {
        return;
    }
    let cutoff = if w.older_than_days > 0 { SystemTime::now().checked_sub(Duration::from_secs(w.older_than_days * 86_400)) } else { None };
    let accept = |path: &Path, md: &Metadata| -> bool {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if w.exclude.iter().any(|e| e.eq_ignore_ascii_case(&name)) {
            return false;
        }
        if !w.include.is_empty() && !w.include.iter().any(|p| wildmatch(p, &name)) {
            return false;
        }
        if let (Some(c), Ok(m)) = (cutoff, md.modified()) {
            if m > c {
                return false;
            }
        }
        true
    };
    if md.is_file() {
        if accept(root, &md) {
            f(root, &md);
        }
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(md) = fs::symlink_metadata(&path) else { continue };
            if is_link(&md) {
                continue;
            }
            if md.is_dir() {
                if w.recurse {
                    stack.push(path);
                }
            } else if accept(&path, &md) {
                f(&path, &md);
            }
        }
    }
}

pub fn size_of(root: &Path) -> u64 {
    let mut total = 0;
    walk_files(root, &Walk::all(), &mut |_, md| total += md.len());
    total
}

/// Parallel size of a folder: splits the first level across threads.
pub fn size_of_fast(root: &Path) -> u64 {
    use rayon::prelude::*;
    let Ok(rd) = fs::read_dir(root) else { return size_of(root) };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.par_iter().map(|p| size_of(p)).sum()
}

fn delete_file(path: &Path, md: &Metadata) -> bool {
    if fs::remove_file(path).is_ok() {
        return true;
    }
    if md.permissions().readonly() {
        let mut perm = md.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        if fs::set_permissions(path, perm).is_ok() {
            return fs::remove_file(path).is_ok();
        }
    }
    false
}

/// Delete (or, in dry run, count) one file.
pub fn remove_file(path: &Path) -> Freed {
    let Ok(md) = fs::symlink_metadata(path) else { return Freed::default() };
    if is_link(&md) || !md.is_file() {
        return Freed::default();
    }
    if dry() || delete_file(path, &md) {
        Freed { bytes: md.len(), files: 1 }
    } else {
        Freed::default()
    }
}

/// Remove empty sub-folders deepest first (never `root` itself).
pub fn remove_empty_dirs(root: &Path) {
    if dry() {
        return;
    }
    let mut all = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if let Ok(md) = fs::symlink_metadata(&p) {
                if md.is_dir() && !is_link(&md) {
                    all.push(p.clone());
                    stack.push(p);
                }
            }
        }
    }
    for d in all.iter().rev() {
        let _ = fs::remove_dir(d);
    }
}

/// Empty a folder (or delete a file). Wildcards in the path are expanded.
pub fn clear(path: impl AsRef<Path>, w: &Walk) -> Freed {
    let mut freed = Freed::default();
    for p in expand(path.as_ref()) {
        if !is_safe(&p) {
            super::log(format!("SKIP protected path {}", p.display()));
            continue;
        }
        walk_files(&p, w, &mut |f, md| {
            if dry() || delete_file(f, md) {
                freed += Freed { bytes: md.len(), files: 1 };
            }
        });
        if w.recurse && p.is_dir() {
            remove_empty_dirs(&p);
        }
    }
    freed
}

pub fn clear_all(path: impl AsRef<Path>) -> Freed {
    clear(path, &Walk::all())
}

/// Delete a folder completely (contents + the folder itself).
pub fn remove_dir_all_safe(path: &Path) -> Freed {
    let f = clear_all(path);
    if !dry() && is_safe(path) {
        let _ = fs::remove_dir(path);
    }
    f
}

/// Does `dir` contain no files at all (only, at most, empty folders)?
pub fn tree_is_empty(dir: &Path) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0;
    while let Some(d) = stack.pop() {
        seen += 1;
        if seen > 200 {
            return false;
        }
        let Ok(rd) = fs::read_dir(&d) else { return false };
        for e in rd.flatten() {
            let Ok(md) = fs::symlink_metadata(e.path()) else { return false };
            if is_link(&md) || !md.is_dir() {
                return false;
            }
            stack.push(e.path());
        }
    }
    true
}

// ---------------------------------------------------------------- Quarantine
//
// Leftovers found after an uninstall are moved here instead of being deleted,
// so a bad match can be put back. (Idea credit: Prune by jimman0I, MIT.)

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct QuarantineEntry {
    pub original: PathBuf,
    pub stored: PathBuf,
    pub bytes: u64,
}

pub fn quarantine_dir(label: &str) -> PathBuf {
    let safe: String = label.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    super::sub_dir("Quarantine").join(format!("{}-{}", super::now_stamp(), super::ellipsize(&safe, 40)))
}

/// Move a file or folder into quarantine. Falls back to delete-by-copy across volumes.
pub fn quarantine(path: &Path, qdir: &Path) -> Option<QuarantineEntry> {
    let md = fs::symlink_metadata(path).ok()?;
    if is_link(&md) || !is_safe(path) {
        return None;
    }
    let bytes = if md.is_dir() { size_of(path) } else { md.len() };
    if dry() {
        return Some(QuarantineEntry { original: path.into(), stored: PathBuf::new(), bytes });
    }
    fs::create_dir_all(qdir).ok()?;
    let n = fs::read_dir(qdir).map(|r| r.count()).unwrap_or(0);
    let stored = qdir.join(format!("{n:04}-{}", path.file_name()?.to_string_lossy()));
    if fs::rename(path, &stored).is_err() {
        copy_tree(path, &stored).ok()?;
        if md.is_dir() {
            remove_dir_all_safe(path);
        } else {
            fs::remove_file(path).ok()?;
        }
    }
    let e = QuarantineEntry { original: path.into(), stored, bytes };
    let manifest = qdir.join("manifest.json");
    let mut list: Vec<QuarantineEntry> = fs::read_to_string(&manifest).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    list.push(e.clone());
    let _ = fs::write(&manifest, serde_json::to_string_pretty(&list).unwrap_or_default());
    Some(e)
}

pub fn restore_quarantine(qdir: &Path) -> usize {
    let manifest = qdir.join("manifest.json");
    let list: Vec<QuarantineEntry> = fs::read_to_string(&manifest).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut n = 0;
    for e in &list {
        if let Some(parent) = e.original.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::rename(&e.stored, &e.original).is_ok() || copy_tree(&e.stored, &e.original).is_ok() {
            n += 1;
        }
    }
    let _ = fs::remove_dir_all(qdir);
    n
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let md = fs::symlink_metadata(from)?;
    if md.is_dir() {
        fs::create_dir_all(to)?;
        for e in fs::read_dir(from)?.flatten() {
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Free space of the volume that holds `p`.
pub fn free_space(p: &Path) -> u64 {
    super::sys::free_space(p)
}

pub fn system_root() -> PathBuf {
    if cfg!(windows) {
        super::env_path("SystemDrive").map(|d| PathBuf::from(format!("{}\\", d.display()))).unwrap_or_else(|| PathBuf::from(r"C:\"))
    } else {
        PathBuf::from("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard() {
        assert!(wildmatch("thumbcache_*.db", "thumbcache_256.db"));
        assert!(wildmatch("*.LOG", "cbs.log"));
        assert!(!wildmatch("*.log", "cbs.log.old"));
        assert!(wildmatch("Profile *", "Profile 2"));
    }

    /// The core promise: emptying a folder never deletes anything through a junction/symlink.
    #[test]
    fn never_follows_links() {
        let base = std::env::temp_dir().join(format!("broom-link-test-{}", std::process::id()));
        let junk = base.join("junk");
        let victim = base.join("victim");
        fs::create_dir_all(junk.join("sub")).unwrap();
        fs::create_dir_all(&victim).unwrap();
        fs::write(junk.join("a.tmp"), b"x").unwrap();
        fs::write(junk.join("sub/b.tmp"), b"y").unwrap();
        fs::write(victim.join("precious.txt"), b"keep me").unwrap();
        #[cfg(windows)]
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J", &junk.join("link").to_string_lossy(), &victim.to_string_lossy()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&victim, junk.join("link")).is_ok();
        assert!(made, "could not create test link");
        let freed = clear_all(&junk);
        assert_eq!(freed.files, 2, "both junk files deleted");
        assert!(victim.join("precious.txt").exists(), "file behind the link must survive");
        assert!(!junk.join("a.tmp").exists() && !junk.join("sub").exists());
        let _ = fs::remove_dir(junk.join("link"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn quarantine_roundtrip() {
        let base = std::env::temp_dir().join(format!("broom-q-test-{}", std::process::id()));
        std::env::set_var("BROOM_DATA_DIR", base.join("data"));
        let target = base.join("leftover");
        fs::create_dir_all(target.join("x")).unwrap();
        fs::write(target.join("x/f.bin"), vec![0u8; 1000]).unwrap();
        let q = quarantine_dir("test app");
        let e = quarantine(&target, &q).expect("quarantined");
        assert_eq!(e.bytes, 1000);
        assert!(!target.exists());
        assert_eq!(restore_quarantine(&q), 1);
        assert!(target.join("x/f.bin").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn protects_roots() {
        assert!(!is_safe(&system_root()));
        assert!(!is_safe(&super::super::home()));
        assert!(!is_safe(&super::super::home().join("Documents").join("x")));
        assert!(!is_safe(&super::super::home().join("Dokumente").join("x")));
        assert!(!is_safe(&super::super::home().join("Pictures")));
        assert!(is_safe(&super::super::home().join("AppData").join("Local").join("Temp")));
    }
}
