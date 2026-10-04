# Tweak reference (Windows)

Generated from `broom tweaks list --json`.

**Undo** column: **Exact** = previous values are journaled and restored precisely · **Script** = an undo script restores the default · **None** = one-way action (Broom warns in red before applying).

**Source**: Broom = Broom's own catalog · winutil = embedded from [winutil](https://github.com/ChrisTitusTech/winutil) (MIT).

## Privacy

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `ads-id-off` | Advertising ID: off | Safe | Exact | Broom | Stops apps from using your advertising ID for personalized ads. |
| `tailored-off` | Tailored experiences: off | Safe | Exact | Broom | Microsoft stops using diagnostic data for personalized tips, ads and recommendations. |
| `suggestions-off` | Start, Settings & lock screen suggestions: off | Safe | Exact | Broom | No more 'suggested' apps, tips, fun facts and silently auto-installed promo apps. |
| `feedback-off` | Feedback requests: never | Safe | Exact | Broom | Windows stops asking for feedback. |
| `inking-off` | Inking & typing personalization: off | Safe | Exact | Broom | Stops collecting typed and handwritten text to build a personal dictionary in the cloud. |
| `speech-off` | Online speech recognition: off | Safe | Exact | Broom | Voice input is no longer sent to Microsoft's cloud. |
| `language-list-off` | Websites reading your language list: off | Safe | Exact | Broom | Websites can no longer read your installed language list for fingerprinting. |
| `copilot-off` | Copilot: off | Moderate | Exact | Broom | Turns off Windows Copilot via policy and hides its taskbar button. |
| `recall-off` | Recall snapshots: off | Safe | Exact | Broom | Copilot+ PCs: Windows stops taking and analysing screenshots of your activity. |
| `clipboard-cloud-off` | Cloud clipboard sync: off | Safe | Exact | Broom | Clipboard history stays on this PC instead of syncing across devices. |
| `wer-off` | Error reporting to Microsoft: off | Moderate | Exact | Broom | Crash reports are no longer sent. You lose Microsoft's automatic fixes for known crashes. |
| `find-my-device-off` | Find My Device: off | Moderate | Exact | Broom | Stops periodically sending this device's location to your Microsoft account. |

## Performance

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `menu-delay` | Instant menus | Safe | Exact | Broom | Removes the 400 ms delay before submenus open. |
| `startup-delay-off` | No startup delay for autostart apps | Safe | Exact | Broom | Windows normally waits ~10 s before launching startup apps. Starts them right away. |
| `fast-shutdown` | Faster shutdown | Moderate | Exact | Broom | Hung apps are closed after 2 s instead of 5-20 s at shutdown. Unsaved work in frozen apps is lost. |
| `last-access-off` | NTFS last-access timestamps: off | Safe | Script | Broom | Saves a disk write on every file read. Default on large modern disks anyway. |
| `transparency-off` | Transparency effects: off | Safe | Exact | Broom | Less GPU work for the taskbar, Start and window backgrounds. |
| `animations-off` | Window animations: off | Safe | Exact | Broom | Windows open, minimise and maximise instantly. |
| `sysmain-off` | SysMain (Superfetch): off | Aggressive | Exact | Broom | Stops RAM prefetching. Can reduce disk activity on SSDs; slows app starts on HDDs. |
| `search-index-off` | Windows Search indexing: off | Aggressive | Exact | Broom | Stops the background indexer. Start/Explorer search becomes slower and less complete. |

## Gaming

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `game-mode-on` | Game Mode: on | Safe | Exact | Broom | Windows prioritises the game and pauses updates/notifications while playing. |
| `game-dvr-off` | Background game recording (Game DVR): off | Safe | Exact | Broom | Stops constant background capture that costs FPS. Game Bar screenshots still work. |
| `hags-on` | Hardware-accelerated GPU scheduling: on | Moderate | Exact | Broom | The GPU manages its own memory: lower latency on recent NVIDIA/AMD GPUs. Restart required. |
| `mmcss-games` | Multimedia scheduler: favour games | Moderate | Exact | Broom | Gives foreground games higher CPU/GPU priority and stops network throttling during playback. |
| `mouse-precision-off` | Mouse acceleration (Enhance pointer precision): off | Safe | Exact | Broom | 1:1 mouse movement - what most games and players expect. |
| `fso-off` | Fullscreen optimizations: off (global) | Aggressive | Exact | Broom | Forces classic exclusive fullscreen. Helps some older games; can break Alt-Tab/HDR in newer ones. |

## Explorer & UI

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `explorer-this-pc` | Explorer opens 'This PC' | Safe | Exact | Broom | File Explorer starts in This PC instead of Home. |
| `quick-access-private` | No recent/frequent files in Explorer Home | Safe | Exact | Broom | Explorer stops listing recently opened files and folders. |
| `compact-mode` | Explorer compact view | Safe | Exact | Broom | Less padding between items - more files on screen. |
| `taskview-button-off` | Task View button: hidden | Safe | Exact | Broom | Removes the Task View button from the taskbar (Win+Tab still works). |
| `snap-flyout-off` | Snap layout pop-up on maximise button: off | Safe | Exact | Broom | No layout flyout when hovering the maximise button. Win+Z still works. |
| `full-path-title` | Full path in Explorer title bar | Safe | Exact | Broom | Shows the complete folder path in the window title. |
| `long-paths-on` | Long file paths (>260 chars): on | Safe | Exact | Broom | Lets apps that support it use paths longer than 260 characters. Great for developers. |

## Services

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `svc-fax` | Fax service: manual | Safe | Exact | Broom | Only needed if you send faxes from this PC. |
| `svc-retaildemo` | Retail Demo service: disabled | Safe | Exact | Broom | Store display-PC mode. Never needed at home. |
| `svc-maps` | Downloaded Maps Manager: manual | Safe | Exact | Broom | Only used by the Maps app's offline maps. |
| `svc-diagtrack` | Connected User Experiences and Telemetry: disabled | Moderate | Exact | Broom | The main telemetry service. |
| `svc-wap` | WAP Push message routing: disabled | Safe | Exact | Broom | Telemetry helper for device management messages. |
| `svc-xbox` | Xbox Live services: manual | Moderate | Exact | Broom | Xbox sign-in and cloud saves start only when a game needs them. Controllers are unaffected. |
| `svc-remote-registry` | Remote Registry: disabled | Safe | Exact | Broom | Nobody on the network can edit this PC's registry. |
| `svc-insider` | Windows Insider service: manual | Safe | Exact | Broom | Only needed for Insider builds. |

## Network

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `delivery-p2p-lan` | Update sharing: local network only | Safe | Exact | Broom | Windows Update no longer uploads update pieces to strangers on the internet. |
| `llmnr-off` | LLMNR name resolution: off | Moderate | Exact | Broom | Closes a legacy protocol often abused for credential theft on public networks. |
| `wifi-sense-off` | Auto-connect to suggested open hotspots: off | Safe | Exact | Broom | Windows stops automatically joining open hotspots it 'recommends'. |

## Power

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `ultimate-plan` | Ultimate Performance power plan | Moderate | Script | Broom | Adds and activates Microsoft's hidden highest-performance plan. Not for laptops on battery. |
| `power-throttling-off` | Power throttling: off | Moderate | Exact | Broom | Background apps keep full CPU speed. More performance, less battery life. |
| `usb-suspend-off` | USB selective suspend: off | Moderate | Script | Broom | Stops Windows from powering down USB devices - fixes disconnecting mice, DACs and hubs. |

## Essential (winutil)

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `WPFTweaksActivity` | Activity History: off | Safe | Exact | winutil | Stops Windows from publishing or uploading user activities while preserving clipboard history. |
| `WPFTweaksConsumerFeatures` | ConsumerFeatures: off | Safe | Exact | winutil | Stops promoted app installs and reduces app suggestions from Microsoft Store content. |
| `WPFTweaksDeleteTempFiles` | Temporary Files: remove | Safe | None | winutil | Erases TEMP Folders. |
| `WPFTweaksDeliveryOptimization` | Delivery Optimization: off | Safe | Exact | winutil | Stops Windows from using your bandwidth to upload updates to other PCs on the internet or local network. |
| `WPFTweaksDisableBitLocker` | BitLocker: off | Safe | Script | winutil | Disables BitLocker. |
| `WPFTweaksDisableExplorerAutoDiscovery` | File Explorer Automatic Folder Discovery: off | Safe | Script | winutil | Windows Explorer automatically tries to guess the type of the folder based on its contents, slowing down the browsing experience. WARNING! Will disable File Explorer grouping. |
| `WPFTweaksDisableStoreSearch` | Microsoft Store Recommended Search Results: off | Safe | Script | winutil | Will not display recommended Microsoft Store apps when searching for apps in the Start menu. |
| `WPFTweaksDiskCleanup` | Disk Cleanup - Run | Safe | None | winutil | Runs Disk Cleanup on Drive C: and removes old Windows Updates. |
| `WPFTweaksEndTaskOnTaskbar` | End Task With Right Click: on | Safe | Exact | winutil | Enables option to end task when right-clicking a program in the taskbar. |
| `WPFTweaksHiber` | Hibernation: off | Safe | Script | winutil | Hibernation is really meant for laptops as it saves what's in memory before turning the PC off. It really should never be used. |
| `WPFTweaksLocation` | Location Tracking: off | Safe | Exact | winutil | Disables Location Tracking. |
| `WPFTweaksPreventDeviceMetadataFromNetwork` | Prevent Device Companion Apps | Safe | Exact | winutil | Prevents additional software from being installed when plugging in devices (e.g. Ads when plugging in a monitor). Poses potential security risk. |
| `WPFTweaksRestorePoint` | Restore Point - Create | Safe | Script | winutil | Creates a restore point at runtime in case a revert is needed from WinUtil modifications. |
| `WPFTweaksRevertStartMenu` | Start Menu Previous Layout: on | Safe | Exact | winutil | Bring back the old Start Menu layout from before the gradual rollout of the new one in 25H2. On newer versions of Windows !!THIS TWEAK WILL NOT WORK!! |
| `WPFTweaksServices` | Services - Set to Manual | Safe | Script | winutil | Sets some services to Manual startup and adjusts the SvcHostSplitThresholdInKB registry value to better match system memory, which can significantly reduce the number of svchost.exe processes. |
| `WPFTweaksTelemetry` | Telemetry: off | Safe | Script | winutil | Disables Microsoft Telemetry. |
| `WPFTweaksWPBT` | Windows Platform Binary Table (WPBT): off | Safe | Exact | winutil | If enabled, WPBT allows your computer vendor to execute programs at boot time, such as anti-theft software, software drivers, as well as force install software without user consent. Poses potential security risk. |
| `WPFTweaksWidget` | Widgets: remove | Safe | None | winutil | Removes the annoying widgets in the bottom left of the Taskbar. |

## Preferences (winutil)

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `WPFToggleBatteryPercentage` | System Tray Battery Percentage | Safe | Exact | winutil | Shows numeric battery percentage next to the battery icon in the system tray. |
| `WPFToggleBingSearch` | Start Menu Bing Search | Safe | Exact | winutil | Toggles Bing web search results in Windows Search. |
| `WPFToggleDarkMode` | Dark Theme for Windows | Safe | Script | winutil | Dark Mode for the system and applications. |
| `WPFToggleDetailedBSoD` | BSoD Verbose Mode | Safe | Exact | winutil | Gives more information when you blue screen. |
| `WPFToggleDisableLockscreen` | Lock Screen: off | Safe | Exact | winutil | Skips the lock screen entirely and goes directly to the sign-in screen on boot and wake. |
| `WPFToggleGameMode` | Game Mode | Safe | Exact | winutil | Toggles Windows prioritizes gaming performance by allocating system resources to games. |
| `WPFToggleHiddenFiles` | File Explorer Hidden Files | Safe | Script | winutil | Reveals hidden files in Explorer. |
| `WPFToggleHideSettingsHome` | Settings Home Page | Safe | Exact | winutil | Toggles the Home Page in the Windows Settings app. |
| `WPFToggleLoginBlur` | Logon Screen Acrylic Blur | Safe | Exact | winutil | Toggles the acrylic blur effect on login screen background. |
| `WPFToggleLongPaths` | Enable Long Paths | Safe | Exact | winutil | Toggles support for file paths longer than 260 characters in Explorer. |
| `WPFToggleMouseAcceleration` | Mouse Acceleration | Safe | Exact | winutil | Makes it so Cursor movement is affected by the speed of your physical mouse movements. |
| `WPFToggleNewOutlook` | Microsoft Outlook New Version | Safe | Exact | winutil | This will ensure the new Outlook application is used. |
| `WPFToggleNumLock` | Num Lock on Startup | Safe | Exact | winutil | Toggle the Num Lock key state when your computer starts. |
| `WPFToggleS3Sleep` | S3 Sleep | Safe | Exact | winutil | Toggles between Modern Standby and S3 Sleep, which cuts off power to the CPU while continuing to refresh the memory. |
| `WPFToggleScrollbars` | Scrollbars Always Visible | Safe | Exact | winutil | If enabled, scrollbars will always be visible. If disabled, Windows will automatically hide scrollbars when not in use. |
| `WPFToggleShowExt` | File Explorer File Extensions | Safe | Script | winutil | Shows .file extensions in Explorer (.exe, .png, etc.) |
| `WPFToggleStandbyFix` | S0 Sleep Network Connectivity | Safe | Exact | winutil | Toggles network connectivity during S0 Sleep which is low power idle in modern laptops. |
| `WPFToggleStartMenuRecommendations` | Start Menu Recommendations | Safe | Script | winutil | Toggles the recommendations section in the Start Menu. WARNING: This will also disable Windows Spotlight on your Lock Screen as a side effect. |
| `WPFToggleStickyKeys` | Sticky Keys | Safe | Exact | winutil | Toggles the Sticky Keys, which activate when clicking shift rapidly. |
| `WPFToggleTaskView` | Taskbar Task View Icon | Safe | Exact | winutil | Toggles the Task View Button in the Taskbar. |
| `WPFToggleTaskbarAlignment` | Taskbar Centered Icons | Safe | Script | winutil | Toggles the Taskbar alignment either to the left or center. |
| `WPFToggleTaskbarSearch` | Taskbar Search Icon | Safe | Exact | winutil | Toggles the Search Button on the Taskbar. |
| `WPFToggleVerboseLogon` | Logon Verbose Mode | Safe | Exact | winutil | Show detailed messages during startup/shutdown. |
| `WPFToggleWindowSnapping` | Window Snapping | Safe | Exact | winutil | Toggles the window snapping feature when dragging windows. |

## Advanced (winutil)

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `WPFTweaksBlockAdobeNet` | Adobe URL Block List: on | Aggressive | Script | winutil | Reduces user interruptions by selectively blocking connections to Adobe's activation and telemetry servers. Credit: Ruddernation-Designs |
| `WPFTweaksBraveDebloat` | Brave Browser - Debloat | Aggressive | Exact | winutil | Disables various annoyances like Brave Rewards, Leo AI, Crypto Wallet and VPN. |
| `WPFTweaksDisableBGapps` | Background Apps: off | Aggressive | Exact | winutil | Disables all Microsoft Store apps from running in the background, which has to be done individually since Windows 11. |
| `WPFTweaksDisableIPv6` | IPv6: off | Aggressive | Script | winutil | Disables IPv6. |
| `WPFTweaksDisableNotifications` | System Tray Notifications & Calendar: off | Aggressive | Exact | winutil | Disables all Notifications INCLUDING Calendar. |
| `WPFTweaksDisableWarningForUnsignedRdp` | RDP Unsigned File Warnings: off | Aggressive | Exact | winutil | Disables warnings shown when launching unsigned RDP files introduced with the latest Windows 10 and 11 updates. |
| `WPFTweaksDisplay` | Visual Effects - Set to Best Performance | Aggressive | Script | winutil | Sets the system preferences to performance. You can do this manually with sysdm.cpl as well. |
| `WPFTweaksEdgeDebloat` | Microsoft Edge - Debloat | Aggressive | Exact | winutil | Disables various telemetry options, popups, and other annoyances in Edge. |
| `WPFTweaksIPv46` | IPv6 - Set IPv4 as Preferred | Aggressive | Exact | winutil | Setting the IPv4 preference can have latency and security benefits on private networks where IPv6 is not configured. |
| `WPFTweaksLogiBlock` | Logitech Download Assistant Auto-Install: off | Aggressive | Script | winutil | Blocks the Logi Download Assistant that Windows Update keeps reinstalling with Logitech device drivers. Logitech hardware keeps working without it. |
| `WPFTweaksRazerBlock` | Razer Software Auto-Install: off | Aggressive | Script | winutil | Blocks ALL Razer Software installations. The hardware works fine without any software. |
| `WPFTweaksRemoveEdge` | Microsoft Edge: remove | Aggressive | Script | winutil | Uninstalls Microsoft Edge by creating dummy MicrosoftEdge.exe file in the legacy Edge folder. This tricks Windows into unlocking the official Edge uninstaller allowing for a system-level removal. |
| `WPFTweaksRemoveHomeAndGallery` | File Explorer Home and Gallery: off | Aggressive | Exact | winutil | Removes the Home and Gallery from Explorer and sets This PC as default. |
| `WPFTweaksRemoveOneDrive` | Microsoft OneDrive: remove | Aggressive | Script | winutil | Denies permission to remove OneDrive user files, then uses its own uninstaller to remove it and restores the original permission afterward. |
| `WPFTweaksReservedStorage` | Disable Reserved Storage | Aggressive | Script | winutil | Disables Windows Reserved Storage (7-10 GB held for updates/temp files). Recommended only on small drives. Re-enable before major Windows feature updates to avoid installation failures. |
| `WPFTweaksRightClickMenu` | Right-Click Menu Previous Layout: on | Aggressive | Script | winutil | Restores the classic context menu when right-clicking in File Explorer, replacing the simplified Windows 11 version. |
| `WPFTweaksStorage` | Storage Sense: off | Aggressive | Exact | winutil | Storage Sense deletes temp files automatically. |
| `WPFTweaksTeredo` | Teredo: off | Aggressive | Script | winutil | Teredo network tunneling is an IPv6 feature that can cause additional latency, but may cause problems with some games. |
| `WPFTweaksUTC` | Date & Time - Set Time to UTC | Aggressive | Exact | winutil | Essential for computers that are dual booting. Fixes the time sync with Linux systems. |
| `WPFTweaksWindowsAI` | Windows AI: off And Remove | Aggressive | Script | winutil | Removes and disables all AI features/packages |

## Windows features

| ID | Tweak | Risk | Undo | Source | What it does |
|---|---|---|---|---|---|
| `WPFFeatureDisableLegacyRecovery` | Legacy F8 Boot Recovery - Disable | Moderate | None | winutil | Disables Advanced Boot Options screen that lets you start Windows in advanced troubleshooting modes. |
| `WPFFeatureEnableLegacyRecovery` | Legacy F8 Boot Recovery | Moderate | None | winutil | Enables Advanced Boot Options screen that lets you start Windows in advanced troubleshooting modes. |
| `WPFFeatureRegBackup` | Registry Backup (Daily Task 12:30am) | Moderate | None | winutil | Enables daily registry backup, previously disabled by Microsoft in Windows 10 1803. |
| `WPFFeaturenfs` | Network File System (NFS) | Moderate | Script | winutil | Network File System (NFS) is a mechanism for storing files on a network. |
| `WPFFeaturesSandbox` | Windows Sandbox | Moderate | Script | winutil | Windows Sandbox is a lightweight virtual machine that provides a temporary desktop environment to safely run applications and programs in isolation. |
| `WPFFeaturesdotnet` | .NET Framework (Versions 2, 3, 4) | Moderate | Script | winutil | .NET and .NET Framework is a developer platform made up of tools, programming languages, and libraries for building many different types of applications. |
| `WPFFeatureshyperv` | Hyper-V | Moderate | Script | winutil | Hyper-V is a hardware virtualization product developed by Microsoft that allows users to create and manage virtual machines. |
| `WPFFeatureslegacymedia` | Legacy Media Components (WMP, DirectPlay) | Moderate | Script | winutil | Enables legacy programs from previous versions of Windows. |
| `WPFFeaturewsl` | Windows Subsystem for Linux (WSL) | Moderate | Script | winutil | Windows Subsystem for Linux is an optional feature of Windows that allows Linux programs to run natively on Windows without the need for a separate virtual machine or dual booting. |

## macOS and Linux

macOS: 22 `defaults` tweaks (Finder, Dock, keyboard, screenshots, privacy). Linux: 12 tweaks (swappiness, inotify, fstrim.timer, journald size, GNOME settings, privacy). Run `broom tweaks list` there for the full list.
