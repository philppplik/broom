//! macOS tweaks via `defaults`. Undo restores the exact previous value (journal).

use super::{Op, Tweak};
use crate::util::Risk;

fn def(domain: &str, key: &str, ty: &str, value: &str) -> Op {
    Op::Defaults { domain: domain.into(), key: key.into(), ty: ty.into(), value: Some(value.into()) }
}
fn killall(app: &str) -> Op {
    Op::Cmd { prog: "killall".into(), args: vec![app.into()] }
}

fn t(id: &str, cat: &str, name: &str, risk: Risk, desc: &str, apply: Vec<Op>) -> Tweak {
    Tweak {
        id: id.into(),
        name: name.into(),
        category: cat.into(),
        desc: desc.into(),
        risk,
        source: "Broom",
        link: None,
        apply,
        undo: vec![],
        restart: false,
        hint: None,
    }
}

pub fn tweaks() -> Vec<Tweak> {
    use Risk::*;
    const F: &str = "com.apple.finder";
    const G: &str = "NSGlobalDomain";
    const D: &str = "com.apple.dock";
    vec![
        t(
            "finder-hidden",
            "Finder & Dock",
            "Finder: show hidden files",
            Safe,
            "Shows dot-files and hidden folders (Cmd+Shift+. toggles too).",
            vec![def(F, "AppleShowAllFiles", "bool", "true"), killall("Finder")],
        ),
        t(
            "finder-ext",
            "Finder & Dock",
            "Finder: always show file extensions",
            Safe,
            "Protects against 'invoice.pdf.app' tricks.",
            vec![def(G, "AppleShowAllExtensions", "bool", "true"), killall("Finder")],
        ),
        t(
            "finder-pathbar",
            "Finder & Dock",
            "Finder: path and status bar",
            Safe,
            "Shows where you are and how much space is free.",
            vec![def(F, "ShowPathbar", "bool", "true"), def(F, "ShowStatusBar", "bool", "true"), killall("Finder")],
        ),
        t(
            "finder-list",
            "Finder & Dock",
            "Finder: list view by default",
            Safe,
            "New windows open in list view.",
            vec![def(F, "FXPreferredViewStyle", "string", "Nlsv"), killall("Finder")],
        ),
        t(
            "finder-folders-first",
            "Finder & Dock",
            "Finder: folders on top",
            Safe,
            "Sorts folders before files.",
            vec![def(F, "_FXSortFoldersFirst", "bool", "true"), killall("Finder")],
        ),
        t(
            "no-ds-store",
            "Finder & Dock",
            "No .DS_Store files on network and USB drives",
            Safe,
            "Stops littering shares and sticks with metadata files.",
            vec![
                def("com.apple.desktopservices", "DSDontWriteNetworkStores", "bool", "true"),
                def("com.apple.desktopservices", "DSDontWriteUSBStores", "bool", "true"),
            ],
        ),
        t(
            "dock-fast",
            "Finder & Dock",
            "Dock: instant auto-hide",
            Safe,
            "Removes the delay before the hidden Dock appears.",
            vec![def(D, "autohide-delay", "float", "0"), def(D, "autohide-time-modifier", "float", "0.3"), killall("Dock")],
        ),
        t(
            "dock-recents",
            "Finder & Dock",
            "Dock: no recent apps",
            Safe,
            "Only your pinned apps appear in the Dock.",
            vec![def(D, "show-recents", "bool", "false"), killall("Dock")],
        ),
        t(
            "dock-minimize-scale",
            "Finder & Dock",
            "Dock: faster minimize effect",
            Safe,
            "Scale instead of the genie animation.",
            vec![def(D, "mineffect", "string", "scale"), killall("Dock")],
        ),
        t(
            "save-expanded",
            "Desktop",
            "Expanded save & print dialogs",
            Safe,
            "Save and print panels open fully expanded.",
            vec![def(G, "NSNavPanelExpandedStateForSaveMode", "bool", "true"), def(G, "PMPrintingExpandedStateForPrint", "bool", "true")],
        ),
        t(
            "save-local",
            "Desktop",
            "Save to disk, not iCloud, by default",
            Safe,
            "New documents default to your Mac instead of iCloud Drive.",
            vec![def(G, "NSDocumentSaveNewDocumentsToCloud", "bool", "false")],
        ),
        t(
            "key-repeat",
            "Desktop",
            "Fast key repeat",
            Safe,
            "Holding a key repeats quickly - great for editing. Sign out to apply.",
            vec![def(G, "KeyRepeat", "int", "2"), def(G, "InitialKeyRepeat", "int", "15")],
        ),
        t(
            "no-press-hold",
            "Desktop",
            "Key repeat instead of accent menu",
            Safe,
            "Holding a letter repeats it instead of showing accented characters.",
            vec![def(G, "ApplePressAndHoldEnabled", "bool", "false")],
        ),
        t(
            "no-autocorrect",
            "Desktop",
            "Autocorrect and smart quotes: off",
            Safe,
            "Stops macOS from changing what you type (code, terminals, chats).",
            vec![
                def(G, "NSAutomaticSpellingCorrectionEnabled", "bool", "false"),
                def(G, "NSAutomaticQuoteSubstitutionEnabled", "bool", "false"),
                def(G, "NSAutomaticDashSubstitutionEnabled", "bool", "false"),
            ],
        ),
        t(
            "window-anim-off",
            "Performance",
            "Window opening animations: off",
            Safe,
            "Windows appear instantly.",
            vec![def(G, "NSAutomaticWindowAnimationsEnabled", "bool", "false")],
        ),
        t(
            "resize-fast",
            "Performance",
            "Faster window resize",
            Safe,
            "Shortens the resize animation of Cocoa windows.",
            vec![def(G, "NSWindowResizeTime", "float", "0.001")],
        ),
        t(
            "screenshot-png-noshadow",
            "Desktop",
            "Screenshots: PNG without window shadow",
            Safe,
            "Cleaner window screenshots.",
            vec![
                def("com.apple.screencapture", "type", "string", "png"),
                def("com.apple.screencapture", "disable-shadow", "bool", "true"),
                killall("SystemUIServer"),
            ],
        ),
        t(
            "tm-no-prompt",
            "System",
            "Time Machine: don't ask for every new disk",
            Safe,
            "Stops the 'use this disk for backups?' prompt.",
            vec![def("com.apple.TimeMachine", "DoNotOfferNewDisksForBackup", "bool", "true")],
        ),
        t(
            "crash-dialog-off",
            "System",
            "Crash reporter dialog: notification only",
            Safe,
            "Crashes show a notification instead of a modal dialog.",
            vec![def("com.apple.CrashReporter", "DialogType", "string", "none")],
        ),
        t(
            "ad-tracking",
            "Privacy",
            "Personalized Apple ads: off",
            Safe,
            "Apple stops using your data for personalized ads in its apps.",
            vec![def("com.apple.AdLib", "allowApplePersonalizedAdvertising", "bool", "false")],
        ),
        t(
            "siri-analytics",
            "Privacy",
            "Siri & dictation improvement sharing: off",
            Safe,
            "Audio recordings are not shared with Apple.",
            vec![def("com.apple.assistant.support", "Siri Data Sharing Opt-In Status", "int", "2")],
        ),
        t(
            "quarantine-off",
            "System",
            "'Downloaded from the internet' warnings: off",
            Aggressive,
            "Removes Gatekeeper's confirmation dialog for downloaded apps. Less protection.",
            vec![def("com.apple.LaunchServices", "LSQuarantine", "bool", "false")],
        ),
    ]
}
