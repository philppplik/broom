//! Uninstall tab: programs (with leftovers review) and startup manager.

use super::list::{ListView, Search};
use super::theme::*;
use super::{Cmd, Confirm, Ctx, Job, Tab};
use crate::uninstall::{self, startup, Confidence, Kind, Leftover, Program, UninstallMode};
use crate::util::{self, fs as bfs};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::collections::HashSet;

enum Ev {
    List(Vec<Program>),
    Measured(Vec<Program>),
    Done(String, Result<String, String>),
    Leftovers(String, Vec<Leftover>),
    Removed(usize, u64, std::path::PathBuf),
    Startup(Vec<startup::StartupItem>),
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Size,
    Name,
    Date,
    Kind,
}

struct Review {
    program: String,
    items: Vec<Leftover>,
    sel: Vec<bool>,
    list: ListView,
}

pub struct UninstallTab {
    view: usize,
    programs: Vec<Program>,
    shown: Vec<usize>,
    marks: HashSet<String>,
    list: ListView,
    search: Search,
    sort: Sort,
    job: Option<Job<Ev>>,
    loaded: bool,
    queue: Vec<Program>,
    mode: UninstallMode,
    review: Option<Review>,
    reviews_pending: Vec<Review>,
    startup: Vec<startup::StartupItem>,
    slist: ListView,
}

const TAG_UNINSTALL: u32 = 1;
const TAG_REMOVE_LEFTOVERS: u32 = 2;
const TAG_DELETE_STARTUP: u32 = 3;
const TAG_AFTER_FOREGROUND: u32 = 4;

impl UninstallTab {
    pub fn new() -> Self {
        UninstallTab {
            view: 0,
            programs: vec![],
            shown: vec![],
            marks: HashSet::new(),
            list: ListView::default(),
            search: Search::default(),
            sort: Sort::Size,
            job: None,
            loaded: false,
            queue: vec![],
            mode: UninstallMode::Auto,
            review: None,
            reviews_pending: vec![],
            startup: vec![],
            slist: ListView::default(),
        }
    }

    fn load(&mut self) {
        if self.job.is_some() {
            return;
        }
        self.job = Some(Job::spawn("Reading installed software", |r| {
            r.send(Ev::Startup(startup::list()));
            let mut v = uninstall::list();
            r.send(Ev::List(v.clone()));
            r.progress(0, 0, "Measuring program folders");
            uninstall::measure(&mut v);
            r.send(Ev::Measured(v));
        }));
    }

    fn refilter(&mut self) {
        let mut idx: Vec<usize> = (0..self.programs.len())
            .filter(|i| {
                let p = &self.programs[*i];
                self.search.matches(&format!("{} {} {}", p.name, p.publisher, p.kind.label()))
            })
            .collect();
        let p = &self.programs;
        match self.sort {
            Sort::Size => idx.sort_by(|a, b| p[*b].size.cmp(&p[*a].size)),
            Sort::Name => idx.sort_by(|a, b| p[*a].name.to_lowercase().cmp(&p[*b].name.to_lowercase())),
            Sort::Date => idx.sort_by(|a, b| p[*b].installed.cmp(&p[*a].installed)),
            Sort::Kind => idx.sort_by(|a, b| p[*a].kind.label().cmp(p[*b].kind.label()).then(p[*b].size.cmp(&p[*a].size))),
        }
        self.shown = idx;
    }

    fn current(&self) -> Option<&Program> {
        self.shown.get(self.list.sel).map(|i| &self.programs[*i])
    }

    fn targets(&self) -> Vec<Program> {
        if self.marks.is_empty() {
            self.current().cloned().into_iter().collect()
        } else {
            self.programs.iter().filter(|p| self.marks.contains(&p.id)).cloned().collect()
        }
    }

    fn start_uninstall(&mut self) {
        let queue = std::mem::take(&mut self.queue);
        let all = self.programs.clone();
        let mode = self.mode.clone();
        self.job = Some(Job::spawn("Uninstalling", move |r| {
            let n = queue.len();
            for (i, p) in queue.iter().enumerate() {
                r.progress(i, n, format!("Uninstalling {}", p.name));
                let res = uninstall::uninstall(p, mode.clone());
                let ok = res.is_ok();
                r.send(Ev::Done(p.name.clone(), res));
                if ok {
                    r.progress(i, n, format!("Looking for leftovers of {}", p.name));
                    let rest: Vec<Program> = all.iter().filter(|o| o.id != p.id).cloned().collect();
                    let l = uninstall::scan_leftovers(p, &rest);
                    if !l.is_empty() {
                        r.send(Ev::Leftovers(p.name.clone(), l));
                    }
                }
            }
            r.progress(n, n, "Refreshing list");
            let v = uninstall::list();
            r.send(Ev::List(v));
        }));
    }

    fn render_programs(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [main, detail] = Layout::vertical([Constraint::Min(6), Constraint::Length(7)]).areas(area);
        let rows: Vec<Line<'static>> = self
            .shown
            .iter()
            .map(|i| {
                let p = &self.programs[*i];
                let name_style = if p.protected { Style::new().fg(DIM) } else { Style::new().fg(Color::White) };
                Line::from(vec![
                    checkbox(self.marks.contains(&p.id)),
                    Span::styled(format!("{:<46}", util::ellipsize(&p.name, 45)), name_style),
                    Span::styled(format!("{:<7}", p.kind.label()), Style::new().fg(ACCENT)),
                    size_span(p.size, p.measured),
                    Span::styled(format!("  {:<10}", p.installed), Style::new().fg(DIM)),
                    Span::styled(format!(" {:<26}", util::ellipsize(&p.publisher, 25)), Style::new().fg(DIM)),
                    if p.running { Span::styled(" running", Style::new().fg(WARN)) } else { Span::raw("") },
                    if p.protected {
                        Span::styled(" system", Style::new().fg(DIM))
                    } else if p.can_quiet() {
                        Span::styled(" silent", Style::new().fg(OK))
                    } else {
                        Span::raw("")
                    },
                ])
            })
            .collect();
        let total: u64 = self.shown.iter().map(|i| self.programs[*i].size).sum();
        let sort = match self.sort {
            Sort::Size => "size",
            Sort::Name => "name",
            Sort::Date => "date",
            Sort::Kind => "type",
        };
        let title = if !self.loaded {
            format!("Programs {} loading...", spinner(ctx.tick))
        } else {
            format!(
                "Programs · {} shown · {} · sort: {sort} · {} marked{}",
                self.shown.len(),
                util::fmt_size(total),
                self.marks.len(),
                self.search.title_suffix()
            )
        };
        self.list.render(f, main, focused_block(&title), rows);

        let mut d: Vec<Line> = Vec::new();
        if let Some(p) = self.current() {
            d.push(Line::from(vec![
                Span::styled(p.name.clone(), Style::new().fg(ACCENT).bold()),
                Span::styled(format!("  {}  {}", p.version, p.publisher), Style::new().fg(TEXT)),
            ]));
            d.push(Line::from(vec![
                Span::styled("Size ", Style::new().fg(DIM)),
                Span::raw(if p.size == 0 {
                    "unknown".into()
                } else if p.measured {
                    format!("{} (measured)", util::fmt_size(p.size))
                } else {
                    format!("{} (as reported by the installer)", util::fmt_size(p.size))
                }),
                Span::styled("   Type ", Style::new().fg(DIM)),
                Span::raw(p.kind.label().to_string()),
            ]));
            if let Some(l) = &p.location {
                d.push(Line::from(vec![Span::styled("Folder ", Style::new().fg(DIM)), Span::raw(l.display().to_string())]));
            }
            let how = if p.protected {
                "Protected system component - cannot be removed".to_string()
            } else if let Some(q) = &p.quiet_cmd {
                format!("Silent: {}", util::ellipsize(q, 110))
            } else {
                format!("Wizard: {}", util::ellipsize(p.uninstall_cmd.as_deref().unwrap_or("-"), 110))
            };
            d.push(Line::from(vec![Span::styled("Uninstall ", Style::new().fg(DIM)), Span::raw(how)]));
            if p.running {
                d.push(Line::from("Running right now - close it first for a clean removal.").fg(WARN));
            }
        }
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details")), detail);
    }

    fn render_startup(&mut self, f: &mut Frame, area: Rect) {
        let [main, detail] = Layout::vertical([Constraint::Min(6), Constraint::Length(5)]).areas(area);
        let rows: Vec<Line<'static>> = self
            .startup
            .iter()
            .map(|s| {
                Line::from(vec![
                    if s.enabled {
                        Span::styled(" ● on  ", Style::new().fg(OK))
                    } else {
                        Span::styled(" ○ off ", Style::new().fg(DIM))
                    },
                    Span::styled(
                        format!("{:<40}", util::ellipsize(&s.name, 39)),
                        Style::new().fg(if s.enabled { Color::White } else { TEXT }),
                    ),
                    Span::styled(format!("{:<26}", s.location), Style::new().fg(DIM)),
                    if s.broken { Span::styled("target missing ", Style::new().fg(BAD)) } else { Span::raw("") },
                    Span::styled(util::ellipsize(&s.command, 70), Style::new().fg(DIM)),
                ])
            })
            .collect();
        let on = self.startup.iter().filter(|s| s.enabled).count();
        self.slist.render(f, main, focused_block(&format!("Startup · {on} of {} enabled", self.startup.len())), rows);
        let mut d = Vec::new();
        if let Some(s) = self.startup.get(self.slist.sel) {
            d.push(Line::from(s.name.clone()).fg(ACCENT).bold());
            d.push(Line::from(s.command.clone()).fg(TEXT));
            d.push(
                Line::from(
                    "Disabling is reversible (same switch as Task Manager). Deleting moves files to quarantine and backs up the registry.",
                )
                .fg(DIM),
            );
        }
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details")), detail);
    }

    fn render_review(&mut self, f: &mut Frame, area: Rect) {
        let Some(rv) = self.review.as_mut() else { return };
        let [main, detail] = Layout::vertical([Constraint::Min(6), Constraint::Length(4)]).areas(area);
        let rows: Vec<Line<'static>> = rv
            .items
            .iter()
            .enumerate()
            .map(|(i, l)| {
                Line::from(vec![
                    checkbox(rv.sel[i]),
                    match l.confidence {
                        Confidence::High => Span::styled("HIGH    ", Style::new().fg(OK)),
                        Confidence::Medium => Span::styled("MEDIUM  ", Style::new().fg(WARN)),
                    },
                    Span::styled(
                        format!("{:<10}", format!("{:?}", l.kind).split([' ', '{']).next().unwrap_or("").to_string()),
                        Style::new().fg(ACCENT),
                    ),
                    size_span(l.size, true),
                    Span::raw(format!("  {}", util::ellipsize(&l.path, 90))),
                ])
            })
            .collect();
        let sel_bytes: u64 = rv.items.iter().zip(&rv.sel).filter(|(_, s)| **s).map(|(l, _)| l.size).sum();
        let title = format!("Leftovers of {} · {} found · {} selected", rv.program, rv.items.len(), util::fmt_size(sel_bytes));
        rv.list.render(f, main, focused_block(&title).border_style(Style::new().fg(ACCENT2)), rows);
        let mut d = vec![];
        if let Some(l) = rv.items.get(rv.list.sel) {
            d.push(Line::from(format!("Why: {}", l.reason)).fg(TEXT));
        }
        d.push(
            Line::from(
                "Files and folders go to quarantine (restorable in Backups), registry is exported first. Medium = review before ticking.",
            )
            .fg(DIM),
        );
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Review")), detail);
    }
}

impl Tab for UninstallTab {
    fn title(&self) -> &'static str {
        "Uninstall"
    }

    fn activate(&mut self, _ctx: &mut Ctx) {
        if !self.loaded && self.job.is_none() {
            self.load();
        }
    }

    fn typing(&self) -> bool {
        self.search.active
    }

    fn busy(&self, tick: usize) -> Option<String> {
        self.job.as_ref().map(|j| j.status(tick))
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        let Some(j) = self.job.as_mut() else { return };
        let evs = j.poll();
        let finished = j.is_finished();
        for e in evs {
            match e {
                Ev::List(v) | Ev::Measured(v) => {
                    let keep = self.current().map(|p| p.id.clone());
                    self.programs = v;
                    self.loaded = true;
                    self.marks.retain(|m| self.programs.iter().any(|p| &p.id == m));
                    self.refilter();
                    if let Some(id) = keep {
                        if let Some(pos) = self.shown.iter().position(|i| self.programs[*i].id == id) {
                            self.list.sel = pos;
                        }
                    }
                }
                Ev::Startup(s) => self.startup = s,
                Ev::Done(name, Ok(m)) => ctx.toast(format!("{name}: {m}")),
                Ev::Done(name, Err(e)) => ctx.error(format!("{name}: {e}")),
                Ev::Leftovers(name, items) => {
                    let sel = items.iter().map(|l| l.confidence == Confidence::High).collect();
                    self.reviews_pending.push(Review { program: name, items, sel, list: ListView::default() });
                }
                Ev::Removed(n, bytes, q) => {
                    ctx.toast(format!("Removed {n} leftovers ({}) - quarantine: {}", util::fmt_size(bytes), q.display()))
                }
            }
        }
        if finished {
            self.job = None;
        }
        if self.review.is_none() && !self.reviews_pending.is_empty() {
            self.review = Some(self.reviews_pending.remove(0));
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [tabs, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).areas(area);
        let v = |i: usize, t: &str| {
            if self.view == i && self.review.is_none() {
                Span::styled(format!(" {t} "), Style::new().fg(Color::Black).bg(ACCENT))
            } else {
                Span::styled(format!(" {t} "), Style::new().fg(TEXT))
            }
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                v(0, "Programs"),
                Span::raw(" "),
                v(1, "Startup"),
                Span::styled("   ← → switch view", Style::new().fg(DIM)),
            ])),
            tabs,
        );
        if self.review.is_some() {
            self.render_review(f, body);
        } else if self.view == 0 {
            self.render_programs(f, body, ctx);
        } else {
            self.render_startup(f, body);
        }
    }

    fn sub(&mut self, d: i32, _ctx: &mut Ctx) {
        if self.review.is_none() {
            self.view = ((self.view as i32 + d).rem_euclid(2)) as usize;
        }
    }

    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd {
        // leftovers review has priority
        if let Some(rv) = self.review.as_mut() {
            if rv.list.nav(&k) {
                return Cmd::None;
            }
            match k.code {
                KeyCode::Char(' ') => {
                    let i = rv.list.sel;
                    rv.sel[i] = !rv.sel[i];
                }
                KeyCode::Char('a') => rv.sel.iter_mut().for_each(|s| *s = true),
                KeyCode::Char('n') => rv.sel.iter_mut().for_each(|s| *s = false),
                KeyCode::Esc => {
                    self.review = None;
                    ctx.toast("Leftovers kept");
                }
                KeyCode::Char('r') | KeyCode::Enter => {
                    if self.job.is_some() {
                        ctx.error("Wait until the current task has finished");
                        return Cmd::None;
                    }
                    let n = rv.sel.iter().filter(|s| **s).count();
                    if n == 0 {
                        return Cmd::None;
                    }
                    return Cmd::Confirm(Confirm {
                        title: "Remove leftovers".into(),
                        body: vec![
                            Line::from(format!("Remove {n} leftovers of {}?", rv.program)),
                            Line::default(),
                            Line::from("Files/folders are moved to quarantine, registry keys are exported first.").fg(OK),
                        ],
                        yes: "Remove".into(),
                        danger: false,
                        tag: TAG_REMOVE_LEFTOVERS,
                    });
                }
                _ => {}
            }
            return Cmd::None;
        }
        if self.search.active {
            if self.search.key(&k) {
                self.refilter();
                self.list.sel = 0;
            }
            return Cmd::None;
        }
        if self.view == 1 {
            if self.slist.nav(&k) {
                return Cmd::None;
            }
            let Some(item) = self.startup.get(self.slist.sel).cloned() else { return Cmd::None };
            match k.code {
                KeyCode::Char(' ') | KeyCode::Enter => match startup::set_enabled(&item, !item.enabled) {
                    Ok(()) => {
                        if !bfs::dry() {
                            self.startup[self.slist.sel].enabled = !item.enabled;
                        }
                        ctx.toast(format!("{} {}", item.name, if item.enabled { "disabled" } else { "enabled" }));
                    }
                    Err(e) => ctx.error(e),
                },
                KeyCode::Delete | KeyCode::Char('d') => {
                    return Cmd::Confirm(Confirm {
                        title: "Delete startup entry".into(),
                        body: vec![
                            Line::from(format!("Delete '{}' permanently?", item.name)),
                            Line::from("Tip: Space only disables it, which is easier to undo.").fg(DIM),
                        ],
                        yes: "Delete".into(),
                        danger: true,
                        tag: TAG_DELETE_STARTUP,
                    })
                }
                KeyCode::Char('r') => self.load(),
                _ => {}
            }
            return Cmd::None;
        }
        if self.list.nav(&k) {
            return Cmd::None;
        }
        match k.code {
            KeyCode::Char('/') => self.search.active = true,
            KeyCode::Esc => {
                self.search.text.clear();
                self.marks.clear();
                self.refilter();
            }
            KeyCode::Char(' ') => {
                if let Some(p) = self.current() {
                    let id = p.id.clone();
                    if !p.protected && !self.marks.remove(&id) {
                        self.marks.insert(id);
                    }
                }
                self.list.move_by(1);
            }
            KeyCode::Char('s') => {
                self.sort = match self.sort {
                    Sort::Size => Sort::Name,
                    Sort::Name => Sort::Date,
                    Sort::Date => Sort::Kind,
                    Sort::Kind => Sort::Size,
                };
                self.refilter();
            }
            KeyCode::Char('r') => self.load(),
            KeyCode::Char('l') => {
                if let Some(p) = self.current().cloned() {
                    let all = self.programs.clone();
                    let rest: Vec<Program> = all.into_iter().filter(|o| o.id != p.id).collect();
                    let items = uninstall::scan_leftovers(&p, &rest);
                    if items.is_empty() {
                        ctx.toast(format!("{}: nothing outside the program itself", p.name));
                    } else {
                        let sel = items.iter().map(|_| false).collect();
                        self.review = Some(Review {
                            program: format!("{} (still installed - nothing pre-selected)", p.name),
                            items,
                            sel,
                            list: ListView::default(),
                        });
                    }
                }
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                if self.job.is_some() {
                    return Cmd::None;
                }
                let t: Vec<Program> = self.targets().into_iter().filter(|p| !p.protected).collect();
                if t.is_empty() {
                    ctx.error("Nothing to uninstall (system components are protected)");
                    return Cmd::None;
                }
                self.mode = if k.code == KeyCode::Char('U') { UninstallMode::Interactive } else { UninstallMode::Auto };
                // package managers on Linux/macOS need sudo: run in the real terminal
                if let Some(cmd) = foreground_cmd(&t) {
                    self.queue = t;
                    return Cmd::Foreground(cmd, TAG_AFTER_FOREGROUND);
                }
                let mut body = vec![Line::from(format!("Uninstall {} program(s):", t.len()))];
                for p in t.iter().take(10) {
                    let how = if self.mode == UninstallMode::Auto && p.can_quiet() { "silent" } else { "wizard" };
                    body.push(Line::from(vec![
                        Span::raw(format!("  • {} ", p.name)),
                        Span::styled(format!("({how}, {})", util::fmt_size(p.size)), Style::new().fg(DIM)),
                    ]));
                }
                if t.len() > 10 {
                    body.push(Line::from(format!("  ... and {} more", t.len() - 10)).fg(DIM));
                }
                if t.iter().any(|p| p.running) {
                    body.push(Line::from("Some of them are running - close them first.").fg(WARN));
                }
                body.push(Line::default());
                body.push(Line::from("Afterwards Broom looks for leftovers and lets you review them.").fg(OK));
                self.queue = t;
                return Cmd::Confirm(Confirm {
                    title: "Uninstall".into(),
                    body,
                    yes: "Uninstall".into(),
                    danger: false,
                    tag: TAG_UNINSTALL,
                });
            }
            _ => {}
        }
        Cmd::None
    }

    fn confirmed(&mut self, tag: u32, ctx: &mut Ctx) {
        match tag {
            TAG_UNINSTALL => self.start_uninstall(),
            TAG_AFTER_FOREGROUND => {
                // package manager ran in the terminal; now hunt leftovers and refresh
                let queue = std::mem::take(&mut self.queue);
                let all = self.programs.clone();
                self.job = Some(Job::spawn("Looking for leftovers", move |r| {
                    let now = uninstall::list();
                    for p in &queue {
                        if now.iter().any(|q| q.id == p.id) {
                            r.send(Ev::Done(p.name.clone(), Err("still installed".into())));
                            continue;
                        }
                        r.send(Ev::Done(p.name.clone(), Ok("removed".into())));
                        let rest: Vec<Program> = all.iter().filter(|o| o.id != p.id).cloned().collect();
                        let l = uninstall::scan_leftovers(p, &rest);
                        if !l.is_empty() {
                            r.send(Ev::Leftovers(p.name.clone(), l));
                        }
                    }
                    r.send(Ev::List(now));
                }));
            }
            TAG_REMOVE_LEFTOVERS => {
                if let Some(rv) = self.review.take() {
                    let items: Vec<Leftover> = rv.items.into_iter().zip(rv.sel).filter(|(_, s)| *s).map(|(l, _)| l).collect();
                    let label = rv.program.clone();
                    self.job = Some(Job::spawn("Removing leftovers", move |r| {
                        let (n, b, q) = uninstall::remove_leftovers(&label, &items);
                        r.send(Ev::Removed(n, b, q));
                    }));
                }
            }
            TAG_DELETE_STARTUP => {
                if let Some(item) = self.startup.get(self.slist.sel).cloned() {
                    match startup::delete(&item) {
                        Ok(()) => {
                            if !bfs::dry() {
                                self.startup.remove(self.slist.sel);
                            }
                            ctx.toast(format!("Deleted {}", item.name));
                        }
                        Err(e) => ctx.error(e),
                    }
                }
            }
            _ => {}
        }
    }

    fn click(&mut self, _c: u16, row: u16) {
        if let Some(rv) = self.review.as_mut() {
            rv.list.click(row);
        } else if self.view == 0 {
            self.list.click(row);
        } else {
            self.slist.click(row);
        }
    }

    fn scroll(&mut self, d: i32) {
        if let Some(rv) = self.review.as_mut() {
            rv.list.scroll(d);
        } else if self.view == 0 {
            self.list.scroll(d);
        } else {
            self.slist.scroll(d);
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.review.is_some() {
            return vec![("Space", "toggle"), ("a/n", "all/none"), ("r", "remove selected"), ("Esc", "keep all")];
        }
        if self.view == 1 {
            return vec![("Space", "on/off"), ("d", "delete"), ("r", "reload")];
        }
        vec![
            ("/", "search"),
            ("Space", "mark"),
            ("u", "uninstall"),
            ("U", "with wizard"),
            ("l", "leftovers"),
            ("s", "sort"),
            ("r", "reload"),
        ]
    }
}

/// Linux/macOS package managers that need root run in the real terminal (sudo prompt).
fn foreground_cmd(t: &[Program]) -> Option<Vec<String>> {
    if cfg!(windows) || util::is_admin() {
        return None;
    }
    let ids: Vec<String> = t.iter().map(|p| p.id.split(':').next_back().unwrap_or(&p.id).to_string()).collect();
    let kind = t.first()?.kind;
    if !t.iter().all(|p| p.kind == kind) {
        return None;
    }
    let mut cmd: Vec<String> = match kind {
        Kind::Deb => vec!["sudo", "apt-get", "remove", "-y"],
        Kind::Rpm => vec!["sudo", "dnf", "remove", "-y"],
        Kind::Pacman => vec!["sudo", "pacman", "-R", "--noconfirm"],
        Kind::Snap => vec!["sudo", "snap", "remove"],
        _ => return None,
    }
    .into_iter()
    .map(String::from)
    .collect();
    cmd.extend(ids);
    Some(cmd)
}
