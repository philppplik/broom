# Item reference

Every cleaning item, exactly what it touches, and how it decides. `%P%` = every real user profile on the PC (from `Win32_UserProfile`). Tiers: **Q** Quick · **D** Deep · **N** Nuclear (each tier includes the ones before it).

How size is reported:

- **Counted**: sum of the deleted files (or would-be-deleted files in a dry run).
- **Δ free**: free-space difference on the system drive before/after. Items that use Windows' own tools can only be measured this way.

---

## System junk

| ID | Tier | Touches | Notes |
|---|:-:|---|---|
| `usertemp` | Q | `%P%\AppData\Local\Temp\*` | Locked files skipped. |
| `wintemp` | Q | `C:\Windows\Temp\*` | |
| `wer` | Q | `ProgramData\Microsoft\Windows\WER\{ReportArchive,ReportQueue,Temp}`, `%P%\AppData\Local\Microsoft\Windows\WER` | |
| `dumps` | Q | `C:\Windows\MEMORY.DMP`, `Minidump`, `LiveKernelReports`, `%P%\AppData\Local\CrashDumps` | |
| `logs` | Q | `C:\Windows\Logs\{CBS,DISM,MoSetup,WindowsUpdate,waasmedic,SIH,NetSetup}`, `System32\LogFiles\setupcln`, `SoftwareDistribution\DataStore\Logs`; `*.log/*.etl/*.txt` in `Panther` and `debug`; `C:\Windows\*.log` | Active logs are locked and stay. |
| `recycle` | Q | `X:\$Recycle.Bin\<SID>\*` on every fixed drive (keeps `desktop.ini`), then `Clear-RecycleBin` | |
| `netcache` | Q | `Clear-DnsClientCache`, `ipconfig /flushdns`, `arp -d *`, `nbtstat -R` | Frees no disk space. |
| `thumbs` | D | `thumbcache_*.db`, `iconcache_*.db` in `%P%\AppData\Local\Microsoft\Windows\Explorer`, `%P%\AppData\Local\IconCache.db` | Explorer is restarted. |
| `prefetch` | N | `C:\Windows\Prefetch\*.pf` | |

## Updates & installers

| ID | Tier | Touches | Size | Notes |
|---|:-:|---|:-:|---|
| `wucache` | Q | `C:\Windows\SoftwareDistribution\Download` | counted | `wuauserv` and `bits` are stopped and restarted. |
| `dopt` | Q | `Delete-DeliveryOptimizationCache` | Δ free | |
| `upgrade` | D | `C:\Windows.old`, `$Windows.~BT`, `$Windows.~WS`, `$WinREAgent`, `$GetCurrent`, `$SysReset`, `ESD\Windows`, `ESD\Download` | Δ free | First via cleanmgr handlers, then `icacls /setowner /L` + `rd /s /q` (never follows junctions). |
| `cleanmgr` | D | `cleanmgr /sagerun` with every `VolumeCaches` handler | Δ free | `DownloadsFolder` is always excluded and its flag actively removed. |
| `winsxs` | D | `DISM /Online /Cleanup-Image /StartComponentCleanup /ResetBase` | Δ free | Takes 5–20 min. Installed updates can no longer be uninstalled. |
| `leftovers` | D | `C:\AMD`, `C:\NVIDIA`, `C:\MSOCache` (removed), `C:\Intel\Logs`, NVIDIA Downloader, Edge/Google updater `Download` folders, `C:\Windows\Installer\$PatchCache$`, `%P%\AppData\Local\Downloaded Installations`; for Squirrel apps (`AppData\Local\<app>\Update.exe`) all but the newest `packages\*.nupkg` | counted | `ProgramData\Package Cache` is never touched. |
| `msiorphans` | N | `C:\Windows\Installer\*.msi/*.msp` not referenced by any product or patch | counted | References are collected from **both** the registry (`Installer\UserData\*\Products\*\InstallProperties\LocalPackage`, `Patches\*\LocalPackage`) and the Windows Installer API (`ProductsEx`, `PatchesEx`). If the API fails or fewer than 3 references are found, the item aborts. |
| `olddrivers` | N | Third-party `oem*.inf` from `Get-WindowsDriver -Online`, grouped by INF name + provider + class; all but the newest version are removed with `pnputil /delete-driver` | counted | No `/force`: Windows refuses drivers still in use. |
| `shadows` | N | `Win32_ShadowCopy`: all but the newest per volume | Δ free | |
| `hibernate` | N | `powercfg /hibernate off` | Δ free | Also disables Fast Startup. |
| `reserved` | N | `DISM /Set-ReservedStorageState /State:Disabled` | Δ free | Freed after a restart. Fails while an update is pending. |
| `eventlogs` | N | `wevtutil cl` on every channel | Δ free | |

## Browsers & apps

| ID | Tier | Touches | Notes |
|---|:-:|---|---|
| `browsers` | Q | For each Chromium "User Data" root (Edge/Beta/Dev, Chrome/Beta, Brave, Vivaldi, Chromium, Opera, Opera GX): root `ShaderCache`, `GrShaderCache`, `GraphiteDawnCache`, `component_crx_cache`, `Crashpad\reports`, `BrowserMetrics`; per profile (`Default`, `Profile *`, `Guest Profile`) `Cache`, `Code Cache`, `GPUCache`, `Dawn*Cache`, `Service Worker\{CacheStorage,ScriptCache}`, `Application Cache`, `Media Cache` | Offers to close the browser first. Cookies, logins and history stay. |
| `firefox` | Q | `Profiles\*\{cache2,startupCache,thumbnails,jumpListCache,shader-cache}`, `Crash Reports`, `minidumps`, `saved-telemetry-pings` for Firefox, LibreWolf, Waterfox, Zen | |
| `appcache` | Q | Breadth-first scan (depth 4) of `%P%\AppData\{Roaming,Local}` and `Packages\*\LocalCache` for folders containing `GPUCache`, `Code Cache` or `Default\Cache`; the same cache folders as above are emptied. Plus `CachedData`, `CachedExtensionVSIXs`, `logs` for VS Code, Insiders, Cursor, VSCodium, Windsurf | Browser roots are excluded here. |
| `gpucache` | D | `D3DSCache`, NVIDIA `DXCache/GLCache/NV_Cache`, AMD `DxCache/DxcCache/VkCache/GLCache/OglCache`, Intel `ShaderCache` | |
| `uwp` | D | `%P%\AppData\Local\Packages\*\{AC\INetCache,AC\Temp,TempState}`, `Microsoft\Windows\INetCache` | |
| `devcache` | D | npm, Yarn, pnpm, pip, uv, Poetry, NuGet (`v3-cache`, `http-cache`, `plugins-cache`), `go-build`, `.cargo\registry\cache`, Composer, Electron, electron-builder, Scoop, Chocolatey `lib-bkp/lib-bad` | NuGet `packages` and Gradle are kept. |
| `shortcuts` | D | `.lnk` in all Start Menus and Desktops whose `TargetPath` passes the *missing* test. Advertised MSI shortcuts are skipped. Start-menu folders left empty are removed (except system ones such as `Startup`). | |
| `emptydirs` | D | First-level folders of `Program Files`, `Program Files (x86)`, `Program Files (Arm)`, `ProgramData`, `%P%\AppData\{Local,Roaming,LocalLow,Local\Programs}` that contain **no files at all** and no reparse points (≤ 200 subfolders). A list of system folder names is excluded. | |
| `privacy` | N | `Recent\*.lnk`, `AutomaticDestinations`, `CustomDestinations`; HKU `Explorer\{RecentDocs,RunMRU,TypedPaths,WordWheelQuery,ComDlg32\OpenSavePidlMRU,ComDlg32\LastVisitedPidlMRU*}` | |

## Registry

All registry items work on HKLM, HKLM `WOW6432Node`, and **every loaded user hive** (`HKEY_USERS\S-1-5-21-*`). Each touched key is exported first.

| ID | Tier | Rule |
|---|:-:|---|
| `reguninstall` | D | `Uninstall\*` with an `UninstallString` whose executable is *missing* **and** whose `InstallLocation` is empty or *missing*. Skipped: `WindowsInstaller=1`, `SystemComponent=1`, anything containing `msiexec`. |
| `regapppaths` | D | `App Paths\*` whose default value is *missing*. |
| `regrun` | D | `Run` / `RunOnce` values (native + WOW) whose executable is *missing*, plus the matching `Explorer\StartupApproved\Run(32)` value. |
| `regmui` | D | `MuiCache` values whose program path (after stripping `.FriendlyAppName`, `.ApplicationCompany`, `@…,-N`) is *missing*. |
| `regshared` | D | `SharedDLLs` (native + WOW) and `Installer\Folders` values whose path is *missing*. |
| `regcom` | N | `CLSID\*` (HKLM, WOW, user `_Classes`) where **every** defined `InprocServer32` / `LocalServer32` / `InprocHandler32` is *missing*. Bare file names like `ole32.dll` always count as present. |

### The *missing* test

A path is considered missing only if **all** of these are true:

1. It is an absolute local path (`X:\…`) without unresolved `%VARS%` and not under `\WindowsApps\`.
2. The drive exists (disconnected USB, network or VHD drives are never judged).
3. Neither the path nor any variant exists: `System32` ↔ `SysWOW64` ↔ `SysNative` ↔ `SysArm32`, `Program Files` ↔ `Program Files (x86)` ↔ `Program Files (Arm)`.
4. The nearest existing parent folder **can actually be listed**, so "access denied" is never mistaken for "gone".
