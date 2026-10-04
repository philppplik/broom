# Clean reference (Windows)

Generated from `broom clean --tier nuclear --json`. Tiers: **Quick** ⊂ **Deep** ⊂ **Nuclear**. Every item supports a read-only analysis.

macOS (20 items) and Linux (17 items) catalogs are listed by `broom clean --tier nuclear` on those systems. Exact paths and rules: see `src/clean/`.

## System junk

| ID | Item | Tier | Risk | What it does |
|---|---|---|---|---|
| `usertemp` | User temp files | Quick | Safe | Everything in AppData\Local\Temp of every user profile. Files in use are skipped. |
| `wintemp` | Windows temp files | Quick | Safe | C:\Windows\Temp - leftovers from installers, updates and services. |
| `wer` | Error reports (WER) | Quick | Safe | Windows Error Reporting archives and queues. |
| `dumps` | Crash & memory dumps | Quick | Safe | MEMORY.DMP, minidumps, live kernel reports and app crash dumps. Only needed to debug crashes. |
| `logs` | Windows log files | Quick | Safe | CBS, DISM, setup, upgrade and update logs. Active logs are skipped. |
| `recycle` | Recycle Bin (all drives, all users) | Quick | Moderate | Empties the Recycle Bin on every fixed drive. Deleted files can no longer be restored. |
| `netcache` | DNS, ARP & NetBIOS caches | Quick | Safe | Flushes stale name-resolution caches. Frees no disk space. |
| `thumbs` | Thumbnail & icon cache | Deep | Moderate | Rebuilt automatically. Explorer restarts for a second. |
| `fontcache` | Font cache | Nuclear | Moderate | Rebuilt on next boot. Fixes garbled fonts. |
| `prefetch` | Prefetch data | Nuclear | Moderate | App launch traces. Rebuilt automatically; apps start a bit slower once. |

## Updates & installers

| ID | Item | Tier | Risk | What it does |
|---|---|---|---|---|
| `wucache` | Windows Update download cache | Quick | Safe | Already-installed update packages. Update services pause briefly. |
| `dopt` | Delivery Optimization cache | Quick | Safe | Update pieces Windows keeps to share with other PCs. |
| `upgrade` | Old Windows installations (Windows.old...) | Deep | Moderate | Windows.old, $Windows.~BT/~WS, $WinREAgent, ESD files. You lose the option to roll back to the previous Windows version. |
| `cleanmgr` | Windows Disk Cleanup (every category) | Deep | Moderate | Runs the built-in Disk Cleanup silently with all categories. Your Downloads are always excluded. |
| `winsxs` | Component store cleanup (WinSxS /ResetBase) | Deep | Moderate | Removes superseded system components via DISM. 5-20 min. Installed updates can no longer be uninstalled. |
| `leftovers` | Installer & updater leftovers | Deep | Moderate | C:\AMD, C:\NVIDIA, MSOCache, Edge/Google updater downloads, MSI patch cache, old Squirrel packages (Discord, Slack...). |
| `msiorphans` | Orphaned Windows Installer packages | Nuclear | Aggressive | Unreferenced .msi/.msp in C:\Windows\Installer, cross-checked against the registry AND the Windows Installer API. |
| `olddrivers` | Old driver versions (DriverStore) | Nuclear | Aggressive | Removes superseded third-party driver packages, keeps the newest. Drivers in use are never removed. |
| `shadows` | Old restore points & shadow copies | Nuclear | Aggressive | Deletes every restore point except the newest per drive. |
| `hibernate` | Hibernation file (hiberfil.sys) | Nuclear | Aggressive | Turns hibernation off (also disables Fast Startup). Re-enable: powercfg /h on |
| `reserved` | Reserved storage | Nuclear | Aggressive | Space Windows reserves for updates (~7 GB). Freed after a restart. |
| `eventlogs` | Event logs (all channels) | Nuclear | Aggressive | Clears every Windows event log. Removes troubleshooting history. |

## Browsers & apps

| ID | Item | Tier | Risk | What it does |
|---|---|---|---|---|
| `gpucache` | GPU shader caches (DirectX, NVIDIA, AMD, Intel, Qualcomm) | Deep | Safe | Compiled shader caches. Rebuilt automatically; games may stutter briefly once. |
| `uwp` | Store app temp & web caches | Deep | Safe | TempState, AC\Temp and AC\INetCache of Store apps plus the legacy IE cache. |
| `shortcuts` | Broken shortcuts (Start menu & Desktop) | Deep | Safe | .lnk files whose target no longer exists. |
| `emptydirs` | Empty leftover folders of uninstalled apps | Deep | Moderate | Top-level folders in Program Files, ProgramData and AppData that contain no files at all. |
| `browsers` | Browser caches (Edge, Chrome, Brave, Opera, Vivaldi, Arc) | Quick | Safe | Web, code, GPU and shader caches of every browser profile. Passwords, cookies, history and logins are NOT touched. |
| `firefox` | Firefox-family caches (Firefox, LibreWolf, Zen...) | Quick | Safe | cache2, startup cache, thumbnails and crash reports. Passwords, cookies and history stay. |
| `appcache` | App caches (Discord, Teams, Slack, VS Code, Steam...) | Quick | Safe | Auto-detects every Electron / Chromium / WebView2 app and empties only its web, code and GPU caches. App data and logins stay. |

## Developer

| ID | Item | Tier | Risk | What it does |
|---|---|---|---|---|
| `devcache` | Package manager caches | Deep | Moderate | npm, Yarn, pnpm, Bun, pip, uv, Poetry, NuGet, Go, Cargo, Gradle, Maven, Composer, Homebrew, CocoaPods, Scoop, Chocolatey. Re-downloaded when needed. |
| `artifacts` | Stale build output in your projects | Nuclear | Moderate | node_modules, target, build, .next, __pycache__, obj/bin... in project folders (source, repos, GitHub, Developer...) untouched for 30+ days, only next to a matching project file. Rebuilt by the next build. |
| `containers` | Docker / Podman build cache | Nuclear | Moderate | Runs `builder prune` and `image prune` (dangling images only). Containers, volumes and tagged images stay. |
| `aicache` | AI download leftovers (Hugging Face, Torch) | Deep | Safe | Incomplete downloads, lock files and transfer caches of local AI tools. Downloaded models are never removed. |

## Registry

| ID | Item | Tier | Risk | What it does |
|---|---|---|---|---|
| `reguninstall` | Ghost entries in "Installed apps" | Deep | Moderate | Uninstall entries whose uninstaller AND install folder are gone. MSI entries are never touched. |
| `regapppaths` | Dead "App Paths" registrations | Deep | Safe | App Paths that point to programs that no longer exist. |
| `regrun` | Dead startup entries (Run / RunOnce) | Deep | Moderate | Autostart entries whose program was uninstalled, plus their Task Manager toggle. |
| `regmui` | MUI cache of deleted programs | Deep | Safe | Cached display names of programs that no longer exist. |
| `regshared` | Missing shared DLL & installer folder references | Deep | Moderate | SharedDLLs and Installer\Folders entries for files/folders that are gone. |
| `regcom` | Orphaned COM / ActiveX registrations | Nuclear | Aggressive | CLSID entries whose every server DLL/EXE is gone. Only absolute paths on present, readable drives are judged. |

## Privacy

| ID | Item | Tier | Risk | What it does |
|---|---|---|---|---|
| `privacy` | Recent files, jump lists & Explorer history | Nuclear | Moderate | Recent items, jump lists, Run box / address bar / search history, Open-Save dialog history. |

