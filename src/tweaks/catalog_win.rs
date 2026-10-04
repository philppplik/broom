//! Broom's own Windows tweaks. Written independently for Broom; the feature ideas are inspired by
//! Optimizer (hellzerg), Sparkle (thedogecraft), ReviOS Playbook (meetrevision) and Slate.
//! Registry locations are documented Windows settings / group policies.

use super::{Op, Tweak};
use crate::util::reg::Val;
use crate::util::Risk;

fn d(path: &str, name: &str, v: u32) -> Op {
    Op::RegSet { path: path.into(), name: name.into(), val: Val::Dword(v) }
}
fn s(path: &str, name: &str, v: &str) -> Op {
    Op::RegSet { path: path.into(), name: name.into(), val: Val::Sz(v.into()) }
}
fn del(path: &str, name: &str) -> Op {
    Op::RegDel { path: path.into(), name: name.into() }
}
fn svc(name: &str, start: &str) -> Op {
    Op::Service { name: name.into(), start: start.into() }
}
fn ps(script: &str) -> Op {
    Op::Ps(script.into())
}

struct T(&'static str, &'static str, &'static str, Risk, &'static str, Vec<Op>, Vec<Op>, bool);

const CDM: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
const ADV: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const MM: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
const GAMES: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";

fn list() -> Vec<T> {
    use Risk::*;
    let mut v = vec![
        // ------------------------------------------------------------ Privacy
        T("ads-id-off", "Privacy", "Advertising ID: off", Safe,
          "Stops apps from using your advertising ID for personalized ads.",
          vec![d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", 0),
               d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo", "DisabledByGroupPolicy", 1)], vec![], false),
        T("tailored-off", "Privacy", "Tailored experiences: off", Safe,
          "Microsoft stops using diagnostic data for personalized tips, ads and recommendations.",
          vec![d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Privacy", "TailoredExperiencesWithDiagnosticDataEnabled", 0),
               d(r"HKCU\Software\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData", 1)], vec![], false),
        T("suggestions-off", "Privacy", "Start, Settings & lock screen suggestions: off", Safe,
          "No more 'suggested' apps, tips, fun facts and silently auto-installed promo apps.",
          vec![d(CDM, "SubscribedContent-338388Enabled", 0), d(CDM, "SubscribedContent-338389Enabled", 0),
               d(CDM, "SubscribedContent-338393Enabled", 0), d(CDM, "SubscribedContent-353694Enabled", 0),
               d(CDM, "SubscribedContent-353696Enabled", 0), d(CDM, "SubscribedContent-338387Enabled", 0),
               d(CDM, "RotatingLockScreenOverlayEnabled", 0), d(CDM, "SystemPaneSuggestionsEnabled", 0),
               d(CDM, "SoftLandingEnabled", 0), d(CDM, "SilentInstalledAppsEnabled", 0),
               d(CDM, "PreInstalledAppsEnabled", 0), d(CDM, "OemPreInstalledAppsEnabled", 0),
               d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement", "ScoobeSystemSettingEnabled", 0)], vec![], false),
        T("feedback-off", "Privacy", "Feedback requests: never", Safe,
          "Windows stops asking for feedback.",
          vec![d(r"HKCU\Software\Microsoft\Siuf\Rules", "NumberOfSIUFInPeriod", 0),
               d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DoNotShowFeedbackNotifications", 1)], vec![], false),
        T("inking-off", "Privacy", "Inking & typing personalization: off", Safe,
          "Stops collecting typed and handwritten text to build a personal dictionary in the cloud.",
          vec![d(r"HKCU\Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", 1),
               d(r"HKCU\Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", 1),
               d(r"HKCU\Software\Microsoft\InputPersonalization\TrainedDataStore", "HarvestContacts", 0),
               d(r"HKCU\Software\Microsoft\Personalization\Settings", "AcceptedPrivacyPolicy", 0)], vec![], false),
        T("speech-off", "Privacy", "Online speech recognition: off", Safe,
          "Voice input is no longer sent to Microsoft's cloud.",
          vec![d(r"HKCU\Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy", "HasAccepted", 0)], vec![], false),
        T("language-list-off", "Privacy", "Websites reading your language list: off", Safe,
          "Websites can no longer read your installed language list for fingerprinting.",
          vec![d(r"HKCU\Control Panel\International\User Profile", "HttpAcceptLanguageOptOut", 1)], vec![], false),
        T("copilot-off", "Privacy", "Copilot: off", Moderate,
          "Turns off Windows Copilot via policy and hides its taskbar button.",
          vec![d(r"HKCU\Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", 1),
               d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", 1),
               d(ADV, "ShowCopilotButton", 0)], vec![], true),
        T("recall-off", "Privacy", "Recall snapshots: off", Safe,
          "Copilot+ PCs: Windows stops taking and analysing screenshots of your activity.",
          vec![d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis", 1),
               d(r"HKCU\Software\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis", 1),
               d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement", 0)], vec![], true),
        T("clipboard-cloud-off", "Privacy", "Cloud clipboard sync: off", Safe,
          "Clipboard history stays on this PC instead of syncing across devices.",
          vec![d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System", "AllowCrossDeviceClipboard", 0)], vec![], false),
        T("wer-off", "Privacy", "Error reporting to Microsoft: off", Moderate,
          "Crash reports are no longer sent. You lose Microsoft's automatic fixes for known crashes.",
          vec![d(r"HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting", "Disabled", 1)], vec![], false),
        T("find-my-device-off", "Privacy", "Find My Device: off", Moderate,
          "Stops periodically sending this device's location to your Microsoft account.",
          vec![d(r"HKLM\SOFTWARE\Policies\Microsoft\FindMyDevice", "AllowFindMyDevice", 0)], vec![], false),

        // ------------------------------------------------------------ Performance
        T("menu-delay", "Performance", "Instant menus", Safe,
          "Removes the 400 ms delay before submenus open.",
          vec![s(r"HKCU\Control Panel\Desktop", "MenuShowDelay", "0")], vec![], false),
        T("startup-delay-off", "Performance", "No startup delay for autostart apps", Safe,
          "Windows normally waits ~10 s before launching startup apps. Starts them right away.",
          vec![d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", "StartupDelayInMSec", 0),
               d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", "WaitForIdleState", 0)], vec![], false),
        T("fast-shutdown", "Performance", "Faster shutdown", Moderate,
          "Hung apps are closed after 2 s instead of 5-20 s at shutdown. Unsaved work in frozen apps is lost.",
          vec![s(r"HKCU\Control Panel\Desktop", "AutoEndTasks", "1"), s(r"HKCU\Control Panel\Desktop", "HungAppTimeout", "2000"),
               s(r"HKCU\Control Panel\Desktop", "WaitToKillAppTimeout", "2000"),
               s(r"HKLM\SYSTEM\CurrentControlSet\Control", "WaitToKillServiceTimeout", "2000")], vec![], false),
        T("last-access-off", "Performance", "NTFS last-access timestamps: off", Safe,
          "Saves a disk write on every file read. Default on large modern disks anyway.",
          vec![ps("fsutil behavior set disablelastaccess 1 | Out-Null")], vec![ps("fsutil behavior set disablelastaccess 2 | Out-Null")], false),
        T("transparency-off", "Performance", "Transparency effects: off", Safe,
          "Less GPU work for the taskbar, Start and window backgrounds.",
          vec![d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 0)], vec![], false),
        T("animations-off", "Performance", "Window animations: off", Safe,
          "Windows open, minimise and maximise instantly.",
          vec![s(r"HKCU\Control Panel\Desktop\WindowMetrics", "MinAnimate", "0"),
               d(ADV, "TaskbarAnimations", 0)], vec![], true),
        T("sysmain-off", "Performance", "SysMain (Superfetch): off", Aggressive,
          "Stops RAM prefetching. Can reduce disk activity on SSDs; slows app starts on HDDs.",
          vec![svc("SysMain", "Disabled")], vec![], false),
        T("search-index-off", "Performance", "Windows Search indexing: off", Aggressive,
          "Stops the background indexer. Start/Explorer search becomes slower and less complete.",
          vec![svc("WSearch", "Disabled")], vec![], false),

        // ------------------------------------------------------------ Gaming
        T("game-mode-on", "Gaming", "Game Mode: on", Safe,
          "Windows prioritises the game and pauses updates/notifications while playing.",
          vec![d(r"HKCU\Software\Microsoft\GameBar", "AutoGameModeEnabled", 1), d(r"HKCU\Software\Microsoft\GameBar", "AllowAutoGameMode", 1)], vec![], false),
        T("game-dvr-off", "Gaming", "Background game recording (Game DVR): off", Safe,
          "Stops constant background capture that costs FPS. Game Bar screenshots still work.",
          vec![d(r"HKCU\System\GameConfigStore", "GameDVR_Enabled", 0),
               d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0),
               d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR", 0)], vec![], false),
        T("hags-on", "Gaming", "Hardware-accelerated GPU scheduling: on", Moderate,
          "The GPU manages its own memory: lower latency on recent NVIDIA/AMD GPUs. Restart required.",
          vec![d(r"HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode", 2)], vec![], true),
        T("mmcss-games", "Gaming", "Multimedia scheduler: favour games", Moderate,
          "Gives foreground games higher CPU/GPU priority and stops network throttling during playback.",
          vec![d(MM, "SystemResponsiveness", 10), d(MM, "NetworkThrottlingIndex", 0xFFFF_FFFF),
               d(GAMES, "GPU Priority", 8), d(GAMES, "Priority", 6), s(GAMES, "Scheduling Category", "High"), s(GAMES, "SFIO Priority", "High")],
          vec![], true),
        T("mouse-precision-off", "Gaming", "Mouse acceleration (Enhance pointer precision): off", Safe,
          "1:1 mouse movement - what most games and players expect.",
          vec![s(r"HKCU\Control Panel\Mouse", "MouseSpeed", "0"), s(r"HKCU\Control Panel\Mouse", "MouseThreshold1", "0"),
               s(r"HKCU\Control Panel\Mouse", "MouseThreshold2", "0")], vec![], true),
        T("fso-off", "Gaming", "Fullscreen optimizations: off (global)", Aggressive,
          "Forces classic exclusive fullscreen. Helps some older games; can break Alt-Tab/HDR in newer ones.",
          vec![d(r"HKCU\System\GameConfigStore", "GameDVR_FSEBehaviorMode", 2), d(r"HKCU\System\GameConfigStore", "GameDVR_HonorUserFSEBehaviorMode", 1),
               d(r"HKCU\System\GameConfigStore", "GameDVR_DXGIHonorFSEWindowsCompatible", 1)], vec![], true),

        // ------------------------------------------------------------ Explorer & UI
        T("explorer-this-pc", "Explorer & UI", "Explorer opens 'This PC'", Safe,
          "File Explorer starts in This PC instead of Home.",
          vec![d(ADV, "LaunchTo", 1)], vec![], false),
        T("quick-access-private", "Explorer & UI", "No recent/frequent files in Explorer Home", Safe,
          "Explorer stops listing recently opened files and folders.",
          vec![d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer", "ShowRecent", 0),
               d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer", "ShowFrequent", 0)], vec![], false),
        T("compact-mode", "Explorer & UI", "Explorer compact view", Safe,
          "Less padding between items - more files on screen.",
          vec![d(ADV, "UseCompactMode", 1)], vec![], false),
        T("taskview-button-off", "Explorer & UI", "Task View button: hidden", Safe,
          "Removes the Task View button from the taskbar (Win+Tab still works).",
          vec![d(ADV, "ShowTaskViewButton", 0)], vec![], false),
        T("snap-flyout-off", "Explorer & UI", "Snap layout pop-up on maximise button: off", Safe,
          "No layout flyout when hovering the maximise button. Win+Z still works.",
          vec![d(ADV, "EnableSnapAssistFlyout", 0)], vec![], false),
        T("full-path-title", "Explorer & UI", "Full path in Explorer title bar", Safe,
          "Shows the complete folder path in the window title.",
          vec![d(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\CabinetState", "FullPath", 1)], vec![], false),
        T("long-paths-on", "Explorer & UI", "Long file paths (>260 chars): on", Safe,
          "Lets apps that support it use paths longer than 260 characters. Great for developers.",
          vec![d(r"HKLM\SYSTEM\CurrentControlSet\Control\FileSystem", "LongPathsEnabled", 1)], vec![], false),

        // ------------------------------------------------------------ Services (unused on most PCs)
        T("svc-fax", "Services", "Fax service: manual", Safe, "Only needed if you send faxes from this PC.", vec![svc("Fax", "Manual")], vec![], false),
        T("svc-retaildemo", "Services", "Retail Demo service: disabled", Safe, "Store display-PC mode. Never needed at home.", vec![svc("RetailDemo", "Disabled")], vec![], false),
        T("svc-maps", "Services", "Downloaded Maps Manager: manual", Safe, "Only used by the Maps app's offline maps.", vec![svc("MapsBroker", "Manual")], vec![], false),
        T("svc-diagtrack", "Services", "Connected User Experiences and Telemetry: disabled", Moderate, "The main telemetry service.", vec![svc("DiagTrack", "Disabled")], vec![], false),
        T("svc-wap", "Services", "WAP Push message routing: disabled", Safe, "Telemetry helper for device management messages.", vec![svc("dmwappushservice", "Disabled")], vec![], false),
        T("svc-xbox", "Services", "Xbox Live services: manual", Moderate, "Xbox sign-in and cloud saves start only when a game needs them. Controllers are unaffected.",
          vec![svc("XblAuthManager", "Manual"), svc("XblGameSave", "Manual"), svc("XboxNetApiSvc", "Manual")], vec![], false),
        T("svc-remote-registry", "Services", "Remote Registry: disabled", Safe, "Nobody on the network can edit this PC's registry.", vec![svc("RemoteRegistry", "Disabled")], vec![], false),
        T("svc-insider", "Services", "Windows Insider service: manual", Safe, "Only needed for Insider builds.", vec![svc("wisvc", "Manual")], vec![], false),

        // ------------------------------------------------------------ Network
        T("delivery-p2p-lan", "Network", "Update sharing: local network only", Safe,
          "Windows Update no longer uploads update pieces to strangers on the internet.",
          vec![d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization", "DODownloadMode", 1)], vec![], false),
        T("llmnr-off", "Network", "LLMNR name resolution: off", Moderate,
          "Closes a legacy protocol often abused for credential theft on public networks.",
          vec![d(r"HKLM\SOFTWARE\Policies\Microsoft\Windows NT\DNSClient", "EnableMulticast", 0)], vec![], false),
        T("wifi-sense-off", "Network", "Auto-connect to suggested open hotspots: off", Safe,
          "Windows stops automatically joining open hotspots it 'recommends'.",
          vec![d(r"HKLM\SOFTWARE\Microsoft\WcmSvc\wifinetworkmanager\config", "AutoConnectAllowedOEM", 0)], vec![], false),

        // ------------------------------------------------------------ Power
        T("ultimate-plan", "Power", "Ultimate Performance power plan", Moderate,
          "Adds and activates Microsoft's hidden highest-performance plan. Not for laptops on battery.",
          vec![ps("$o=powercfg -duplicatescheme e9a42b02-d5df-448d-aa00-03f14749eb61; if($o -match '([0-9a-f-]{36})'){powercfg -setactive $Matches[1]}")],
          vec![ps("powercfg -setactive 381b4222-f694-41f0-9685-ff5bb260df2e")], false),
        T("power-throttling-off", "Power", "Power throttling: off", Moderate,
          "Background apps keep full CPU speed. More performance, less battery life.",
          vec![d(r"HKLM\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling", "PowerThrottlingOff", 1)], vec![], true),
        T("usb-suspend-off", "Power", "USB selective suspend: off", Moderate,
          "Stops Windows from powering down USB devices - fixes disconnecting mice, DACs and hubs.",
          vec![ps("powercfg /setacvalueindex SCHEME_CURRENT 2a737441-1930-4402-8d77-b2bebba308a3 48e6b7a6-50f5-4782-a5d4-53bb8f07e226 0; powercfg /setactive SCHEME_CURRENT")],
          vec![ps("powercfg /setacvalueindex SCHEME_CURRENT 2a737441-1930-4402-8d77-b2bebba308a3 48e6b7a6-50f5-4782-a5d4-53bb8f07e226 1; powercfg /setactive SCHEME_CURRENT")], false),
    ];
    // a couple of reversible helpers for winutil-overlapping items stay out to avoid duplicates
    v.retain(|t| !t.0.is_empty());
    let _ = del; // kept for future items
    v
}

pub fn tweaks() -> Vec<Tweak> {
    list()
        .into_iter()
        .map(|T(id, cat, name, risk, desc, apply, undo, restart)| Tweak {
            id: id.into(),
            name: name.into(),
            category: cat.into(),
            desc: desc.into(),
            risk,
            source: "Broom",
            link: None,
            apply,
            undo,
            restart,
            hint: None,
        })
        .collect()
}

pub fn ids_in(cats: &[&str]) -> Vec<String> {
    list().into_iter().filter(|t| cats.contains(&t.1) && t.3 != Risk::Aggressive).map(|t| t.0.to_string()).collect()
}
