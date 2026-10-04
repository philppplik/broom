//! Home: logo, system dashboard and quick actions.

use super::list::ListView;
use super::theme::*;
use super::{Cmd, Ctx, Job, Tab};
use crate::util;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const ACTIONS: [(&str, &str, usize); 5] = [
    ("Clean", "See how much junk can go - analysis is read-only", super::TAB_CLEAN),
    ("Uninstall", "Biggest programs first, with leftovers hunting", super::TAB_UNINSTALL),
    ("Doctor", "Health check with fixes tailored to this hardware", super::TAB_DOCTOR),
    ("Tweaks", "Privacy, performance and gaming tweaks - all reversible", super::TAB_TWEAKS),
    ("Backups", "Undo tweaks, restore registry backups and quarantined files", super::TAB_BACKUPS),
];

pub struct Home {
    list: ListView,
    job: Option<Job<crate::doctor::Hardware>>,
}

impl Home {
    pub fn new() -> Self {
        Home { list: ListView::default(), job: None }
    }
}

impl Tab for Home {
    fn title(&self) -> &'static str {
        "Home"
    }

    fn activate(&mut self, ctx: &mut Ctx) {
        if ctx.hardware.is_none() && self.job.is_none() {
            self.job = Some(Job::spawn("Reading hardware", |r| r.send(crate::doctor::hardware())));
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        if let Some(j) = self.job.as_mut() {
            if let Some(hw) = j.poll().pop() {
                ctx.hardware = Some(hw);
            }
            if j.is_finished() {
                self.job = None;
            }
        }
    }

    fn busy(&self, tick: usize) -> Option<String> {
        self.job.as_ref().map(|j| j.status(tick))
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let [top, mid, safety, bottom] =
            Layout::vertical([Constraint::Length(8), Constraint::Length(12), Constraint::Min(0), Constraint::Length(2)]).areas(area);
        // logo + tagline
        let mut l = vec![Line::default()];
        for line in logo_lines() {
            let mut sp = vec![Span::raw("  ")];
            sp.extend(line.spans);
            l.push(Line::from(sp));
        }
        l.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Clean · Uninstall · Tweak · Diagnose", Style::new().fg(ACCENT2).bold()),
            Span::styled("   - every change backed up, one-way actions clearly marked", Style::new().fg(DIM)),
        ]));
        f.render_widget(Paragraph::new(l), top);

        let [left, right] = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(mid);
        // system panel
        let mut s: Vec<Line> = Vec::new();
        let row = |k: &str, v: String| Line::from(vec![Span::styled(format!("{k:<10}"), Style::new().fg(DIM)), Span::raw(v)]);
        if let Some(hw) = &ctx.hardware {
            if !hw.model.is_empty() {
                let first = hw.manufacturer.split_whitespace().next().unwrap_or("");
                let device = if !first.is_empty() && hw.model.starts_with(first) {
                    hw.model.clone()
                } else {
                    format!("{} {}", hw.manufacturer, hw.model)
                };
                s.push(row("Device", format!("{device}{}", if hw.is_laptop { " (laptop)" } else { "" })));
            }
            s.push(row("CPU", format!("{} · {} threads", hw.cpu, hw.cores)));
            if !hw.gpus.is_empty() {
                s.push(row("GPU", hw.gpus.join(", ")));
            }
            if let Some(b) = hw.battery.as_ref().and_then(|b| b.health_pct()) {
                s.push(row("Battery", format!("{b:.0}% health")));
            }
        } else {
            s.push(row("CPU", util::sys::cpu_name()));
            s.push(Line::from(Span::styled("          reading hardware...", Style::new().fg(DIM))));
        }
        let m = util::sys::memory();
        let used = 1.0 - m.available as f64 / m.total.max(1) as f64;
        s.push(Line::from(vec![
            Span::styled(format!("{:<10}", "Memory"), Style::new().fg(DIM)),
            Span::styled(bar(used, 20), Style::new().fg(bar_color(used))),
            Span::raw(format!(" {} of {} used", util::fmt_size(m.total - m.available), util::fmt_size(m.total))),
        ]));
        for d in util::sys::disks().into_iter().filter(|d| d.total > 1 << 30) {
            let u = 1.0 - d.free as f64 / d.total as f64;
            s.push(Line::from(vec![
                Span::styled(format!("{:<10}", util::ellipsize(&d.mount.to_string_lossy(), 9)), Style::new().fg(DIM)),
                Span::styled(bar(u, 20), Style::new().fg(bar_color(u))),
                Span::raw(format!(
                    " {} free of {}{}",
                    util::fmt_size(d.free),
                    util::fmt_size(d.total),
                    if d.removable { " (removable)" } else { "" }
                )),
            ]));
        }
        let up = util::sys::uptime_secs();
        s.push(row("Uptime", format!("{}d {}h {}m", up / 86400, up / 3600 % 24, up / 60 % 60)));
        s.push(row("Data", util::data_dir().display().to_string()));
        f.render_widget(Paragraph::new(s).block(block("This machine")), left);

        // quick actions
        let rows: Vec<Line<'static>> = ACTIONS
            .iter()
            .enumerate()
            .map(|(i, (n, d, _))| {
                Line::from(vec![
                    Span::styled(format!(" {} ", i + 2), Style::new().fg(ACCENT2).bold()),
                    Span::styled(format!("{n:<10}"), Style::new().fg(ACCENT).bold()),
                    Span::styled(d.to_string(), Style::new().fg(TEXT)),
                ])
            })
            .collect();
        self.list.render(f, right, focused_block("Start here"), rows);

        let safe: Vec<Line> = [
            ("Analyze first", "every list shows sizes before anything is deleted - F2 turns on dry run for everything"),
            ("Restore point", "created before cleaning and before applying tweaks (Windows, as admin)"),
            ("Registry export", "every key Broom touches is saved as .reg first - re-import it in Backups"),
            ("Quarantine", "uninstall leftovers are moved, not deleted - put them back with one key"),
            ("Exact undo", "tweaks record the real previous value, so undo puts back what you had (one-way ones are flagged)"),
            ("Your files", "Documents, Downloads, Pictures, Music, Videos and OneDrive are never touched"),
        ]
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled("  ✓ ", Style::new().fg(OK)),
                Span::styled(format!("{k:<16}"), Style::new().fg(ACCENT2)),
                Span::styled(v.to_string(), Style::new().fg(TEXT)),
            ])
        })
        .collect();
        if safety.height > 2 {
            f.render_widget(Paragraph::new(safe).block(block("How Broom keeps you safe")), safety);
        }

        let credits = Line::from(vec![
            Span::styled("  Standing on the shoulders of ", Style::new().fg(DIM)),
            Span::styled(
                "winutil · Bulk Crap Uninstaller · Prune · Optimizer · Sparkle · ReviOS · Slate · MangoDisk",
                Style::new().fg(TEXT),
            ),
            Span::styled("  (see About in Backups)", Style::new().fg(DIM)),
        ]);
        f.render_widget(Paragraph::new(vec![credits]), bottom);
    }

    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd {
        if self.list.nav(&k) {
            return Cmd::None;
        }
        if k.code == KeyCode::Enter {
            ctx.switch_to = Some(ACTIONS[self.list.sel].2);
        }
        Cmd::None
    }

    fn click(&mut self, _c: u16, row: u16) {
        self.list.click(row);
    }

    fn scroll(&mut self, d: i32) {
        self.list.scroll(d.signum());
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "choose"), ("Enter", "open")]
    }
}
