# Doctor reference

The Doctor collects hardware facts once (Windows: a single batched CIM/PowerShell query plus direct registry reads;
macOS: `system_profiler`, `fdesetup`, `csrutil`…; Linux: `/proc`, `/sys`, `systemctl`, `journalctl`, `lsblk`, `smartctl`),
then turns them into findings. Each finding has a severity and optionally a **fix**: a tweak, a repair action, or a
pointer to the right Broom tab.

**Health score** = 100 − 15 per critical − 6 per warning − 2 per advice.

## Hardware it understands

Manufacturer and model · laptop vs. desktop (PC system type, chassis, battery) · CPU and threads · RAM total/free,
rated vs. configured speed, used/total slots · GPUs and vendor (NVIDIA, AMD, Intel, Qualcomm, Apple) · physical disks
with media (NVMe/SSD/HDD), health, temperature and wear · which disk holds the OS · battery design vs. full capacity
and cycles · firmware (UEFI/legacy) · uptime.

## Checks (Windows)

| Area | Check | Fix offered |
|---|---|---|
| Storage | Free space per volume (< 12 % warning, < 5 % critical) | Clean tab |
| Storage | Disk health not "Healthy", SSD wear ≥ 80 %, drive temperature ≥ 65 °C | — |
| Storage | Disk / NTFS errors in the System log (30 days) | Online `chkdsk /scan` |
| Storage | Windows on an HDD | advice: SSD upgrade |
| Storage | TRIM disabled on SSD | enable TRIM |
| Stability | Pending restart (CBS, Windows Update, pending file renames) | — |
| Stability | Blue screens (BugCheck 1001) in 30 days | Memory test |
| Stability | Unexpected shutdowns (Kernel-Power 41) | Reliability Monitor |
| Stability | App crashes in 7 days, with the top offenders | Uninstall tab |
| Stability | Long uptime with Fast Startup on | advice |
| Drivers | Devices with a ConfigManager error | Device Manager |
| Drivers | GPU driver older than ~13 months | advice |
| System files | Component store corruption (`DISM /CheckHealth`, admin) | `DISM /RestoreHealth` |
| Updates | Last installed update > 45 days | Reset Windows Update |
| Windows | Not activated | — |
| Security | No real-time antivirus / stale definitions | — |
| Security | Firewall profile off · UAC off · SMBv1 on · RDP on | disable SMBv1 |
| Security | Auto-logon password stored in plain text | — |
| Security | Secure Boot off · no TPM · laptop without BitLocker | — |
| Recovery | No restore points | enable System Protection + create one |
| Performance | NVIDIA/AMD GPU with HAGS off · Game DVR on | tweaks `hags-on`, `game-dvr-off` |
| Performance | Memory Integrity on a gaming desktop | advice (security trade-off) |
| Power | Laptop on High/Ultimate performance · desktop on Power saver | Balanced plan |
| Memory | < 8 GB RAM · > 90 % in use · RAM below rated speed (XMP/EXPO off) · single channel · manual page file | startup manager / page file |
| Startup | More than 10 startup apps | Startup view |
| Network | Your DNS > 2× slower than Cloudflare and > 40 ms | DNS view |

macOS adds FileVault, SIP, Gatekeeper, firewall, Time Machine, kernel panics, memory pressure and launch agents.
Linux adds failed systemd units, reboot-required, journal errors, OOM kills, swap/zram, fstrim.timer, swappiness,
firewall, Secure Boot, NTP sync, pending updates and CPU governor.

## Repair toolbox (Windows)

SFC · DISM RestoreHealth · online chkdsk · network stack reset · Windows Update reset · icon/thumbnail cache rebuild ·
restart Explorer · restart audio · rebuild Search index · clock sync · Store cache reset · clear print queue ·
System Protection + restore point · battery report · Balanced / High performance plan · TRIM · optimize drives ·
automatic page file · disable SMBv1 · restart graphics driver · memory test · Device Manager · Reliability Monitor.

macOS: flush DNS, rebuild Spotlight, verify disk, periodic maintenance, restart Dock/Finder, rebuild "Open With".
Linux: reset failed units, fstrim, flush DNS, enable NTP, rebuild font cache, repair interrupted dpkg installs.
