//! Broom - clean, uninstall, tweak and diagnose your PC from one terminal UI.
//! https://github.com/philppplik/broom  ·  MIT

// Parts of the API are only used on some operating systems.
#![allow(dead_code)]

mod clean;
mod doctor;
mod tui;
mod tweaks;
mod uninstall;
mod util;

use clap::{Parser, Subcommand, ValueEnum};
use std::sync::atomic::Ordering;
use util::fs as bfs;

#[derive(Parser)]
#[command(name = "broom", version, about = "Clean, uninstall, tweak and diagnose your PC - Windows, macOS, Linux", long_about = None)]
struct Cli {
    /// Change nothing: every action only reports what it would do
    #[arg(long, global = true)]
    dry_run: bool,
    /// Windows: don't ask for administrator rights (many items are skipped)
    #[arg(long, global = true)]
    no_elevate: bool,
    /// Tab to open in the TUI
    #[arg(long, value_enum, default_value_t = StartTab::Home)]
    tab: StartTab,
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Clone, Copy, ValueEnum)]
enum StartTab {
    Home,
    Clean,
    Uninstall,
    Tweaks,
    Doctor,
    Backups,
}

#[derive(Clone, Copy, ValueEnum)]
enum Tier {
    Quick,
    Deep,
    Nuclear,
}

#[derive(Subcommand)]
enum Command {
    /// Clean junk. Without --yes only analyzes.
    Clean {
        #[arg(long, value_enum, default_value_t = Tier::Quick)]
        tier: Tier,
        /// Actually delete (otherwise: analysis only)
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// List installed software (largest first)
    Uninstall {
        #[arg(long)]
        json: bool,
        /// Measure folder sizes (slower, exact)
        #[arg(long)]
        measure: bool,
    },
    /// List, apply or undo tweaks
    Tweaks {
        #[command(subcommand)]
        action: Option<TweakCmd>,
    },
    /// Health check with hardware-aware recommendations
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Render the UI off-screen (for docs/screenshots/tests)
    #[command(hide = true)]
    Snapshot {
        #[arg(long, value_enum, default_value_t = StartTab::Home)]
        tab: StartTab,
        #[arg(long, default_value_t = 150)]
        width: u16,
        #[arg(long, default_value_t = 42)]
        height: u16,
        /// comma separated keys, e.g. "Right,Down,Space"
        #[arg(long, default_value = "")]
        keys: String,
        /// write an SVG instead of printing text
        #[arg(long)]
        svg: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 60)]
        wait: u64,
    },
    /// Benchmark public DNS resolvers from this machine
    Dns {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum TweakCmd {
    List {
        #[arg(long)]
        json: bool,
    },
    Apply {
        ids: Vec<String>,
    },
    Undo {
        ids: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    if cli.dry_run {
        bfs::DRY_RUN.store(true, Ordering::Relaxed);
    }
    let read_only = matches!(
        &cli.cmd,
        Some(Command::Uninstall { .. })
            | Some(Command::Doctor { .. })
            | Some(Command::Dns { .. })
            | Some(Command::Snapshot { .. })
            | Some(Command::Tweaks { action: None })
            | Some(Command::Tweaks { action: Some(TweakCmd::List { .. }) })
    ) || matches!(&cli.cmd, Some(Command::Clean { yes: false, .. }));
    #[cfg(windows)]
    if !cli.no_elevate && !read_only && !util::is_admin() {
        if util::relaunch_elevated() {
            return;
        }
        eprintln!("Broom works best as administrator; continuing with limited rights.");
    }
    let _ = read_only;
    util::log(format!(
        "=== Broom {} on {} ({}) {}",
        util::VERSION,
        util::sys::os_pretty(),
        util::native_arch(),
        if bfs::dry() { "[dry run]" } else { "" }
    ));

    let code = match cli.cmd {
        None => match tui::run(tab_index(cli.tab)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("broom: {e}");
                1
            }
        },
        Some(Command::Clean { tier, yes, json }) => cmd_clean(tier, yes, json),
        Some(Command::Uninstall { json, measure }) => {
            let mut v = uninstall::list();
            if measure {
                uninstall::measure(&mut v);
                v.sort_by(|a, b| b.size.cmp(&a.size));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                for p in &v {
                    println!("{:>10}  {:<7} {:<50} {}", util::fmt_size(p.size), p.kind.label(), util::ellipsize(&p.name, 50), p.version);
                }
                println!("\n{} programs", v.len());
            }
            0
        }
        Some(Command::Tweaks { action }) => cmd_tweaks(action),
        Some(Command::Snapshot { tab, width, height, keys, svg, wait }) => {
            let buf = tui::snapshot(tab_index(tab), width, height, &keys, std::time::Duration::from_secs(wait));
            match svg {
                Some(p) => {
                    let _ = std::fs::write(&p, tui::export::to_svg(&buf, "broom"));
                    eprintln!("wrote {}", p.display());
                }
                None => print!("{}", tui::export::to_text(&buf)),
            }
            0
        }
        Some(Command::Doctor { json }) => {
            let r = doctor::scan();
            if json {
                println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
            } else {
                let h = &r.hardware;
                println!("{} {} · {} · {} · {} RAM\n", h.manufacturer, h.model, h.os, h.cpu, util::fmt_size(h.ram_total));
                for f in &r.findings {
                    println!("[{:<8}] {:<12} {}\n{:>24}{}", f.severity.label(), f.area, f.title, "", f.detail);
                }
                println!("\nHealth score: {} / 100", r.score);
            }
            0
        }
        Some(Command::Dns { json }) => {
            let mut p = tweaks::dns::providers();
            tweaks::dns::benchmark(&mut p);
            if json {
                println!("{}", serde_json::to_string_pretty(&p).unwrap_or_default());
            } else {
                println!("Current: {:?}\n", tweaks::dns::current());
                for x in &p {
                    println!("{:<36} {:<16} {}", x.name, x.primary, x.latency.map(|l| format!("{l:.1} ms")).unwrap_or("-".into()));
                }
            }
            0
        }
    };
    util::log("=== exit");
    std::process::exit(code);
}

fn tab_index(t: StartTab) -> usize {
    match t {
        StartTab::Home => tui::TAB_HOME,
        StartTab::Clean => tui::TAB_CLEAN,
        StartTab::Uninstall => tui::TAB_UNINSTALL,
        StartTab::Tweaks => tui::TAB_TWEAKS,
        StartTab::Doctor => tui::TAB_DOCTOR,
        StartTab::Backups => tui::TAB_BACKUPS,
    }
}

fn cmd_clean(tier: Tier, yes: bool, json: bool) -> i32 {
    let max = match tier {
        Tier::Quick => 1,
        Tier::Deep => 2,
        Tier::Nuclear => 3,
    };
    if !yes {
        bfs::DRY_RUN.store(true, Ordering::Relaxed);
    }
    #[cfg(windows)]
    if yes && !bfs::dry() {
        util::reg::new_backup_set();
        if util::is_admin() {
            eprintln!("Creating restore point...");
            util::create_restore_point("Broom clean (CLI)");
        }
    }
    let items: Vec<clean::Item> = clean::catalog().into_iter().filter(|i| i.tier <= max).collect();
    let mut total = 0;
    let mut out = Vec::new();
    for it in &items {
        if !json {
            eprint!("{:<58}", util::ellipsize(it.name, 57));
        }
        let o = clean::execute(it);
        total += o.bytes;
        if !json {
            eprintln!("{:>10}  {}", util::fmt_size(o.bytes), o.note.clone().unwrap_or_default());
        }
        out.push(serde_json::json!({"id": it.id, "name": it.name, "category": it.category, "tier": clean::TIERS[it.tier as usize - 1], "risk": it.risk, "description": it.desc, "bytes": o.bytes, "files": o.files, "note": o.note}));
    }
    if json {
        println!("{}", serde_json::json!({"dry_run": bfs::dry(), "total_bytes": total, "items": out}));
    } else {
        eprintln!("\n{} {}", if bfs::dry() { "Could free" } else { "Freed" }, util::fmt_size(total));
        if !yes {
            eprintln!("(analysis only - add --yes to clean)");
        }
    }
    0
}

fn cmd_tweaks(action: Option<TweakCmd>) -> i32 {
    let all = tweaks::catalog();
    match action.unwrap_or(TweakCmd::List { json: false }) {
        TweakCmd::List { json } => {
            if json {
                let v: Vec<serde_json::Value> = all
                    .iter()
                    .map(|t| {
                        serde_json::json!({"id": t.id, "name": t.name, "category": t.category, "risk": t.risk, "source": t.source,
                                                "state": format!("{:?}", tweaks::state(t)), "undo": t.undo_kind(), "description": t.desc})
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                for t in &all {
                    let s = match tweaks::state(t) {
                        tweaks::State::Applied => "●",
                        tweaks::State::NotApplied => "○",
                        tweaks::State::Unknown => "?",
                    };
                    println!("{s} {:<34} {:<22} {}", t.id, t.category, t.name);
                }
            }
            0
        }
        TweakCmd::Apply { ids } | TweakCmd::Undo { ids } if ids.is_empty() => {
            eprintln!("give one or more tweak ids (see `broom tweaks list`)");
            2
        }
        cmd => {
            let undo = matches!(cmd, TweakCmd::Undo { .. });
            let ids = match cmd {
                TweakCmd::Apply { ids } | TweakCmd::Undo { ids } => ids,
                _ => vec![],
            };
            let mut rc = 0;
            for id in ids {
                match all.iter().find(|t| t.id.eq_ignore_ascii_case(&id)) {
                    None => {
                        eprintln!("unknown tweak: {id}");
                        rc = 2;
                    }
                    Some(t) => {
                        let r = if undo { tweaks::undo(t) } else { tweaks::apply(t) };
                        match r {
                            Ok(()) => println!("{} {}", if undo { "undone " } else { "applied" }, t.name),
                            Err(e) => {
                                eprintln!("failed  {}: {e}", t.name);
                                rc = 1;
                            }
                        }
                    }
                }
            }
            rc
        }
    }
}
