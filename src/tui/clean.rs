//! Clean tab: analyze (read-only) then sweep.

use super::list::ListView;
use super::theme::*;
use super::{Cmd, Confirm, Ctx, Job, Tab};
use crate::clean::{self, Item, Outcome, TIERS};
use crate::util::{self, fs as bfs, Risk};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Analyze,
    Clean,
}

enum Ev {
    Item(usize, Outcome),
    RestorePoint(bool),
}

pub struct CleanTab {
    items: Vec<Item>,
    sel: Vec<bool>,
    res: Vec<Option<Outcome>>,
    running_idx: Option<usize>,
    phase: Phase,
    last_run: Option<Phase>,
    job: Option<Job<Ev>>,
    list: ListView,
    tier: usize,
    analyzed_once: bool,
    kill: Vec<String>,
    restore_point: Option<bool>,
}

const TAG_CLEAN: u32 = 1;
const TAG_CLOSE_AND_CLEAN: u32 = 2;

impl CleanTab {
    pub fn new() -> Self {
        let items = clean::catalog();
        let n = items.len();
        let mut t = CleanTab {
            sel: vec![false; n],
            res: vec![None; n],
            items,
            running_idx: None,
            phase: Phase::Analyze,
            last_run: None,
            job: None,
            list: ListView::default(),
            tier: 1,
            analyzed_once: false,
            kill: vec![],
            restore_point: None,
        };
        t.preset(1);
        t
    }

    fn preset(&mut self, tier: usize) {
        self.tier = tier;
        for (i, it) in self.items.iter().enumerate() {
            self.sel[i] = (it.tier as usize) <= tier + 1;
        }
    }

    fn start(&mut self, phase: Phase, ctx: &mut Ctx) {
        if self.job.is_some() {
            return;
        }
        let idx: Vec<usize> = if phase == Phase::Analyze {
            (0..self.items.len()).collect() // analysis covers everything so sizes show for unticked items too
        } else {
            (0..self.items.len()).filter(|i| self.sel[*i]).collect()
        };
        if idx.is_empty() {
            ctx.error("Nothing selected");
            return;
        }
        for i in &idx {
            self.res[*i] = None;
        }
        self.phase = phase;
        self.restore_point = None;
        let kill = std::mem::take(&mut self.kill);
        let label = if phase == Phase::Analyze { "Analyzing (read-only)" } else { "Cleaning" };
        self.job = Some(Job::spawn(label, move |r| {
            if phase == Phase::Analyze {
                bfs::set_thread_dry(true);
            } else {
                #[cfg(windows)]
                crate::util::reg::new_backup_set();
                if !kill.is_empty() {
                    r.progress(0, idx.len(), format!("Closing {}", kill.join(", ")));
                    clean::kill(&kill);
                }
                if cfg!(windows) && util::is_admin() {
                    r.progress(0, idx.len(), "Creating restore point");
                    r.send(Ev::RestorePoint(util::create_restore_point("Broom clean")));
                }
            }
            let items = clean::catalog();
            for (n, i) in idx.iter().enumerate() {
                r.progress(n, idx.len(), items[*i].name);
                let o = clean::execute(&items[*i]);
                r.send(Ev::Item(*i, o));
            }
            r.progress(idx.len(), idx.len(), "done");
        }));
    }

    fn total(&self, only_selected: bool) -> (u64, u64) {
        let mut b = 0;
        let mut f = 0;
        for (i, r) in self.res.iter().enumerate() {
            if let Some(o) = r {
                if !only_selected || self.sel[i] {
                    b += o.bytes;
                    f += o.files;
                }
            }
        }
        (b, f)
    }

    fn summary(&self) -> Vec<Line<'static>> {
        let (b, f) = self.total(true);
        let mut l = vec![
            Line::from(vec![
                Span::styled("  Freed ", Style::new().fg(TEXT)),
                Span::styled(util::fmt_size(b), Style::new().fg(OK).bold()),
                Span::styled(format!("   ·   {f} files"), Style::new().fg(TEXT)),
            ]),
            Line::default(),
        ];
        let mut top: Vec<(u64, &str)> = self
            .res
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.as_ref().filter(|_| self.sel[i]).map(|o| (o.bytes, self.items[i].name)))
            .filter(|x| x.0 > 0)
            .collect();
        top.sort_by(|a, b| b.0.cmp(&a.0));
        let max = top.first().map(|t| t.0).unwrap_or(1) as f64;
        for (bytes, name) in top.iter().take(8) {
            l.push(Line::from(vec![
                Span::styled(format!("  {:>10}  ", util::fmt_size(*bytes)), Style::new().fg(OK)),
                Span::styled(format!("{:<24}", "█".repeat(((*bytes as f64 / max) * 24.0).ceil() as usize)), Style::new().fg(ACCENT)),
                Span::raw(format!(" {name}")),
            ]));
        }
        l.push(Line::default());
        match self.restore_point {
            Some(true) => l.push(Line::from("  ✓ Restore point created before cleaning").fg(OK)),
            Some(false) => l.push(Line::from("  ! Restore point could not be created (System Protection off?)").fg(WARN)),
            None => {}
        }
        #[cfg(windows)]
        if let Some(d) = crate::util::reg::current_backup_dir() {
            l.push(Line::from(format!("  ✓ Registry backup: {}", d.display())).fg(OK));
        }
        l.push(Line::from(format!("  Log: {}", util::log_path().display())).fg(DIM));
        l.push(Line::from("  Tip: restart Windows to finish WinSxS, hibernation and locked files.").fg(DIM));
        l
    }
}

impl Tab for CleanTab {
    fn title(&self) -> &'static str {
        "Clean"
    }

    fn activate(&mut self, ctx: &mut Ctx) {
        if !self.analyzed_once {
            self.analyzed_once = true;
            self.start(Phase::Analyze, ctx);
        }
    }

    fn busy(&self, tick: usize) -> Option<String> {
        self.job.as_ref().map(|j| j.status(tick))
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        let Some(j) = self.job.as_mut() else { return };
        for ev in j.poll() {
            match ev {
                Ev::Item(i, o) => self.res[i] = Some(o),
                Ev::RestorePoint(ok) => self.restore_point = Some(ok),
            }
        }
        self.running_idx = self.items.iter().position(|it| it.name == j.label);
        if j.is_finished() {
            self.job = None;
            self.running_idx = None;
            self.last_run = Some(self.phase);
            let (b, _) = self.total(true);
            if self.phase == Phase::Clean {
                ctx.toast(format!("Done - freed {}", util::fmt_size(b)));
            } else {
                ctx.toast(format!("Analysis done - {} can be freed with the current selection", util::fmt_size(b)));
            }
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [main, detail, gauge] = Layout::vertical([Constraint::Min(6), Constraint::Length(5), Constraint::Length(1)]).areas(area);
        let busy_idx = self.running_idx;
        let rows: Vec<Line<'static>> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let res = match (&self.res[i], busy_idx == Some(i)) {
                    (_, true) => Span::styled(format!("{:>10}", spinner(ctx.tick)), Style::new().fg(ACCENT)),
                    (Some(o), _) => size_span(o.bytes, true),
                    (None, _) => Span::styled(format!("{:>10}", "·"), Style::new().fg(DIM)),
                };
                let note = self.res[i].as_ref().and_then(|o| o.note.clone()).unwrap_or_default();
                Line::from(vec![
                    checkbox(self.sel[i]),
                    Span::styled(
                        format!("{:<56}", util::ellipsize(it.name, 55)),
                        Style::new().fg(if self.sel[i] { ratatui::style::Color::White } else { TEXT }),
                    ),
                    Span::styled(format!("{:<21}", it.category), Style::new().fg(DIM)),
                    risk_span(it.risk),
                    res,
                    Span::styled(format!("  {}", util::ellipsize(&note, 34)), Style::new().fg(DIM)),
                ])
            })
            .collect();
        let (sel_b, _) = self.total(true);
        let n_sel = self.sel.iter().filter(|s| **s).count();
        let title = format!(
            "Clean · preset {} · {} of {} selected · {} {}",
            TIERS[self.tier],
            n_sel,
            self.items.len(),
            util::fmt_size(sel_b),
            if self.last_run == Some(Phase::Clean) { "freed" } else { "to free" }
        );
        self.list.render(f, main, focused_block(&title), rows);

        let it = &self.items[self.list.sel.min(self.items.len() - 1)];
        let mut d = vec![Line::from(vec![
            Span::styled(it.name, Style::new().fg(ACCENT).bold()),
            Span::styled(format!("   tier {} · ", TIERS[it.tier as usize - 1]), Style::new().fg(DIM)),
            risk_span(it.risk),
        ])];
        d.push(Line::from(it.desc).fg(TEXT));
        if !it.procs.is_empty() {
            d.push(Line::from(format!("Closes for best results: {}", it.procs.join(", "))).fg(DIM));
        }
        f.render_widget(Paragraph::new(d).wrap(Wrap { trim: true }).block(block("Details")), detail);

        if let Some(j) = &self.job {
            let ratio = if j.total > 0 { j.done as f64 / j.total as f64 } else { 0.0 };
            f.render_widget(
                Gauge::default()
                    .ratio(ratio)
                    .gauge_style(Style::new().fg(ACCENT).bg(ratatui::style::Color::Black))
                    .label(format!("{} {}/{}", j.label, j.done, j.total)),
                gauge,
            );
        } else {
            let msg = if self.last_run.is_some() {
                "Enter re-analyzes · r cleans the selection"
            } else {
                "Analysis is read-only. Nothing is deleted until you press r."
            };
            f.render_widget(Paragraph::new(Line::from(msg).fg(DIM)), gauge);
        }
    }

    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd {
        if self.list.nav(&k) {
            return Cmd::None;
        }
        let i = self.list.sel;
        match k.code {
            KeyCode::Char(' ') => self.sel[i] = !self.sel[i],
            KeyCode::Char('a') => self.sel.iter_mut().for_each(|s| *s = true),
            KeyCode::Char('n') => self.sel.iter_mut().for_each(|s| *s = false),
            KeyCode::Char('p') => self.preset((self.tier + 1) % 3),
            KeyCode::Enter => self.start(Phase::Analyze, ctx),
            KeyCode::Char('r') | KeyCode::Char('c') => {
                if self.job.is_some() {
                    return Cmd::None;
                }
                let chosen: Vec<&Item> = self.items.iter().enumerate().filter(|(i, _)| self.sel[*i]).map(|(_, x)| x).collect();
                if chosen.is_empty() {
                    ctx.error("Nothing selected");
                    return Cmd::None;
                }
                let procs: Vec<&str> = chosen.iter().flat_map(|c| c.procs.iter().copied()).collect();
                let running = clean::running(&procs);
                let aggressive: Vec<&str> = chosen.iter().filter(|c| c.risk == Risk::Aggressive).map(|c| c.name).collect();
                let (b, _) = self.total(true);
                let mut body = vec![
                    Line::from(format!(
                        "Sweep {} items{}.",
                        chosen.len(),
                        if b > 0 { format!(" (~{} found by the analysis)", util::fmt_size(b)) } else { String::new() }
                    )),
                    Line::default(),
                ];
                if cfg!(windows) {
                    body.push(Line::from("  ✓ restore point first   ✓ registry exported to .reg first").fg(OK));
                }
                body.push(Line::from("  ✓ Documents, Downloads, Pictures, OneDrive are never touched").fg(OK));
                if !aggressive.is_empty() {
                    body.push(Line::default());
                    body.push(Line::from("No way back for:").fg(BAD).bold());
                    for a in aggressive.iter().take(6) {
                        body.push(Line::from(format!("  ! {a}")).fg(BAD));
                    }
                }
                if !running.is_empty() {
                    body.push(Line::default());
                    body.push(Line::from(format!("Running apps lock their caches: {}", running.join(", "))).fg(WARN));
                    body.push(
                        Line::from("Y closes them first (unsaved work in them is lost). Esc and close them yourself to be safe.").fg(WARN),
                    );
                    self.kill = running;
                    return Cmd::Confirm(Confirm {
                        title: "Clean".into(),
                        body,
                        yes: "Close apps & clean".into(),
                        danger: !aggressive.is_empty(),
                        tag: TAG_CLOSE_AND_CLEAN,
                    });
                }
                return Cmd::Confirm(Confirm {
                    title: "Clean".into(),
                    body,
                    yes: "Clean now".into(),
                    danger: !aggressive.is_empty(),
                    tag: TAG_CLEAN,
                });
            }
            KeyCode::Char('s') if self.last_run.is_some() => {
                return Cmd::Info("Summary".into(), self.summary());
            }
            _ => {}
        }
        Cmd::None
    }

    fn confirmed(&mut self, tag: u32, ctx: &mut Ctx) {
        if tag != TAG_CLOSE_AND_CLEAN {
            self.kill.clear();
        }
        self.start(Phase::Clean, ctx);
    }

    fn click(&mut self, _c: u16, row: u16) {
        let before = self.list.sel;
        if self.list.click(row) && before == self.list.sel {
            self.sel[before] = !self.sel[before];
        }
    }

    fn scroll(&mut self, d: i32) {
        self.list.scroll(d);
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Space", "toggle"), ("a/n", "all/none"), ("p", "preset"), ("Enter", "analyze"), ("r", "clean"), ("s", "summary")]
    }
}
