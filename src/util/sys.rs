//! Hardware / process facts with one API on every OS.
//! Windows uses windows-sys directly (small, fast); macOS/Linux use `sysinfo`.

use std::path::PathBuf;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Disk {
    pub mount: PathBuf,
    pub name: String,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
}

#[derive(Clone, Debug, serde::Serialize, Default)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
}

pub fn cpu_cores() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
    use windows_sys::Win32::System::SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    pub fn disks() -> Vec<Disk> {
        let mask = unsafe { GetLogicalDrives() };
        let mut v = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let w = wide(&root);
            let ty = unsafe { GetDriveTypeW(w.as_ptr()) };
            // 2 removable, 3 fixed, 4 remote, 5 cdrom, 6 ramdisk
            if ty != 2 && ty != 3 {
                continue;
            }
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            if unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
                continue;
            }
            v.push(Disk { mount: PathBuf::from(&root), name: root.clone(), total, free: avail, removable: ty == 2 });
        }
        v
    }

    pub fn memory() -> Memory {
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
            return Memory::default();
        }
        Memory { total: m.ullTotalPhys, available: m.ullAvailPhys }
    }

    pub fn uptime_secs() -> u64 {
        unsafe { GetTickCount64() / 1000 }
    }

    pub fn cpu_name() -> String {
        crate::util::reg::get_str(r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString")
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_else(|| "Unknown CPU".into())
    }

    /// (lower-case image name without .exe, pid)
    pub fn processes() -> Vec<(String, u32)> {
        let out = crate::util::run("tasklist.exe", &["/fo", "csv", "/nh"]).unwrap_or_default();
        out.lines()
            .filter_map(|l| {
                let cols: Vec<&str> = l.split("\",\"").map(|c| c.trim_matches('"')).collect();
                let pid = cols.get(1)?.parse().ok()?;
                Some((cols.first()?.to_lowercase().trim_end_matches(".exe").to_string(), pid))
            })
            .collect()
    }

    pub fn kill(pid: u32) {
        let _ = crate::util::run_status("taskkill.exe", &["/f", "/pid", &pid.to_string()]);
    }

    pub fn hostname() -> String {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use sysinfo::{Disks, ProcessRefreshKind, RefreshKind, System};

    pub fn disks() -> Vec<Disk> {
        Disks::new_with_refreshed_list()
            .list()
            .iter()
            .filter(|d| {
                let m = d.mount_point().to_string_lossy();
                !(m.starts_with("/snap")
                    || m.starts_with("/boot/efi")
                    || m.starts_with("/System/Volumes/VM")
                    || m.starts_with("/System/Volumes/Preboot")
                    || m.starts_with("/System/Volumes/Update"))
            })
            .map(|d| Disk {
                mount: d.mount_point().to_path_buf(),
                name: d.name().to_string_lossy().to_string(),
                total: d.total_space(),
                free: d.available_space(),
                removable: d.is_removable(),
            })
            .collect()
    }

    pub fn memory() -> Memory {
        let s = System::new_with_specifics(RefreshKind::nothing().with_memory(sysinfo::MemoryRefreshKind::everything()));
        Memory { total: s.total_memory(), available: s.available_memory() }
    }

    pub fn uptime_secs() -> u64 {
        System::uptime()
    }

    pub fn cpu_name() -> String {
        let s = System::new_with_specifics(RefreshKind::nothing().with_cpu(sysinfo::CpuRefreshKind::nothing()));
        s.cpus().first().map(|c| c.brand().trim().to_string()).filter(|b| !b.is_empty()).unwrap_or_else(|| "Unknown CPU".into())
    }

    pub fn processes() -> Vec<(String, u32)> {
        let s = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()));
        s.processes().iter().map(|(pid, p)| (p.name().to_string_lossy().to_lowercase(), pid.as_u32())).collect()
    }

    pub fn kill(pid: u32) {
        let s = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()));
        if let Some(p) = s.process(sysinfo::Pid::from_u32(pid)) {
            p.kill();
        }
    }

    pub fn hostname() -> String {
        System::host_name().unwrap_or_default()
    }
}

pub use imp::*;

/// Free bytes on the volume holding `p` (longest matching mount point).
pub fn free_space(p: &std::path::Path) -> u64 {
    let target = p.to_string_lossy().to_lowercase().replace('\\', "/");
    disks()
        .into_iter()
        .filter(|d| target.starts_with(&d.mount.to_string_lossy().to_lowercase().replace('\\', "/")))
        .max_by_key(|d| d.mount.as_os_str().len())
        .map(|d| d.free)
        .unwrap_or(0)
}

/// OS product name and version line, e.g. "Windows 11 Pro 25H2 (26200.6584)".
pub fn os_pretty() -> String {
    #[cfg(windows)]
    {
        use crate::util::reg;
        let cv = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let build: u32 = reg::get_str(cv, "CurrentBuild").and_then(|b| b.parse().ok()).unwrap_or(0);
        let mut name = reg::get_str(cv, "ProductName").unwrap_or_else(|| "Windows".into());
        if build >= 22000 {
            name = name.replace("Windows 10", "Windows 11");
        }
        let ubr = reg::get(cv, "UBR").and_then(|v| v.as_u64()).unwrap_or(0);
        format!("{name} {} ({build}.{ubr})", reg::get_str(cv, "DisplayVersion").unwrap_or_default())
    }
    #[cfg(not(windows))]
    {
        let n = sysinfo::System::long_os_version().unwrap_or_else(|| crate::util::os_name().into());
        let k = sysinfo::System::kernel_version().unwrap_or_default();
        format!("{n} (kernel {k})")
    }
}
