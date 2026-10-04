//! Backups tab: undo tweaks, import registry backups, restore quarantine, credits.

use super::list::ListView;
use super::theme::*;
use super::{Cmd, Confirm, Ctx, Tab};
use crate::util::{self, fs as bfs};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::path::PathBuf;

const VIEWS: [&str; 4] = ["Tweak journal", "Registry backups", "Quarantine", "About & credits"];

struct Folder {
    path: PathBuf,
    name: String,
    count: usize,
    bytes: u64,
}

pub struct BackupsTab {
    view: usize,
    journal: Vec<crate::tweaks::JournalEntry>,
    regs: Vec<Folder>,
    quarantine: Vec<Folder>,
    lists: [ListView; 3],
    action: u32,
}

const TAG_UNDO: u32 = 1;
const TAG_IMPORT: u32 = 2;
const TAG_RESTORE: u32 = 3;
const TAG_PURGE: u32 = 4;

fn folders(dir: &str) -> Vec<Folder> {
    let base = util::sub_dir(dir);
    let mut v: Vec<Folder> = std::fs::read_dir(&base)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| {
                    let p = e.path();
                    Folder {
                        name: e.file_name().to_string_lossy().to_string(),
                        count: std::fs::read_dir(&p).map(|r| r.count()).unwrap_or(0),
                        bytes: bfs::size_of(&p),
                        path: p,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.name.cmp(&a.name));
    v
}

fn pretty(name: &str) -> String {
    // 20261004-153012-label -> 2026-10-04 15:30  label
    if name.len() >= 15 && name.as_bytes()[8] == b'-' {
        let (d, t) = (&name[..8], &name[9..13]);
        let rest = name.get(16..).unwrap_or("").trim_start_matches('-');
        format!("{}-{}-{} {}:{}  {}", &d[..4], &d[4..6], &d[6..], &t[..2], &t[2..], rest)
    } else {
        name.to_string()
    }
}

impl BackupsTab {
    pub fn new() -> Self {
        BackupsTab { view: 0, journal: vec![], regs: vec![], quarantine: vec![], lists: Default::default(), action: 0 }
    }

    fn reload(&mut self) {
        self.journal = crate::tweaks::journal();
        self.journal.reverse();
        self.regs = folders("Backups");
        self.quarantine = folders("Quarantine");
    }
}

impl Tab for BackupsTab {
    fn title(&self) -> &'static str {
        "Backups"
    }

    fn activate(&mut self, _ctx: &mut Ctx) {
        self.reload();
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _ctx: &Ctx) {
        let [tabs, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).areas(area);
        let mut spans = Vec::new();
        for (i, v) in VIEWS.iter().enumerate() {
            spans.push(if i == self.view {
                Span::styled(format!(" {v} "), Style::new().fg(Color::Black).bg(ACCENT))
            } else {
                Span::styled(format!(" {v} "), Style::new().fg(TEXT))
            });
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("  ← → switch view", Style::new().fg(DIM)));
        f.render_widget(Paragraph::new(Line::from(spans)), tabs);
        match self.view {
            0 => {
                let rows = self
                    .journal
                    .iter()
                    .map(|e| {
                        Line::from(vec![
                            Span::styled(format!(" {:<18}", e.time), Style::new().fg(DIM)),
                            Span::styled(format!("{:<60}", util::ellipsize(&e.name, 59)), Style::new().fg(Color::White)),
                            Span::styled(format!("{} value(s) recorded", e.restore.len()), Style::new().fg(DIM)),
                        ])
                    })
                    .collect();
                let t = if self.journal.is_empty() {
                    "Tweak journal · nothing applied yet".to_string()
                } else {
                    format!("Tweak journal · {} active tweak(s) · Enter undoes the selected one", self.journal.len())
                };
                self.lists[0].render(f, body, focused_block(&t), rows);
            }
            1 | 2 => {
                let (list, what) = if self.view == 1 { (&self.regs, "registry backup") } else { (&self.quarantine, "quarantine") };
                let rows = list
                    .iter()
                    .map(|b| {
                        Line::from(vec![
                            Span::styled(format!(" {:<50}", util::ellipsize(&pretty(&b.name), 49)), Style::new().fg(Color::White)),
                            Span::styled(format!("{:>5} item(s)  ", b.count), Style::new().fg(DIM)),
                            size_span(b.bytes, true),
                        ])
                    })
                    .collect();
                let hint =
                    if self.view == 1 { "Enter re-imports every key of the set" } else { "Enter puts files back · d deletes for good" };
                let t = format!("{} · {} {what} set(s) · {hint}", VIEWS[self.view], list.len());
                self.lists[self.view].render(f, body, focused_block(&t), rows);
            }
            _ => {
                let link = |n: &str, who: &str, lic: &str, url: &str, what: &str| {
                    vec![
                        Line::from(vec![
                            Span::styled(format!("  {n}"), Style::new().fg(ACCENT).bold()),
                            Span::styled(format!("  by {who}  ·  {lic}"), Style::new().fg(DIM)),
                        ]),
                        Line::from(format!("    {url}")).fg(TEXT),
                        Line::from(format!("    {what}")).fg(DIM),
                    ]
                };
                let mut l = logo_lines();
                l.push(Line::from(format!("Broom {} · MIT · https://github.com/philppplik/broom", util::VERSION)).fg(ACCENT2));
                l.push(Line::default());
                l.push(Line::from("Data embedded (MIT, unmodified):").bold());
                l.extend(link(
                    "winutil",
                    "Chris Titus Tech / CT Tech Group LLC",
                    "MIT",
                    "https://github.com/ChrisTitusTech/winutil",
                    "tweaks.json, feature.json, appx.json, dns.json and preset.json are embedded verbatim and interpreted natively.",
                ));
                l.push(Line::default());
                l.push(Line::from("Approach followed (permissive licenses, own implementation - no code copied):").bold());
                l.extend(link(
                    "Bulk Crap Uninstaller",
                    "Marcin Szeniak",
                    "Apache-2.0",
                    "https://github.com/BCUninstaller/Bulk-Crap-Uninstaller",
                    "Quiet-uninstall detection per installer type and the leftover scan with confidence levels follow BCU's approach.",
                ));
                l.extend(link(
                    "Prune",
                    "jimman0I",
                    "MIT",
                    "https://github.com/jimman0I/prune",
                    "Quarantine-before-delete for leftovers, honest measured sizes.",
                ));
                l.push(Line::default());
                l.push(Line::from("Ideas that inspired independently written features (no code copied - GPL / CC-BY-SA):").bold());
                l.extend(link(
                    "Optimizer",
                    "hellzerg",
                    "GPL-3.0",
                    "https://github.com/hellzerg/optimizer",
                    "Service, privacy and performance tweak categories.",
                ));
                l.extend(link(
                    "Sparkle",
                    "thedogecraft",
                    "GPL-3.0",
                    "https://github.com/thedogecraft/sparkle",
                    "Find-fastest-DNS, GPU-aware tweaks, repair utilities.",
                ));
                l.extend(link(
                    "ReviOS Playbook",
                    "Revision",
                    "CC-BY-SA-4.0",
                    "https://github.com/meetrevision/playbook",
                    "Privacy & debloat philosophy.",
                ));
                l.extend(link(
                    "Slate Desktop",
                    "QuiteAFancyEmerald",
                    "GPL-3.0",
                    "https://github.com/QuiteAFancyEmerald/Slate-Desktop-for-Windows-11",
                    "Performance & shell tweak ideas.",
                ));
                l.extend(link(
                    "MangoDisk",
                    "harry0703",
                    "GPL-3.0",
                    "https://github.com/harry0703/MangoDisk",
                    "Project build artifacts, AI caches, startup management, system maintenance.",
                ));
                l.push(Line::default());
                l.push(Line::from(format!("Data folder: {}   (o opens it)", util::data_dir().display())).fg(DIM));
                l.push(Line::from(format!("This session's log: {}", util::log_path().display())).fg(DIM));
                f.render_widget(Paragraph::new(l).wrap(Wrap { trim: false }).block(focused_block("About & credits")), body);
            }
        }
    }

    fn sub(&mut self, d: i32, _ctx: &mut Ctx) {
        self.view = (self.view as i32 + d).rem_euclid(VIEWS.len() as i32) as usize;
    }

    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd {
        if self.view < 3 && self.lists[self.view].nav(&k) {
            return Cmd::None;
        }
        match (self.view, k.code) {
            (_, KeyCode::Char('r')) => self.reload(),
            (3, KeyCode::Char('o')) => util::open_in_file_manager(&util::data_dir()),
            (1, KeyCode::Char('o')) | (2, KeyCode::Char('o')) => {
                if let Some(b) = (if self.view == 1 { &self.regs } else { &self.quarantine }).get(self.lists[self.view].sel) {
                    util::open_in_file_manager(&b.path);
                }
            }
            (0, KeyCode::Enter) => {
                if let Some(e) = self.journal.get(self.lists[0].sel) {
                    self.action = TAG_UNDO;
                    return Cmd::Confirm(Confirm {
                        title: "Undo tweak".into(),
                        body: vec![Line::from(format!("Restore the values from before '{}' was applied?", e.name))],
                        yes: "Undo".into(),
                        danger: false,
                        tag: TAG_UNDO,
                    });
                }
            }
            (1, KeyCode::Enter) => {
                if !cfg!(windows) {
                    ctx.error("Registry backups exist on Windows only");
                } else if let Some(b) = self.regs.get(self.lists[1].sel) {
                    return Cmd::Confirm(Confirm {
                        title: "Import registry backup".into(),
                        body: vec![
                            Line::from(format!("Re-import all {} keys from {}?", b.count, pretty(&b.name))),
                            Line::from("Restores values Broom removed or changed in that run.").fg(DIM),
                        ],
                        yes: "Import".into(),
                        danger: false,
                        tag: TAG_IMPORT,
                    });
                }
            }
            (2, KeyCode::Enter) => {
                if let Some(b) = self.quarantine.get(self.lists[2].sel) {
                    return Cmd::Confirm(Confirm {
                        title: "Restore from quarantine".into(),
                        body: vec![Line::from(format!("Move everything in '{}' back to its original place?", pretty(&b.name)))],
                        yes: "Restore".into(),
                        danger: false,
                        tag: TAG_RESTORE,
                    });
                }
            }
            (2, KeyCode::Char('d')) | (2, KeyCode::Delete) => {
                if let Some(b) = self.quarantine.get(self.lists[2].sel) {
                    return Cmd::Confirm(Confirm {
                        title: "Delete for good".into(),
                        body: vec![Line::from(format!("Permanently delete '{}' ({})?", pretty(&b.name), util::fmt_size(b.bytes)))],
                        yes: "Delete".into(),
                        danger: true,
                        tag: TAG_PURGE,
                    });
                }
            }
            _ => {}
        }
        Cmd::None
    }

    fn confirmed(&mut self, tag: u32, ctx: &mut Ctx) {
        match tag {
            TAG_UNDO => {
                if let Some(e) = self.journal.get(self.lists[0].sel).cloned() {
                    match crate::tweaks::undo_journal_entry(&e.id) {
                        Ok(()) => ctx.toast(format!("Undone: {}", e.name)),
                        Err(x) => ctx.error(x),
                    }
                }
            }
            #[cfg(windows)]
            TAG_IMPORT => {
                if let Some(b) = self.regs.get(self.lists[1].sel) {
                    let mut files: Vec<PathBuf> =
                        std::fs::read_dir(&b.path).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
                    files.sort();
                    let ok = files.iter().filter(|f| crate::util::reg::import(f)).count();
                    ctx.toast(format!("Imported {ok} of {} registry files", files.len()));
                }
            }
            TAG_RESTORE => {
                if let Some(b) = self.quarantine.get(self.lists[2].sel) {
                    let n = bfs::restore_quarantine(&b.path);
                    ctx.toast(format!("Restored {n} item(s)"));
                }
            }
            TAG_PURGE => {
                if let Some(b) = self.quarantine.get(self.lists[2].sel) {
                    match std::fs::remove_dir_all(&b.path) {
                        Ok(()) => ctx.toast("Deleted"),
                        Err(e) => ctx.error(e.to_string()),
                    }
                }
            }
            _ => {}
        }
        self.reload();
    }

    fn click(&mut self, _c: u16, row: u16) {
        if self.view < 3 {
            self.lists[self.view].click(row);
        }
    }

    fn scroll(&mut self, d: i32) {
        if self.view < 3 {
            self.lists[self.view].scroll(d);
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.view {
            0 => vec![("Enter", "undo"), ("r", "reload")],
            1 => vec![("Enter", "import"), ("o", "open folder")],
            2 => vec![("Enter", "restore"), ("d", "delete"), ("o", "open folder")],
            _ => vec![("o", "open data folder")],
        }
    }
}
