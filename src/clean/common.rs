//! Cleaning items that exist on every OS: browsers, Electron apps, developer caches.

use super::{Item, Outcome};
use crate::util::fs::{self as bfs, Walk};
use crate::util::{self, Risk};
use std::path::{Path, PathBuf};

/// Per-user application data roots: (local, roaming)
fn app_roots(home: &Path) -> (PathBuf, PathBuf) {
    if cfg!(windows) {
        (home.join("AppData/Local"), home.join("AppData/Roaming"))
    } else if cfg!(target_os = "macos") {
        (home.join("Library/Caches"), home.join("Library/Application Support"))
    } else {
        (home.join(".cache"), home.join(".config"))
    }
}

const CHROMIUM_ROOT_DIRS: &[&str] =
    &["ShaderCache", "GrShaderCache", "GraphiteDawnCache", "component_crx_cache", "Crashpad/reports", "BrowserMetrics"];
const CHROMIUM_PROFILE_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "Service Worker/CacheStorage",
    "Service Worker/ScriptCache",
    "Application Cache",
    "Media Cache",
];

/// Empty only the cache folders of a Chromium "User Data" / Electron / WebView2 folder.
pub fn clear_chromium(root: &Path) -> Outcome {
    let mut o = Outcome::default();
    if !root.is_dir() {
        return o;
    }
    for d in CHROMIUM_ROOT_DIRS {
        o += bfs::clear_all(root.join(d));
    }
    let mut profiles = vec![root.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n == "Default" || n.starts_with("Profile ") || n == "Guest Profile" {
                profiles.push(e.path());
            }
        }
    }
    for p in profiles {
        for d in CHROMIUM_PROFILE_DIRS {
            o += bfs::clear_all(p.join(d));
        }
    }
    o
}

fn browser_roots(home: &Path) -> Vec<PathBuf> {
    let (local, roaming) = app_roots(home);
    let list: Vec<PathBuf> = if cfg!(windows) {
        [
            "Microsoft/Edge/User Data",
            "Microsoft/Edge Beta/User Data",
            "Microsoft/Edge Dev/User Data",
            "Google/Chrome/User Data",
            "Google/Chrome Beta/User Data",
            "BraveSoftware/Brave-Browser/User Data",
            "Vivaldi/User Data",
            "Chromium/User Data",
            "Opera Software/Opera Stable",
            "Opera Software/Opera GX Stable",
        ]
        .iter()
        .map(|r| local.join(r))
        .chain(["Opera Software/Opera Stable", "Opera Software/Opera GX Stable"].iter().map(|r| roaming.join(r)))
        .collect()
    } else if cfg!(target_os = "macos") {
        // Chromium keeps its user data in Application Support and its HTTP cache in Caches
        [
            "Google/Chrome",
            "Microsoft Edge",
            "BraveSoftware/Brave-Browser",
            "Vivaldi",
            "Chromium",
            "com.operasoftware.Opera",
            "Arc/User Data",
        ]
        .iter()
        .flat_map(|r| [roaming.join(r), local.join(r)])
        .collect()
    } else {
        ["google-chrome", "microsoft-edge", "BraveSoftware/Brave-Browser", "vivaldi", "chromium", "opera"]
            .iter()
            .flat_map(|r| [roaming.join(r), local.join(r)])
            .collect()
    };
    list
}

fn browsers() -> Outcome {
    let mut o = Outcome::default();
    for h in util::profiles() {
        for r in browser_roots(&h) {
            let r2 = clear_chromium(&r);
            o.bytes += r2.bytes;
            o.files += r2.files;
        }
    }
    o
}

fn firefox() -> Outcome {
    let mut o = Outcome::default();
    for h in util::profiles() {
        let (local, roaming) = app_roots(&h);
        let bases: Vec<(PathBuf, PathBuf)> = if cfg!(windows) {
            ["Mozilla/Firefox", "librewolf", "Waterfox", "zen"].iter().map(|b| (local.join(b), roaming.join(b))).collect()
        } else if cfg!(target_os = "macos") {
            vec![(local.join("Firefox"), roaming.join("Firefox")), (local.join("zen"), roaming.join("zen"))]
        } else {
            vec![(local.join("mozilla/firefox"), h.join(".mozilla/firefox")), (local.join("librewolf"), h.join(".librewolf"))]
        };
        for (cache_base, data_base) in bases {
            for sub in ["cache2", "startupCache", "thumbnails", "jumpListCache", "shader-cache"] {
                o += bfs::clear_all(cache_base.join("Profiles/*").join(sub));
                o += bfs::clear_all(cache_base.join("*").join(sub)); // Linux layout: ~/.cache/mozilla/firefox/<profile>/cache2
            }
            for sub in ["Crash Reports", "Profiles/*/minidumps", "Profiles/*/saved-telemetry-pings", "*/minidumps"] {
                o += bfs::clear_all(data_base.join(sub));
            }
        }
    }
    o
}

/// Folders that look like Chromium/Electron/WebView2 data roots (contain GPUCache / Code Cache / Default/Cache).
pub fn find_chromium_roots(base: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut queue = vec![(base.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = queue.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
            if !md.is_dir() || bfs::is_link(&md) {
                continue;
            }
            if p.join("GPUCache").is_dir() || p.join("Code Cache").is_dir() || p.join("Default/Cache").is_dir() {
                found.push(p);
                continue;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if depth + 1 < max_depth && !["packages", "temp", "node_modules", ".git"].contains(&name.as_str()) {
                queue.push((p, depth + 1));
            }
        }
    }
    found
}

fn is_browser_root(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase().replace('\\', "/");
    ["/microsoft/edge", "/google/chrome", "bravesoftware", "/vivaldi", "/chromium", "opera software", "/opera", "microsoft edge", "/arc/"]
        .iter()
        .any(|b| s.contains(b))
}

fn app_caches() -> Outcome {
    let mut o = Outcome::default();
    for h in util::profiles() {
        let (local, roaming) = app_roots(&h);
        let mut roots = find_chromium_roots(&roaming, 4);
        roots.extend(find_chromium_roots(&local, 4));
        if cfg!(windows) {
            for pkg in bfs::expand(&local.join("Packages/*/LocalCache")) {
                roots.extend(find_chromium_roots(&pkg, 4));
            }
        }
        if !cfg!(windows) && !cfg!(target_os = "macos") {
            roots.extend(find_chromium_roots(&h.join(".config"), 3));
        }
        roots.sort();
        roots.dedup();
        for r in roots.iter().filter(|r| !is_browser_root(r)) {
            let r2 = clear_chromium(r);
            o.bytes += r2.bytes;
            o.files += r2.files;
        }
        for code in ["Code", "Code - Insiders", "Cursor", "VSCodium", "Windsurf"] {
            for sub in ["CachedData", "CachedExtensionVSIXs", "logs"] {
                o += bfs::clear_all(roaming.join(code).join(sub));
            }
        }
    }
    o
}

fn dev_caches() -> Outcome {
    let mut o = Outcome::default();
    for h in util::profiles() {
        let (local, roaming) = app_roots(&h);
        let rel_home = [
            ".npm/_cacache",
            ".cargo/registry/cache",
            ".cargo/git/checkouts",
            ".gradle/caches/build-cache-1",
            ".m2/repository/.cache",
            "go/pkg/mod/cache/download",
            ".bun/install/cache",
            ".deno/gen",
            "scoop/cache",
            ".pnpm-store",
            ".nuget/v3-cache",
        ];
        for r in rel_home {
            o += bfs::clear_all(h.join(r));
        }
        let rel_local: &[&str] = if cfg!(windows) {
            &[
                "npm-cache",
                "Yarn/Cache",
                "pnpm-cache",
                "pnpm/store",
                "pip/Cache",
                "uv/cache",
                "pypoetry/Cache",
                "NuGet/v3-cache",
                "NuGet/http-cache",
                "NuGet/plugins-cache",
                "go-build",
                "Composer/files",
                "Composer/repo",
                "Composer/vcs",
                "electron/Cache",
                "electron-builder/Cache",
                "ms-playwright/.links",
                "node-gyp/Cache",
                "deno/gen",
            ]
        } else if cfg!(target_os = "macos") {
            &[
                "Yarn",
                "pip",
                "uv",
                "pypoetry",
                "go-build",
                "composer",
                "electron",
                "electron-builder",
                "node-gyp",
                "Homebrew/downloads",
                "Homebrew/Cask",
                "CocoaPods",
                "org.swift.swiftpm",
                "pnpm",
                "deno",
            ]
        } else {
            &[
                "yarn",
                "pip",
                "uv",
                "pypoetry",
                "go-build",
                "composer",
                "electron",
                "electron-builder",
                "node-gyp",
                "pnpm",
                "deno",
                "Homebrew",
            ]
        };
        for r in rel_local {
            o += bfs::clear_all(local.join(r));
        }
        if cfg!(windows) {
            o += bfs::clear_all(roaming.join("npm-cache"));
        }
    }
    if cfg!(windows) {
        if let Some(pd) = util::env_path("ProgramData") {
            for r in ["scoop/cache", "chocolatey/lib-bkp", "chocolatey/lib-bad"] {
                o += bfs::clear_all(pd.join(r));
            }
        }
    }
    o
}

/// Rebuildable build output inside the user's projects (node_modules, target, build...).
/// Only folders next to a matching project file are considered, and only if untouched for 30 days.
/// (Idea credit: MangoDisk "project build artifacts".)
fn project_artifacts() -> Outcome {
    let rules: &[(&str, &[&str])] = &[
        ("node_modules", &["package.json"]),
        ("target", &["Cargo.toml"]),
        (".gradle", &["build.gradle", "build.gradle.kts", "settings.gradle"]),
        ("build", &["build.gradle", "build.gradle.kts", "CMakeLists.txt", "pubspec.yaml"]),
        (".next", &["next.config.js", "next.config.mjs", "next.config.ts"]),
        (".nuxt", &["nuxt.config.ts", "nuxt.config.js"]),
        (".svelte-kit", &["svelte.config.js"]),
        ("__pycache__", &[]),
        (".pytest_cache", &[]),
        (".mypy_cache", &[]),
        (".tox", &["tox.ini"]),
        ("obj", &["*.csproj", "*.fsproj"]),
        ("bin", &["*.csproj", "*.fsproj"]),
        ("DerivedData", &[]),
        (".dart_tool", &["pubspec.yaml"]),
        (".godot", &["project.godot"]),
        ("zig-cache", &["build.zig"]),
        (".zig-cache", &["build.zig"]),
    ];
    let month = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
    let mut o = Outcome::default();
    let mut n = 0;
    for h in util::profiles() {
        let roots = [
            "source",
            "src",
            "code",
            "dev",
            "projects",
            "Projects",
            "repos",
            "git",
            "GitHub",
            "workspace",
            "Developer",
            "Documents/GitHub",
            "Documents/Projects",
            "Desktop/projects",
        ];
        for r in roots {
            let base = h.join(r);
            if !base.is_dir() {
                continue;
            }
            let mut stack = vec![(base, 0usize)];
            while let Some((dir, depth)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
                    if !md.is_dir() || bfs::is_link(&md) {
                        continue;
                    }
                    let name = e.file_name().to_string_lossy().to_string();
                    if let Some((_, markers)) = rules.iter().find(|(d, _)| *d == name) {
                        let marker_ok = markers.is_empty()
                            || markers.iter().any(|m| {
                                if m.contains('*') {
                                    std::fs::read_dir(&dir)
                                        .map(|r| r.flatten().any(|x| bfs::wildmatch(m, &x.file_name().to_string_lossy())))
                                        .unwrap_or(false)
                                } else {
                                    dir.join(m).exists()
                                }
                            });
                        let stale = md.modified().map(|m| m < month).unwrap_or(false);
                        if marker_ok && stale {
                            // project root bypasses the personal-folder guard on purpose: only build output is touched
                            let mut f = bfs::Freed::default();
                            bfs::walk_files(&p, &Walk::all(), &mut |file, fmd| {
                                if bfs::dry() || std::fs::remove_file(file).is_ok() {
                                    f.bytes += fmd.len();
                                    f.files += 1;
                                }
                            });
                            if !bfs::dry() {
                                bfs::remove_empty_dirs(&p);
                                let _ = std::fs::remove_dir(&p);
                            }
                            util::log(format!("project artifact {} {}", p.display(), util::fmt_size(f.bytes)));
                            o += f;
                            n += 1;
                        }
                        continue;
                    }
                    if depth < 6 && !name.starts_with('.') {
                        stack.push((p, depth + 1));
                    }
                }
            }
        }
    }
    o.note(format!("{n} folders"))
}

/// Docker / Podman build cache and dangling images, via their own CLI.
fn containers() -> Outcome {
    let mut notes = Vec::new();
    for tool in ["docker", "podman"] {
        if !util::has_cmd(tool) {
            continue;
        }
        if bfs::dry() {
            let out = util::run(tool, &["system", "df"]).unwrap_or_default();
            if let Some(l) = out.lines().find(|l| l.to_lowercase().starts_with("build cache")) {
                notes.push(format!("{tool}: {}", l.split_whitespace().last().unwrap_or("?")));
            }
            continue;
        }
        let _ = util::run_status(tool, &["builder", "prune", "-f"]);
        let _ = util::run_status(tool, &["image", "prune", "-f"]);
        notes.push(format!("{tool} pruned"));
    }
    if notes.is_empty() {
        notes.push("not installed".into());
    }
    Outcome::default().note(notes.join(", "))
}

/// Download caches of local AI tooling. Models themselves (Ollama, LM Studio) are never touched.
fn ai_caches() -> Outcome {
    let mut o = Outcome::default();
    for h in util::profiles() {
        for r in [
            ".cache/huggingface/xet",
            ".cache/huggingface/hub/.locks",
            ".cache/torch/hub/checkpoints/.tmp",
            ".cache/pip",
            ".cache/whisper/.tmp",
        ] {
            o += bfs::clear_all(h.join(r));
        }
        o += bfs::clear(h.join(".cache/huggingface/hub"), &Walk::pattern(&["*.incomplete", "*.lock"]));
    }
    o
}

pub fn items() -> Vec<Item> {
    vec![
        Item { id: "browsers", category: "Browsers & apps", name: "Browser caches (Edge, Chrome, Brave, Opera, Vivaldi, Arc)",
               tier: 1, risk: Risk::Safe, procs: &["msedge", "chrome", "brave", "opera", "vivaldi", "chromium", "arc", "google chrome", "microsoft edge"],
               desc: "Web, code, GPU and shader caches of every browser profile. Passwords, cookies, history and logins are NOT touched.",
               delta: false, slow: false, run: browsers },
        Item { id: "firefox", category: "Browsers & apps", name: "Firefox-family caches (Firefox, LibreWolf, Zen...)",
               tier: 1, risk: Risk::Safe, procs: &["firefox", "librewolf", "waterfox", "zen"],
               desc: "cache2, startup cache, thumbnails and crash reports. Passwords, cookies and history stay.",
               delta: false, slow: false, run: firefox },
        Item { id: "appcache", category: "Browsers & apps", name: "App caches (Discord, Teams, Slack, VS Code, Steam...)",
               tier: 1, risk: Risk::Safe, procs: &["discord", "slack", "teams", "ms-teams", "code", "spotify", "notion", "signal"],
               desc: "Auto-detects every Electron / Chromium / WebView2 app and empties only its web, code and GPU caches. App data and logins stay.",
               delta: false, slow: false, run: app_caches },
        Item { id: "devcache", category: "Developer", name: "Package manager caches",
               tier: 2, risk: Risk::Moderate, procs: &[],
               desc: "npm, Yarn, pnpm, Bun, pip, uv, Poetry, NuGet, Go, Cargo, Gradle, Maven, Composer, Homebrew, CocoaPods, Scoop, Chocolatey. Re-downloaded when needed.",
               delta: false, slow: false, run: dev_caches },
        Item { id: "artifacts", category: "Developer", name: "Stale build output in your projects",
               tier: 3, risk: Risk::Moderate, procs: &[],
               desc: "node_modules, target, build, .next, __pycache__, obj/bin... in project folders (source, repos, GitHub, Developer...) untouched for 30+ days, only next to a matching project file. Rebuilt by the next build.",
               delta: false, slow: true, run: project_artifacts },
        Item { id: "containers", category: "Developer", name: "Docker / Podman build cache",
               tier: 3, risk: Risk::Moderate, procs: &[],
               desc: "Runs `builder prune` and `image prune` (dangling images only). Containers, volumes and tagged images stay.",
               delta: true, slow: true, run: containers },
        Item { id: "aicache", category: "Developer", name: "AI download leftovers (Hugging Face, Torch)",
               tier: 2, risk: Risk::Safe, procs: &[],
               desc: "Incomplete downloads, lock files and transfer caches of local AI tools. Downloaded models are never removed.",
               delta: false, slow: false, run: ai_caches },
    ]
}
