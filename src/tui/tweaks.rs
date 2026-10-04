//! Tweaks tab: reversible tweaks, debloat (Windows) and DNS manager.

use super::list::{ListView, Search};
use super::theme::*;
use super::{Cmd, Confirm, Ctx, Job, Tab};
use crate::tweaks::{self, dns, State, Tweak};
use crate::util::{self, Risk};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::collections::HashSet;

enum Ev {
    States(Vec<State>),
    Result(String, Result<(), String>),
    Dns(Vec<dns::Provider>, Vec<String>),
    #[cfg(windows)]
    Bloat(Vec<tweaks::debloat::BloatApp>),
    Msg(Result<String, String>),
}

pub struct TweaksTab {
    view: usize,
    tweaks: Vec<Tweak>,
    states: Vec<State>,
    shown: Vec<usize>,
    marks: HashSet<usize>,
    list: ListView,
    search: Search,
    preset: usize,
    job: Option<Job<Ev>>,
    hinted: bool,
    pending: Vec<usize>,
    undo_mode: bool,
    dns: Vec<dns::Provider>,
    dns_current: Vec<String>,
    dlist: ListView,
    #[cfg(windows)]
    bloat: Vec<tweaks::debloat::BloatApp>,
    #[cfg(windows)]
    bmarks: HashSet<usize>,
    blist: ListView,
    loaded: bool,
}

const TAG_APPLY: u32 = 1;
const TAG_DNS_SET: u32 = 2;
const TAG_DNS_RESET: u32 = 3;
const TAG_DEBLOAT: u32 = 4;

impl TweaksTab {
    pub fn new() -> Self {
        let tweaks = tweaks::catalog();
        let n = tweaks.len();
        let mut t = TweaksTab {
            view: 0,
            states: vec![State::Unknown; n],
            shown: (0..n).collect(),
            tweaks,
            marks: HashSet::new(),
            list: ListView::default(),
            search: Search::default(),
            preset: 0,
            job: None,
            hinted: false,
            pending: vec![],
            undo_mode: false,
            dns: dns::providers(),
            dns_current: vec![],
            dlist: ListView::default(),
            #[cfg(windows)]
            bloat: vec![],
            #[cfg(windows)]
            bmarks: HashSet::new(),
            blist: ListView::default(),
            loaded: false,
        };
        t.refilter();
        t
    }

    fn views(&self) -> Vec<&'static str> {
        if cfg!(windows) {
            vec!["Tweaks", "Debloat", "DNS"]
        } else {
            vec!["Tweaks", "DNS"]
        }
    }

    fn view_name(&self) -> &'static str {
        self.views()[self.view]
    }

    fn refilter(&mut self) {
        self.shown = (0..self.tweaks.len())
            .filter(|i| {
                let t = &self.tweaks[*i];
                self.search.matches(&format!("{} {} {} {}", t.name, t.category, t.desc, t.source))
            })
            .collect();
    }

    fn refresh_states(&mut self) {
        if self.job.is_some() {
            return;
        }
        self.job = Some(Job::spawn("Reading current settings", |r| {
            let all = tweaks::catalog();
            r.send(Ev::States(all.iter().map(tweaks::state).collect()));
            #[cfg(windows)]
            r.send(Ev::Bloat(tweaks::debloat::list()));
            r.send(Ev::Dns(dns::providers(), dns::current()));
        }));
    }

    fn targets(&self) -> Vec<usize> {
        if self.marks.is_empty() {
            self.shown.get(self.list.sel).copied().into_iter().collect()
        } else {
            let mut v: Vec<usize> = self.marks.iter().copied().collect();
            v.sort();
            v
        }
    }

    fn run_tweaks(&mut self, idx: Vec<usize>, undo: bool) {
        let label = if undo { "Undoing tweaks" } else { "Applying tweaks" };
        self.job = Some(Job::spawn(label, move |r| {
            let all = tweaks::catalog();
            #[cfg(windows)]
            if !undo && util::is_admin() {
                r.progress(0, idx.len(), "Creating restore point");
                util::create_restore_point("Broom tweaks");
                crate::util::reg::new_backup_set();
            }
            for (n, i) in idx.iter().enumerate() {
                let t = &all[*i];
                r.progress(n, idx.len(), t.name.clone());
                let res = if undo { tweaks::undo(t) } else { tweaks::apply(t) };
                r.send(Ev::Result(t.name.clone(), res));
            }
            r.send(Ev::States(all.iter().map(tweaks::state).collect()));
        }));
    }

    fn render_tweaks(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [main, detail] = Layout::vertical([Constraint::Min(6), Constraint::Length(7)]).areas(area);
        let rows: Vec<Line<'static>> = self
            .shown
            .iter()
            .map(|i| {
                let t = &self.tweaks[*i];
                let st = match self.states[*i] {
                    State::Applied => Span::styled("● ", Style::new().fg(OK)),
                    State::NotApplied => Span::styled("○ ", Style::new().fg(DIM)),
                    State::Unknown => Span::styled("? ", Style::new().fg(DIM)),
                };
                let hint = match &t.hint {
                    Some(h) if h.starts_with("Recommended") => Span::styled(" ★", Style::new().fg(ACCENT2)),
                    Some(_) => Span::styled(" ⚠", Style::new().fg(WARN)),
                    None => Span::raw("  "),
                };
                Line::from(vec![
                    checkbox(self.marks.contains(i)),
                    st,
                    Span::styled(format!("{:<58}", util::ellipsize(&t.name, 57)), Style::new().fg(Color::White)),
                    hint,
                    Span::styled(format!(" {:<22}", t.category), Style::new().fg(DIM)),
                    risk_span(t.risk),
                    Span::styled(format!("{:<8}", t.source), Style::new().fg(if t.source == "Broom" { ACCENT } else { Color::Magenta })),
                    if t.restart { Span::styled("restart", Style::new().fg(DIM)) } else { Span::raw("") },
                ])
            })
            .collect();
        let applied = self.states.iter().filter(|s| **s == State::Applied).count();
        let presets = tweaks::presets();
        let preset = presets.get(self.preset.wrapping_sub(1)).map(|p| format!(" · preset: {}", p.0)).unwrap_or_default();
        let title = format!(
            "Tweaks · {} total · {applied} active · {} marked{preset}{}",
            self.tweaks.len(),
            self.marks.len(),
            self.search.title_suffix()
        );
        self.list.render(f, main, focused_block(&title), rows);
        let mut d = Vec::new();
        if let Some(t) = self.shown.get(self.list.sel).map(|i| &self.tweaks[*i]) {
            d.push(Line::from(vec![
                Span::styled(t.name.clone(), Style::new().fg(ACCENT).bold()),
                Span::styled(format!("   {} · source: {}", t.category, t.source), Style::new().fg(DIM)),
            ]));
            d.push(Line::from(t.desc.clone()).fg(TEXT));
            if let Some(h) = &t.hint {
                d.push(Line::from(format!("For this PC: {h}")).fg(if h.starts_with("Recommended") { ACCENT2 } else { WARN }));
            }
            d.push(match t.undo_kind() {
                tweaks::Undo::Exact => Line::from("Undo: exact - previous values are journaled and restored").fg(OK),
                tweaks::Undo::Script => {
                    Line::from("Undo: via undo script - restores the default, not necessarily your old setting").fg(WARN)
                }
                tweaks::Undo::None => {
                    Line::from("Undo: NONE - one-way change (e.g. removes software). Reinstall manually to go back.").fg(BAD)
                }
            });
            d.push(
                Line::from(format!("{} change(s){}", t.apply.len(), t.link.as_ref().map(|l| format!(" · docs: {l}")).unwrap_or_default()))
                    .fg(DIM),
            );
        }
        let _ = ctx;
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details  (i: show every change)")), detail);
    }

    #[cfg(windows)]
    fn render_bloat(&mut self, f: &mut Frame, area: Rect) {
        let [main, detail] = Layout::vertical([Constraint::Min(6), Constraint::Length(5)]).areas(area);
        let rows: Vec<Line<'static>> = self
            .bloat
            .iter()
            .enumerate()
            .map(|(i, b)| {
                Line::from(vec![
                    checkbox(self.bmarks.contains(&i)),
                    if b.installed {
                        Span::styled("installed  ", Style::new().fg(WARN))
                    } else {
                        Span::styled("removed    ", Style::new().fg(DIM))
                    },
                    Span::styled(
                        format!("{:<34}", util::ellipsize(&b.name, 33)),
                        Style::new().fg(if b.installed { Color::White } else { DIM }),
                    ),
                    Span::styled(format!("{:<20}", b.category), Style::new().fg(DIM)),
                    Span::styled(util::ellipsize(&b.desc, 80), Style::new().fg(TEXT)),
                ])
            })
            .collect();
        let inst = self.bloat.iter().filter(|b| b.installed).count();
        self.blist.render(
            f,
            main,
            focused_block(&format!("Debloat · {inst} of {} preinstalled apps present · list by winutil", self.bloat.len())),
            rows,
        );
        let mut d = Vec::new();
        if let Some(b) = self.bloat.get(self.blist.sel) {
            d.push(Line::from(format!("{}  ({})", b.name, b.package)).fg(ACCENT).bold());
            d.push(
                Line::from(format!("Removed for all users and de-provisioned. Reinstall anytime with r (Store ID {}).", b.store_id))
                    .fg(DIM),
            );
        }
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details")), detail);
    }

    fn render_dns(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [info, main] = Layout::vertical([Constraint::Length(3), Constraint::Min(6)]).areas(area);
        let cur = if self.dns_current.is_empty() { "unknown".into() } else { self.dns_current.join(", ") };
        let known = self
            .dns
            .iter()
            .find(|p| self.dns_current.contains(&p.primary))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "your router / ISP".into());
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![Span::styled("In use: ", Style::new().fg(DIM)), Span::styled(format!("{cur}  ({known})"), Style::new().fg(Color::White).bold())]),
                Line::from("b benchmarks every provider with real DNS queries from this PC. Enter switches to the selected one, d goes back to automatic.").fg(DIM),
            ])
            .block(block("DNS")),
            info,
        );
        let best = self.dns.iter().filter_map(|p| p.latency).fold(f64::MAX, f64::min);
        let rows: Vec<Line<'static>> = self
            .dns
            .iter()
            .map(|p| {
                let lat = match p.latency {
                    Some(l) => Span::styled(
                        format!("{:>8.1} ms ", l),
                        Style::new().fg(if (l - best).abs() < 0.01 {
                            OK
                        } else if l < 40.0 {
                            TEXT
                        } else {
                            WARN
                        }),
                    ),
                    None => Span::styled(format!("{:>11} ", "-"), Style::new().fg(DIM)),
                };
                let inuse = self.dns_current.contains(&p.primary);
                Line::from(vec![
                    if inuse { Span::styled(" ● ", Style::new().fg(OK)) } else { Span::raw("   ") },
                    Span::styled(format!("{:<36}", p.name), Style::new().fg(Color::White)),
                    lat,
                    Span::styled(format!("{:<18}{:<18}", p.primary, p.secondary), Style::new().fg(DIM)),
                    if p.latency.is_some() && (p.latency.unwrap() - best).abs() < 0.01 {
                        Span::styled(" fastest", Style::new().fg(OK).bold())
                    } else {
                        Span::raw("")
                    },
                ])
            })
            .collect();
        let _ = ctx;
        self.dlist.render(f, main, focused_block("Providers (list from winutil)"), rows);
    }
}

impl Tab for TweaksTab {
    fn title(&self) -> &'static str {
        "Tweaks"
    }

    fn activate(&mut self, ctx: &mut Ctx) {
        if !self.loaded {
            self.loaded = true;
            self.refresh_states();
        }
        if !self.hinted {
            if let Some(hw) = &ctx.hardware {
                tweaks::recommend(&mut self.tweaks, hw);
                self.hinted = true;
            }
        }
    }

    fn typing(&self) -> bool {
        self.search.active
    }

    fn busy(&self, tick: usize) -> Option<String> {
        self.job.as_ref().map(|j| j.status(tick))
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        if !self.hinted {
            if let Some(hw) = &ctx.hardware {
                tweaks::recommend(&mut self.tweaks, hw);
                self.hinted = true;
            }
        }
        let Some(j) = self.job.as_mut() else { return };
        let evs = j.poll();
        let done = j.is_finished();
        let mut ok = 0;
        let mut errs = Vec::new();
        for e in evs {
            match e {
                Ev::States(s) => {
                    if s.len() == self.states.len() {
                        self.states = s;
                    }
                }
                Ev::Result(_, Ok(())) => ok += 1,
                Ev::Result(n, Err(e)) => errs.push(format!("{n}: {e}")),
                Ev::Dns(p, c) => {
                    if self.dns.iter().all(|x| x.latency.is_none()) {
                        self.dns = p;
                    }
                    self.dns_current = c;
                }
                #[cfg(windows)]
                Ev::Bloat(b) => self.bloat = b,
                Ev::Msg(Ok(m)) => ctx.toast(m),
                Ev::Msg(Err(e)) => ctx.error(e),
            }
        }
        if ok > 0 {
            ctx.toast(format!("{ok} done{}", if errs.is_empty() { String::new() } else { format!(", {} failed", errs.len()) }));
        }
        if let Some(e) = errs.first() {
            ctx.error(e.clone());
        }
        if done {
            self.job = None;
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [tabs, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).areas(area);
        let mut spans = Vec::new();
        for (i, v) in self.views().iter().enumerate() {
            spans.push(if i == self.view {
                Span::styled(format!(" {v} "), Style::new().fg(Color::Black).bg(ACCENT))
            } else {
                Span::styled(format!(" {v} "), Style::new().fg(TEXT))
            });
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("  ← → switch view", Style::new().fg(DIM)));
        f.render_widget(Paragraph::new(Line::from(spans)), tabs);
        match self.view_name() {
            "Tweaks" => self.render_tweaks(f, body, ctx),
            #[cfg(windows)]
            "Debloat" => self.render_bloat(f, body),
            _ => self.render_dns(f, body, ctx),
        }
    }

    fn sub(&mut self, d: i32, _ctx: &mut Ctx) {
        let n = self.views().len() as i32;
        self.view = (self.view as i32 + d).rem_euclid(n) as usize;
    }

    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd {
        if self.search.active {
            if self.search.key(&k) {
                self.refilter();
                self.list.sel = 0;
            }
            return Cmd::None;
        }
        match self.view_name() {
            "Tweaks" => {
                if self.list.nav(&k) {
                    return Cmd::None;
                }
                match k.code {
                    KeyCode::Char('/') => self.search.active = true,
                    KeyCode::Esc => {
                        self.marks.clear();
                        self.search.text.clear();
                        self.preset = 0;
                        self.refilter();
                    }
                    KeyCode::Char(' ') => {
                        if let Some(i) = self.shown.get(self.list.sel).copied() {
                            if !self.marks.remove(&i) {
                                self.marks.insert(i);
                            }
                        }
                        self.list.move_by(1);
                    }
                    KeyCode::Char('p') => {
                        let presets = tweaks::presets();
                        self.preset = (self.preset + 1) % (presets.len() + 1);
                        self.marks.clear();
                        if let Some((name, desc, ids)) = presets.get(self.preset.wrapping_sub(1)) {
                            for (i, t) in self.tweaks.iter().enumerate() {
                                if ids.contains(&t.id) {
                                    self.marks.insert(i);
                                }
                            }
                            ctx.toast(format!("Preset {name}: {desc} ({} tweaks marked) - a applies", self.marks.len()));
                        } else {
                            ctx.toast("Preset cleared");
                        }
                    }
                    KeyCode::Char('i') => {
                        if let Some(t) = self.shown.get(self.list.sel).map(|i| &self.tweaks[*i]) {
                            let mut l = vec![Line::from(t.desc.clone()), Line::default(), Line::from("Changes:").fg(ACCENT).bold()];
                            for op in &t.apply {
                                for line in format!("{op:?}").lines().take(12) {
                                    l.push(Line::from(format!("  {}", util::ellipsize(line, 160))).fg(TEXT));
                                }
                            }
                            if !t.undo.is_empty() {
                                l.push(Line::default());
                                l.push(Line::from("Fallback undo (used only if Broom has no journal entry):").fg(ACCENT).bold());
                                for op in &t.undo {
                                    l.push(Line::from(format!("  {}", util::ellipsize(&format!("{op:?}"), 160))).fg(DIM));
                                }
                            }
                            return Cmd::Info(t.name.clone(), l);
                        }
                    }
                    KeyCode::Char('a') | KeyCode::Enter | KeyCode::Char('u') => {
                        if self.job.is_some() {
                            return Cmd::None;
                        }
                        let undo = k.code == KeyCode::Char('u');
                        let t = self.targets();
                        if t.is_empty() {
                            return Cmd::None;
                        }
                        let aggressive = t.iter().filter(|i| self.tweaks[**i].risk == Risk::Aggressive).count();
                        let restart = t.iter().any(|i| self.tweaks[*i].restart);
                        let mut body = vec![Line::from(format!("{} {} tweak(s):", if undo { "Undo" } else { "Apply" }, t.len()))];
                        for i in t.iter().take(12) {
                            body.push(Line::from(vec![
                                Span::raw(format!("  • {} ", self.tweaks[*i].name)),
                                risk_span(self.tweaks[*i].risk),
                            ]));
                        }
                        if t.len() > 12 {
                            body.push(Line::from(format!("  ... and {} more", t.len() - 12)).fg(DIM));
                        }
                        body.push(Line::default());
                        if !undo {
                            body.push(Line::from("✓ previous values are journaled where possible - undo with u or in Backups").fg(OK));
                            if cfg!(windows) {
                                body.push(Line::from("✓ restore point + registry export first").fg(OK));
                            }
                        }
                        let one_way: Vec<&str> = t
                            .iter()
                            .filter(|i| self.tweaks[**i].undo_kind() == tweaks::Undo::None)
                            .map(|i| self.tweaks[*i].name.as_str())
                            .collect();
                        if !undo && !one_way.is_empty() {
                            body.push(Line::from("One-way - cannot be undone by Broom:").fg(BAD).bold());
                            for n in one_way.iter().take(5) {
                                body.push(Line::from(format!("  ! {n}")).fg(BAD));
                            }
                        }
                        if restart {
                            body.push(Line::from("Some changes need a sign-out or restart.").fg(WARN));
                        }
                        let danger_one_way = !undo && !one_way.is_empty();
                        self.pending = t;
                        self.undo_mode = undo;
                        return Cmd::Confirm(Confirm {
                            title: if undo { "Undo tweaks".into() } else { "Apply tweaks".into() },
                            body,
                            yes: if undo { "Undo".into() } else { "Apply".into() },
                            danger: (aggressive > 0 && !undo) || danger_one_way,
                            tag: TAG_APPLY,
                        });
                    }
                    KeyCode::Char('r') => self.refresh_states(),
                    _ => {}
                }
            }
            #[cfg(windows)]
            "Debloat" => {
                if self.blist.nav(&k) {
                    return Cmd::None;
                }
                match k.code {
                    KeyCode::Char(' ') => {
                        let i = self.blist.sel;
                        if !self.bmarks.remove(&i) {
                            self.bmarks.insert(i);
                        }
                        self.blist.move_by(1);
                    }
                    KeyCode::Char('a') => {
                        self.bmarks = (0..self.bloat.len()).filter(|i| self.bloat[*i].installed).collect();
                    }
                    KeyCode::Char('n') | KeyCode::Esc => self.bmarks.clear(),
                    KeyCode::Enter | KeyCode::Char('x') => {
                        let t: Vec<usize> =
                            if self.bmarks.is_empty() { vec![self.blist.sel] } else { self.bmarks.iter().copied().collect() };
                        let names: Vec<String> =
                            t.iter().filter_map(|i| self.bloat.get(*i)).filter(|b| b.installed).map(|b| b.name.clone()).collect();
                        if names.is_empty() {
                            return Cmd::None;
                        }
                        let mut body = vec![Line::from(format!("Remove {} app(s) for all users:", names.len()))];
                        for n in names.iter().take(14) {
                            body.push(Line::from(format!("  • {n}")));
                        }
                        body.push(Line::default());
                        body.push(Line::from("Reinstall any of them later with r (Microsoft Store).").fg(OK));
                        self.pending = t;
                        return Cmd::Confirm(Confirm {
                            title: "Debloat".into(),
                            body,
                            yes: "Remove".into(),
                            danger: false,
                            tag: TAG_DEBLOAT,
                        });
                    }
                    KeyCode::Char('r') => {
                        if let Some(b) = self.bloat.get(self.blist.sel).cloned() {
                            self.job = Some(Job::spawn(&format!("Reinstalling {}", b.name), move |r| {
                                r.send(Ev::Msg(tweaks::debloat::reinstall(&b).map(|_| format!("{} reinstalled", b.name))));
                                r.send(Ev::Bloat(tweaks::debloat::list()));
                            }));
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                if self.dlist.nav(&k) {
                    return Cmd::None;
                }
                match k.code {
                    KeyCode::Char('b') => {
                        if self.job.is_none() {
                            let mut p = self.dns.clone();
                            self.job = Some(Job::spawn("Benchmarking DNS providers", move |r| {
                                dns::benchmark(&mut p);
                                r.send(Ev::Dns(p, dns::current()));
                            }));
                            self.dns.iter_mut().for_each(|p| p.latency = None);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(p) = self.dns.get(self.dlist.sel) {
                            return Cmd::Confirm(Confirm {
                                title: "Change DNS".into(),
                                body: vec![
                                    Line::from(format!("Use {} ({} / {}) on all active network adapters?", p.name, p.primary, p.secondary)),
                                    Line::from("d switches back to automatic at any time.").fg(DIM),
                                ],
                                yes: "Switch".into(),
                                danger: false,
                                tag: TAG_DNS_SET,
                            });
                        }
                    }
                    KeyCode::Char('d') => {
                        return Cmd::Confirm(Confirm {
                            title: "Automatic DNS".into(),
                            body: vec![Line::from("Go back to the DNS servers your router/ISP provides?")],
                            yes: "Reset".into(),
                            danger: false,
                            tag: TAG_DNS_RESET,
                        });
                    }
                    _ => {}
                }
            }
        }
        Cmd::None
    }

    fn confirmed(&mut self, tag: u32, _ctx: &mut Ctx) {
        match tag {
            TAG_APPLY => {
                let p = std::mem::take(&mut self.pending);
                self.marks.clear();
                self.run_tweaks(p, self.undo_mode);
            }
            TAG_DNS_SET | TAG_DNS_RESET => {
                let p = if tag == TAG_DNS_SET { self.dns.get(self.dlist.sel).cloned() } else { None };
                self.job = Some(Job::spawn("Changing DNS", move |r| {
                    r.send(Ev::Msg(dns::set(p.as_ref())));
                    r.send(Ev::Dns(dns::providers(), dns::current()));
                }));
            }
            #[cfg(windows)]
            TAG_DEBLOAT => {
                let apps: Vec<tweaks::debloat::BloatApp> =
                    self.pending.iter().filter_map(|i| self.bloat.get(*i).cloned()).filter(|b| b.installed).collect();
                self.bmarks.clear();
                self.job = Some(Job::spawn("Removing apps", move |r| {
                    for (n, a) in apps.iter().enumerate() {
                        r.progress(n, apps.len(), a.name.clone());
                        r.send(Ev::Result(a.name.clone(), tweaks::debloat::remove(a)));
                    }
                    r.send(Ev::Bloat(tweaks::debloat::list()));
                }));
            }
            _ => {}
        }
    }

    fn click(&mut self, _c: u16, row: u16) {
        match self.view_name() {
            "Tweaks" => {
                self.list.click(row);
            }
            "DNS" => {
                self.dlist.click(row);
            }
            _ => {
                self.blist.click(row);
            }
        }
    }

    fn scroll(&mut self, d: i32) {
        match self.view_name() {
            "Tweaks" => self.list.scroll(d),
            "DNS" => self.dlist.scroll(d),
            _ => self.blist.scroll(d),
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.view_name() {
            "Tweaks" => vec![("/", "search"), ("Space", "mark"), ("p", "preset"), ("a", "apply"), ("u", "undo"), ("i", "details")],
            "Debloat" => vec![("Space", "mark"), ("a", "all installed"), ("Enter", "remove"), ("r", "reinstall")],
            _ => vec![("b", "benchmark"), ("Enter", "use"), ("d", "automatic")],
        }
    }
}
