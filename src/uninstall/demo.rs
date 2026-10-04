//! Made-up but realistic data for documentation screenshots (`BROOM_DEMO=1`),
//! so README images never show a real person's installed software.

use super::startup::{Source, StartupItem};
use super::{Kind, Program};
use std::path::PathBuf;

pub fn on() -> bool {
    std::env::var_os("BROOM_DEMO").is_some()
}

#[allow(clippy::too_many_arguments)]
fn p(name: &str, ver: &str, publisher: &str, kind: Kind, mb: u64, date: &str, quiet: bool, running: bool) -> Program {
    let dir = format!(r"C:\Program Files\{name}");
    Program {
        id: format!("demo:{name}"),
        name: name.into(),
        version: ver.into(),
        publisher: publisher.into(),
        kind,
        size: mb << 20,
        measured: true,
        installed: date.into(),
        uninstall_cmd: Some(format!(r"{dir}\uninstall.exe")),
        quiet_cmd: quiet.then(|| format!(r#""{dir}\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART"#)),
        location: Some(PathBuf::from(dir)),
        reg_key: None,
        protected: false,
        running,
    }
}

pub fn programs() -> Vec<Program> {
    vec![
        p("Call of Duty HQ", "1.48.2", "Activision", Kind::Program, 148_000, "2026-08-12", false, false),
        p("Adobe Photoshop 2026", "27.4", "Adobe Inc.", Kind::Program, 4_310, "2026-05-02", false, false),
        p("Microsoft Office Professional", "16.0.19127", "Microsoft Corporation", Kind::Program, 3_870, "2026-01-19", false, true),
        p("Steam", "2.10.91", "Valve Corporation", Kind::Program, 1_650, "2025-11-30", true, true),
        p("Docker Desktop", "4.47.0", "Docker Inc.", Kind::Program, 1_420, "2026-07-08", false, false),
        p("Spotify", "1.2.71", "Spotify AB", Kind::Store, 912, "2026-09-21", true, true),
        p("Visual Studio Code", "1.104.2", "Microsoft Corporation", Kind::Program, 640, "2026-09-29", true, false),
        p("Discord", "1.0.9213", "Discord Inc.", Kind::Program, 520, "2026-09-03", true, true),
        p("Zoom Workplace", "6.6.4", "Zoom Communications", Kind::Msi, 410, "2026-06-11", true, false),
        p("Google Chrome", "141.0.7390", "Google LLC", Kind::Program, 395, "2026-10-01", true, true),
        p("OBS Studio", "32.0.1", "OBS Project", Kind::Program, 380, "2026-04-15", true, false),
        p("Old Printer Utility", "3.2", "PrintCo", Kind::Program, 310, "2019-03-02", true, false),
        p("Candy Crush Saga", "1.300", "King", Kind::Store, 210, "2026-03-03", true, false),
        p("WinRAR 7.11", "7.11", "win.rar GmbH", Kind::Program, 9, "2025-08-14", true, false),
        p("7-Zip 25.01 (x64)", "25.01", "Igor Pavlov", Kind::Program, 6, "2026-02-27", true, false),
    ]
}

pub fn startup() -> Vec<StartupItem> {
    let it = |name: &str, cmd: &str, on: bool, broken: bool| StartupItem {
        name: name.into(),
        command: cmd.into(),
        location: "User registry (Run)".into(),
        enabled: on,
        broken,
        source: Source::Folder { path: PathBuf::new(), approved: String::new() },
    };
    vec![
        it("Discord", r"C:\Users\you\AppData\Local\Discord\Update.exe --processStart Discord.exe", true, false),
        it("Spotify", r"C:\Users\you\AppData\Roaming\Spotify\Spotify.exe --autostart --minimized", true, false),
        it("Steam", r"C:\Program Files (x86)\Steam\steam.exe -silent", true, false),
        it("Docker Desktop", r"C:\Program Files\Docker\Docker\Docker Desktop.exe -Autostart", false, false),
        it("PrintCo Helper", r"C:\Program Files\PrintCo\helper.exe", true, true),
    ]
}
