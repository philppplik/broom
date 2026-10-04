//! Junk cleaner: a catalog of items per OS, each able to dry-run.

mod common;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// Path checks shared with the uninstaller.
#[cfg(windows)]
pub mod windows_paths {
    pub use super::windows::{exe_from_command, path_missing};
}

use crate::util::fs::{self as bfs, Freed};
use crate::util::{self, Risk};

#[derive(Default, Clone, Debug)]
pub struct Outcome {
    pub bytes: u64,
    pub files: u64,
    pub note: Option<String>,
}

impl Outcome {
    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }
}

impl From<Freed> for Outcome {
    fn from(f: Freed) -> Self {
        Outcome { bytes: f.bytes, files: f.files, note: None }
    }
}

impl std::ops::AddAssign<Freed> for Outcome {
    fn add_assign(&mut self, f: Freed) {
        self.bytes += f.bytes;
        self.files += f.files;
    }
}

pub struct Item {
    pub id: &'static str,
    pub category: &'static str,
    pub name: &'static str,
    /// 1 = Quick, 2 = Deep, 3 = Nuclear
    pub tier: u8,
    pub risk: Risk,
    pub desc: &'static str,
    /// Processes that lock these files (offered to close before cleaning).
    pub procs: &'static [&'static str],
    /// Size is measured by free-space difference (Windows tools do the work).
    pub delta: bool,
    pub slow: bool,
    pub run: fn() -> Outcome,
}

pub const TIERS: [&str; 3] = ["Quick", "Deep", "Nuclear"];

pub fn catalog() -> Vec<Item> {
    #[allow(unused_mut)]
    let mut v: Vec<Item> = Vec::new();
    #[cfg(windows)]
    v.extend(windows::items());
    #[cfg(target_os = "macos")]
    v.extend(macos::items());
    #[cfg(target_os = "linux")]
    v.extend(linux::items());
    v.extend(common::items());
    // stable order: by category as declared, then as listed
    let order = categories();
    v.sort_by_key(|i| order.iter().position(|c| *c == i.category).unwrap_or(99));
    v
}

pub fn categories() -> Vec<&'static str> {
    vec!["System junk", "Updates & installers", "Browsers & apps", "Developer", "Registry", "Privacy"]
}

/// Run one item, measuring by free-space delta where needed.
pub fn execute(item: &Item) -> Outcome {
    let root = bfs::system_root();
    let before = if item.delta && !bfs::dry() { bfs::free_space(&root) } else { 0 };
    let mut out = (item.run)();
    if item.delta && !bfs::dry() {
        out.bytes = bfs::free_space(&root).saturating_sub(before);
    }
    util::log(format!(
        "{}{}: {} {}",
        if bfs::dry() { "[dry] " } else { "" },
        item.name,
        util::fmt_size(out.bytes),
        out.note.clone().unwrap_or_default()
    ));
    out
}

/// Running processes (lower-case names without .exe) among `names`.
pub fn running(names: &[&str]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (n, _) in util::sys::processes() {
        let n = n.trim_end_matches(".exe").to_string();
        if names.iter().any(|w| w.eq_ignore_ascii_case(&n)) && !found.contains(&n) {
            found.push(n);
        }
    }
    found
}

pub fn kill(names: &[String]) {
    for (n, pid) in util::sys::processes() {
        if names.iter().any(|w| *w == n.trim_end_matches(".exe")) {
            util::sys::kill(pid);
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(800));
}
