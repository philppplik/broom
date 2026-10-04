//! Doctor tab: findings with one-key fixes, repair toolbox, hardware overview.

use super::list::ListView;
use super::theme::*;
use super::{Cmd, Confirm, Ctx, Job, Tab};
use crate::doctor::{self, Fix, Report, Severity};
use crate::util::{self, Risk};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph, Wrap};
use ratatui::Frame;

enum Ev {
    Report(Report),
    Done(String, Result<String, String>),
}

pub struct DoctorTab {
    view: usize,
    report: Option<Report>,
    list: ListView,
    alist: ListView,
    actions: Vec<doctor::Action>,
    job: Option<Job<Ev>>,
    scanned: bool,
    pending: Option<Fix>,
    hw_scroll: u16,
}

const TAG_FIX: u32 = 1;

fn sev_style(s: Severity) -> Style {
    match s {
        Severity::Critical => Style::new().fg(Color::Black).bg(BAD),
        Severity::Warning => Style::new().fg(Color::Black).bg(WARN),
        Severity::Advice => Style::new().fg(Color::Black).bg(ACCENT),
        Severity::Ok => Style::new().fg(Color::Black).bg(OK),
    }
}

impl DoctorTab {
    pub fn new() -> Self {
        DoctorTab {
            view: 0,
            report: None,
            list: ListView::default(),
            alist: ListView::default(),
            actions: doctor::actions(),
            job: None,
            scanned: false,
            pending: None,
            hw_scroll: 0,
        }
    }

    fn scan(&mut self) {
        if self.job.is_none() {
            self.job = Some(Job::spawn("Examining this PC (hardware, health, security, events)", |r| r.send(Ev::Report(doctor::scan()))));
        }
    }

    fn fix_label(f: &Fix) -> String {
        match f {
            Fix::Tweak(id) => format!(
                "apply tweak '{}'",
                crate::tweaks::catalog().into_iter().find(|t| &t.id == id).map(|t| t.name).unwrap_or(id.clone())
            ),
            Fix::Action(id) => format!("run '{}'", doctor::actions().into_iter().find(|a| a.id == id).map(|a| a.name).unwrap_or("?")),
            Fix::Open(t) => format!("open {t}"),
        }
    }

    fn confirm_fix(&mut self, fix: Fix, ctx: &mut Ctx) -> Cmd {
        match &fix {
            Fix::Open(t) => {
                ctx.switch_to = Some(match t.as_str() {
                    "clean" => super::TAB_CLEAN,
                    "uninstall" | "startup" => super::TAB_UNINSTALL,
                    _ => super::TAB_TWEAKS,
                });
                if t == "startup" {
                    ctx.toast("Uninstall tab: press → for the Startup view");
                } else if t == "dns" {
                    ctx.toast("Tweaks tab: press → to reach the DNS view");
                }
                Cmd::None
            }
            Fix::Action(id) => {
                let Some(a) = self.actions.iter().find(|a| a.id == id) else { return Cmd::None };
                let mut body = vec![Line::from(a.desc.to_string())];
                if a.needs_admin && !ctx.admin {
                    body.push(Line::from("Needs administrator rights - restart Broom as admin.").fg(BAD));
                }
                if a.slow {
                    body.push(Line::from("Takes a while; Broom stays usable meanwhile.").fg(DIM));
                }
                let title = a.name.to_string();
                let danger = a.risk == Risk::Aggressive;
                self.pending = Some(fix);
                Cmd::Confirm(Confirm { title, body, yes: "Run".into(), danger, tag: TAG_FIX })
            }
            Fix::Tweak(_) => {
                let body = vec![
                    Line::from(format!("Broom will {}.", Self::fix_label(&fix))),
                    Line::from("Reversible in Tweaks (u) or Backups.").fg(OK),
                ];
                self.pending = Some(fix);
                Cmd::Confirm(Confirm { title: "Apply fix".into(), body, yes: "Apply".into(), danger: false, tag: TAG_FIX })
            }
        }
    }

    fn render_findings(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [score, main, detail] = Layout::vertical([Constraint::Length(3), Constraint::Min(6), Constraint::Length(6)]).areas(area);
        match &self.report {
            Some(r) => {
                let c = if r.score >= 85 {
                    OK
                } else if r.score >= 65 {
                    WARN
                } else {
                    BAD
                };
                let counts = |s: Severity| r.findings.iter().filter(|x| x.severity == s).count();
                let label = format!(
                    "Health {} / 100   ·   {} critical · {} warnings · {} advice · {} OK",
                    r.score,
                    counts(Severity::Critical),
                    counts(Severity::Warning),
                    counts(Severity::Advice),
                    counts(Severity::Ok)
                );
                f.render_widget(
                    Gauge::default()
                        .ratio(r.score as f64 / 100.0)
                        .label(label)
                        .gauge_style(Style::new().fg(c).bg(Color::Black))
                        .block(block("Health")),
                    score,
                );
            }
            None => f.render_widget(
                Paragraph::new(Line::from(format!("{} scanning...", spinner(ctx.tick))).fg(ACCENT)).block(block("Health")),
                score,
            ),
        }
        let rows: Vec<Line<'static>> = self
            .report
            .as_ref()
            .map(|r| {
                r.findings
                    .iter()
                    .map(|x| {
                        Line::from(vec![
                            Span::styled(format!(" {:<8} ", x.severity.label()), sev_style(x.severity)),
                            Span::styled(format!("  {:<13}", x.area), Style::new().fg(DIM)),
                            Span::styled(util::ellipsize(&x.title, 80), Style::new().fg(Color::White)),
                            if x.fix.is_some() { Span::styled("  ⏎ fix", Style::new().fg(ACCENT2)) } else { Span::raw("") },
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.list.render(f, main, focused_block("Findings"), rows);
        let mut d = Vec::new();
        if let Some(x) = self.report.as_ref().and_then(|r| r.findings.get(self.list.sel)) {
            d.push(Line::from(x.title.clone()).fg(ACCENT).bold());
            d.push(Line::from(x.detail.clone()).fg(TEXT));
            if let Some(fx) = &x.fix {
                d.push(Line::from(format!("Enter: {}", Self::fix_label(fx))).fg(ACCENT2));
            }
        }
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details")), detail);
    }

    fn render_actions(&mut self, f: &mut Frame, area: Rect) {
        let [main, detail] = Layout::vertical([Constraint::Min(6), Constraint::Length(4)]).areas(area);
        let rows: Vec<Line<'static>> = self
            .actions
            .iter()
            .map(|a| {
                Line::from(vec![
                    Span::styled(format!(" {:<48}", a.name), Style::new().fg(Color::White)),
                    risk_span(a.risk),
                    Span::styled(if a.needs_admin { "admin " } else { "      " }, Style::new().fg(DIM)),
                    Span::styled(if a.slow { "slow  " } else { "      " }, Style::new().fg(DIM)),
                    Span::styled(util::ellipsize(a.desc, 70), Style::new().fg(TEXT)),
                ])
            })
            .collect();
        self.alist.render(f, main, focused_block("Repair & maintenance toolbox"), rows);
        let d = self
            .actions
            .get(self.alist.sel)
            .map(|a| vec![Line::from(a.name).fg(ACCENT).bold(), Line::from(a.desc).fg(TEXT)])
            .unwrap_or_default();
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details")), detail);
    }

    fn render_hw(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let hw = self.report.as_ref().map(|r| &r.hardware).or(ctx.hardware.as_ref());
        let mut l: Vec<Line> = Vec::new();
        let kv = |k: &str, v: String| Line::from(vec![Span::styled(format!("  {k:<14}"), Style::new().fg(DIM)), Span::raw(v)]);
        match hw {
            None => l.push(Line::from("  scanning...").fg(DIM)),
            Some(h) => {
                l.push(Line::from("System").fg(ACCENT).bold());
                l.push(kv("Device", format!("{} {}", h.manufacturer, h.model)));
                l.push(kv("Form factor", if h.is_laptop { "Laptop / portable".into() } else { "Desktop".into() }));
                l.push(kv("OS", h.os.clone()));
                l.push(kv("Architecture", h.arch.clone()));
                if !h.firmware.is_empty() {
                    l.push(kv("Firmware", h.firmware.clone()));
                }
                l.push(Line::default());
                l.push(Line::from("Processor & memory").fg(ACCENT).bold());
                l.push(kv("CPU", format!("{} ({} threads)", h.cpu, h.cores)));
                l.push(kv("RAM", format!("{} total, {} free", util::fmt_size(h.ram_total), util::fmt_size(h.ram_available))));
                if let Some((r, c)) = h.ram_speed {
                    l.push(kv("RAM speed", format!("{c} MT/s configured, rated {r}")));
                }
                if let Some((s, t)) = h.ram_slots {
                    l.push(kv("RAM slots", format!("{s} of {t} used")));
                }
                l.push(Line::default());
                l.push(Line::from("Graphics").fg(ACCENT).bold());
                for g in &h.gpus {
                    l.push(kv("GPU", g.clone()));
                }
                l.push(Line::default());
                l.push(Line::from("Storage").fg(ACCENT).bold());
                for d in &h.disks {
                    let mut extra = String::new();
                    if let Some(t) = d.temperature {
                        extra.push_str(&format!(", {t} °C"));
                    }
                    if let Some(w) = d.wear {
                        extra.push_str(&format!(", {w}% worn"));
                    }
                    l.push(kv(&d.media, format!("{} · {} · {}{extra}", d.model, util::fmt_size(d.size), d.health)));
                }
                if let Some(b) = &h.battery {
                    l.push(Line::default());
                    l.push(Line::from("Battery").fg(ACCENT).bold());
                    l.push(kv(
                        "Health",
                        b.health_pct()
                            .map(|p| format!("{p:.0}%  ({} of {} mWh design)", b.full_mwh, b.design_mwh))
                            .unwrap_or("unknown".into()),
                    ));
                    if let Some(c) = b.cycles {
                        l.push(kv("Cycles", c.to_string()));
                    }
                }
                l.push(Line::default());
                l.push(kv("Uptime", format!("{} days {} h", h.uptime_secs / 86400, h.uptime_secs / 3600 % 24)));
            }
        }
        f.render_widget(Paragraph::new(l).scroll((self.hw_scroll, 0)).block(focused_block("Hardware")), area);
    }
}

impl Tab for DoctorTab {
    fn title(&self) -> &'static str {
        "Doctor"
    }

    fn activate(&mut self, _ctx: &mut Ctx) {
        if !self.scanned {
            self.scanned = true;
            self.scan();
        }
    }

    fn busy(&self, tick: usize) -> Option<String> {
        self.job.as_ref().map(|j| j.status(tick))
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        let Some(j) = self.job.as_mut() else { return };
        let evs = j.poll();
        let done = j.is_finished();
        for e in evs {
            match e {
                Ev::Report(r) => {
                    ctx.hardware = Some(r.hardware.clone());
                    ctx.toast(format!("Scan complete - health {} / 100", r.score));
                    self.report = Some(r);
                }
                Ev::Done(n, Ok(m)) => ctx.toast(format!("{n}: {m}")),
                Ev::Done(n, Err(e)) => ctx.error(format!("{n}: {e}")),
            }
        }
        if done {
            self.job = None;
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [tabs, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).areas(area);
        let mut spans = Vec::new();
        for (i, v) in ["Findings", "Repair", "Hardware"].iter().enumerate() {
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
            0 => self.render_findings(f, body, ctx),
            1 => self.render_actions(f, body),
            _ => self.render_hw(f, body, ctx),
        }
    }

    fn sub(&mut self, d: i32, _ctx: &mut Ctx) {
        self.view = (self.view as i32 + d).rem_euclid(3) as usize;
    }

    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd {
        match self.view {
            0 => {
                if self.list.nav(&k) {
                    return Cmd::None;
                }
                match k.code {
                    KeyCode::Char('r') => self.scan(),
                    KeyCode::Enter => {
                        if let Some(fix) = self.report.as_ref().and_then(|r| r.findings.get(self.list.sel)).and_then(|x| x.fix.clone()) {
                            return self.confirm_fix(fix, ctx);
                        }
                    }
                    KeyCode::Char('e') => {
                        if let Some(r) = &self.report {
                            let p = util::sub_dir("Reports").join(format!("doctor-{}.json", util::now_stamp()));
                            match std::fs::write(&p, serde_json::to_string_pretty(r).unwrap_or_default()) {
                                Ok(()) => ctx.toast(format!("Report saved: {}", p.display())),
                                Err(e) => ctx.error(e.to_string()),
                            }
                        }
                    }
                    _ => {}
                }
            }
            1 => {
                if self.alist.nav(&k) {
                    return Cmd::None;
                }
                if k.code == KeyCode::Enter {
                    if let Some(a) = self.actions.get(self.alist.sel) {
                        return self.confirm_fix(Fix::Action(a.id.to_string()), ctx);
                    }
                }
            }
            _ => match k.code {
                KeyCode::Down | KeyCode::Char('j') => self.hw_scroll = self.hw_scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => self.hw_scroll = self.hw_scroll.saturating_sub(1),
                _ => {}
            },
        }
        Cmd::None
    }

    fn confirmed(&mut self, _tag: u32, _ctx: &mut Ctx) {
        let Some(fix) = self.pending.take() else { return };
        let label = Self::fix_label(&fix);
        let rescan = self.view == 0;
        self.job = Some(Job::spawn(&label.clone(), move |r| {
            let res = match &fix {
                Fix::Action(id) => doctor::run_action(id),
                Fix::Tweak(id) => match crate::tweaks::catalog().into_iter().find(|t| &t.id == id) {
                    Some(t) => crate::tweaks::apply(&t).map(|_| "applied".into()),
                    None => Err("tweak not found".into()),
                },
                Fix::Open(_) => Ok(String::new()),
            };
            r.send(Ev::Done(label, res));
            if rescan {
                r.progress(0, 0, "Re-checking");
                r.send(Ev::Report(doctor::scan()));
            }
        }));
    }

    fn click(&mut self, _c: u16, row: u16) {
        match self.view {
            0 => {
                self.list.click(row);
            }
            1 => {
                self.alist.click(row);
            }
            _ => {}
        }
    }

    fn scroll(&mut self, d: i32) {
        match self.view {
            0 => self.list.scroll(d),
            1 => self.alist.scroll(d),
            _ => self.hw_scroll = (self.hw_scroll as i32 + d).max(0) as u16,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.view {
            0 => vec![("Enter", "fix"), ("r", "rescan"), ("e", "export report")],
            1 => vec![("Enter", "run")],
            _ => vec![("↑↓", "scroll")],
        }
    }
}
