//! Terminal UI: tabs, background jobs, modals, mouse, help.

mod backups;
mod clean;
mod doctor;
pub mod export;
mod home;
mod list;
pub mod theme;
mod tweaks;
mod uninstall;

use crate::util::{self, fs as bfs};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use theme::*;

// ---------------------------------------------------------------- background jobs

pub enum Msg<T> {
    Progress(usize, usize, String),
    Data(T),
    Finished,
}

#[derive(Clone)]
pub struct Reporter<T>(Sender<Msg<T>>);

impl<T> Reporter<T> {
    pub fn progress(&self, done: usize, total: usize, label: impl Into<String>) {
        let _ = self.0.send(Msg::Progress(done, total, label.into()));
    }
    pub fn send(&self, t: T) {
        let _ = self.0.send(Msg::Data(t));
    }
}

pub struct Job<T> {
    rx: Receiver<Msg<T>>,
    pub done: usize,
    pub total: usize,
    pub label: String,
    pub started: Instant,
    finished: bool,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(label: &str, f: impl FnOnce(Reporter<T>) + Send + 'static) -> Self {
        let (tx, rx) = channel();
        let rep = Reporter(tx.clone());
        std::thread::spawn(move || {
            f(rep);
            let _ = tx.send(Msg::Finished);
        });
        Job { rx, done: 0, total: 0, label: label.into(), started: Instant::now(), finished: false }
    }

    /// Drain pending messages; returns the data items received.
    pub fn poll(&mut self) -> Vec<T> {
        let mut out = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Progress(d, t, l) => {
                    self.done = d;
                    self.total = t;
                    self.label = l;
                }
                Msg::Data(x) => out.push(x),
                Msg::Finished => self.finished = true,
            }
        }
        out
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn status(&self, tick: usize) -> String {
        let pct = if self.total > 0 { format!(" {}/{}", self.done, self.total) } else { String::new() };
        format!("{} {}{}  ({}s)", spinner(tick), self.label, pct, self.started.elapsed().as_secs())
    }
}

// ---------------------------------------------------------------- tab contract

pub struct Ctx {
    pub tick: usize,
    pub admin: bool,
    pub toast: Option<(String, Instant, Color)>,
    pub hardware: Option<crate::doctor::Hardware>,
    pub switch_to: Option<usize>,
}

impl Ctx {
    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now(), OK));
    }
    pub fn error(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now(), BAD));
    }
}

pub struct Confirm {
    pub title: String,
    pub body: Vec<Line<'static>>,
    pub yes: String,
    pub danger: bool,
    pub tag: u32,
}

pub enum Cmd {
    None,
    Confirm(Confirm),
    Info(String, Vec<Line<'static>>),
    /// Leave the TUI, run a command in the real terminal (e.g. sudo), come back, then call `confirmed(tag)`
    Foreground(Vec<String>, u32),
    Quit,
}

pub trait Tab {
    fn title(&self) -> &'static str;
    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx);
    fn key(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Cmd;
    fn tick(&mut self, _ctx: &mut Ctx) {}
    fn hints(&self) -> Vec<(&'static str, &'static str)>;
    fn confirmed(&mut self, _tag: u32, _ctx: &mut Ctx) {}
    /// Called when the tab becomes visible.
    fn activate(&mut self, _ctx: &mut Ctx) {}
    fn busy(&self, _tick: usize) -> Option<String> {
        None
    }
    /// True while the tab is reading text (search box): global keys are disabled.
    fn typing(&self) -> bool {
        false
    }
    fn click(&mut self, _col: u16, _row: u16) {}
    fn scroll(&mut self, _delta: i32) {}
    /// Sub-view switch (Left/Right) handled by the tab.
    fn sub(&mut self, _dir: i32, _ctx: &mut Ctx) {}
}

enum Modal {
    Confirm(usize, Confirm),
    Info(String, Vec<Line<'static>>, u16),
    Help,
}

struct App {
    tabs: Vec<Box<dyn Tab>>,
    active: usize,
    ctx: Ctx,
    modal: Option<Modal>,
    tab_rects: Vec<Rect>,
    body: Rect,
    quit: bool,
    os_line: String,
}

pub const TAB_HOME: usize = 0;
pub const TAB_CLEAN: usize = 1;
pub const TAB_UNINSTALL: usize = 2;
pub const TAB_TWEAKS: usize = 3;
pub const TAB_DOCTOR: usize = 4;
pub const TAB_BACKUPS: usize = 5;

impl App {
    fn new(start_tab: usize) -> Self {
        App {
            tabs: vec![
                Box::new(home::Home::new()),
                Box::new(clean::CleanTab::new()),
                Box::new(uninstall::UninstallTab::new()),
                Box::new(tweaks::TweaksTab::new()),
                Box::new(doctor::DoctorTab::new()),
                Box::new(backups::BackupsTab::new()),
            ],
            active: start_tab,
            ctx: Ctx { tick: 0, admin: util::is_admin(), toast: None, hardware: None, switch_to: None },
            modal: None,
            tab_rects: vec![],
            body: Rect::default(),
            quit: false,
            os_line: format!("{} · {}", util::sys::os_pretty(), util::native_arch()),
        }
    }

    fn step(&mut self) {
        self.ctx.tick = self.ctx.tick.wrapping_add(1);
        for t in self.tabs.iter_mut() {
            t.tick(&mut self.ctx);
        }
        if let Some(i) = self.ctx.switch_to.take() {
            self.switch(i);
        }
    }
}

/// Render the real UI off-screen (for tests, docs and screenshots).
/// `keys` is a comma list like "Right,Down,Space,a"; jobs are awaited (up to `wait`) before and after the keys.
pub fn snapshot(start_tab: usize, width: u16, height: u16, keys: &str, wait: Duration) -> ratatui::buffer::Buffer {
    use ratatui::backend::TestBackend;
    let mut term = ratatui::Terminal::new(TestBackend::new(width, height)).expect("test backend");
    let mut app = App::new(start_tab);
    app.tabs[app.active].activate(&mut app.ctx);
    let settle = |app: &mut App, term: &mut ratatui::Terminal<TestBackend>| {
        let t0 = Instant::now();
        loop {
            app.step();
            let _ = term.draw(|f| app.draw(f));
            let busy = app.tabs.iter().any(|t| t.busy(0).is_some());
            if (!busy && t0.elapsed() > Duration::from_millis(300)) || t0.elapsed() > wait {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    settle(&mut app, &mut term);
    for k in keys.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()) {
        let code = match k {
            "Right" => KeyCode::Right,
            "Left" => KeyCode::Left,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Enter" => KeyCode::Enter,
            "Esc" => KeyCode::Esc,
            "Tab" => KeyCode::Tab,
            "Space" => KeyCode::Char(' '),
            "PageDown" => KeyCode::PageDown,
            c if c.chars().count() == 1 => KeyCode::Char(c.chars().next().unwrap()),
            _ => continue,
        };
        let _ = app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        settle(&mut app, &mut term);
    }
    let _ = term.draw(|f| app.draw(f));
    term.backend().buffer().clone()
}

pub fn run(start_tab: usize) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    let mut app = App::new(start_tab);
    app.tabs[app.active].activate(&mut app.ctx);
    let res = (|| -> anyhow::Result<()> {
        while !app.quit {
            terminal.draw(|f| app.draw(f))?;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => {
                        if let Some(fg) = app.on_key(k) {
                            // run a command in the real terminal
                            let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
                            ratatui::restore();
                            foreground(&fg.0);
                            terminal = ratatui::init();
                            let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
                            let a = app.active;
                            app.tabs[a].confirmed(fg.1, &mut app.ctx);
                        }
                    }
                    Event::Mouse(m) => app.on_mouse(m.kind, m.column, m.row),
                    _ => {}
                }
            }
            app.step();
        }
        Ok(())
    })();
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    res
}

fn foreground(cmd: &[String]) {
    println!("\n  Broom is running: {}\n", cmd.join(" "));
    if let Some((prog, args)) = cmd.split_first() {
        match std::process::Command::new(prog).args(args).status() {
            Ok(s) => println!("\n  Finished ({s})."),
            Err(e) => println!("\n  Could not start: {e}"),
        }
    }
    println!("  Press Enter to return to Broom...");
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}

impl App {
    fn switch(&mut self, i: usize) {
        if i < self.tabs.len() && i != self.active {
            self.active = i;
            self.tabs[i].activate(&mut self.ctx);
        }
    }

    fn on_key(&mut self, k: KeyEvent) -> Option<(Vec<String>, u32)> {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return None;
        }
        // modal first
        if let Some(m) = self.modal.take() {
            match m {
                Modal::Confirm(tab, c) => match k.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        let tag = c.tag;
                        self.tabs[tab].confirmed(tag, &mut self.ctx);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {}
                    _ => self.modal = Some(Modal::Confirm(tab, c)),
                },
                Modal::Info(t, l, scroll) => match k.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('i') => {}
                    KeyCode::Down | KeyCode::Char('j') => self.modal = Some(Modal::Info(t, l, scroll.saturating_add(1))),
                    KeyCode::Up | KeyCode::Char('k') => self.modal = Some(Modal::Info(t, l, scroll.saturating_sub(1))),
                    KeyCode::PageDown => self.modal = Some(Modal::Info(t, l, scroll.saturating_add(10))),
                    KeyCode::PageUp => self.modal = Some(Modal::Info(t, l, scroll.saturating_sub(10))),
                    _ => self.modal = Some(Modal::Info(t, l, scroll)),
                },
                Modal::Help => {}
            }
            return None;
        }
        let typing = self.tabs[self.active].typing();
        if !typing {
            match k.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.quit = true;
                    return None;
                }
                KeyCode::Char('?') | KeyCode::F(1) => {
                    self.modal = Some(Modal::Help);
                    return None;
                }
                KeyCode::F(2) => {
                    let d = !bfs::dry();
                    bfs::DRY_RUN.store(d, std::sync::atomic::Ordering::Relaxed);
                    self.ctx.toast(if d { "Dry run ON - nothing will be changed" } else { "Dry run OFF - changes are real" });
                    return None;
                }
                KeyCode::Tab => {
                    self.switch((self.active + 1) % self.tabs.len());
                    return None;
                }
                KeyCode::BackTab => {
                    self.switch((self.active + self.tabs.len() - 1) % self.tabs.len());
                    return None;
                }
                KeyCode::Char(c @ '1'..='6') => {
                    self.switch(c as usize - '1' as usize);
                    return None;
                }
                KeyCode::Left => {
                    let a = self.active;
                    self.tabs[a].sub(-1, &mut self.ctx);
                    return None;
                }
                KeyCode::Right => {
                    let a = self.active;
                    self.tabs[a].sub(1, &mut self.ctx);
                    return None;
                }
                _ => {}
            }
        }
        let a = self.active;
        match self.tabs[a].key(k, &mut self.ctx) {
            Cmd::None => {}
            Cmd::Quit => self.quit = true,
            Cmd::Confirm(c) => self.modal = Some(Modal::Confirm(a, c)),
            Cmd::Info(t, l) => self.modal = Some(Modal::Info(t, l, 0)),
            Cmd::Foreground(cmd, tag) => return Some((cmd, tag)),
        }
        None
    }

    fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.modal.is_some() {
                    return;
                }
                if let Some(i) = self.tab_rects.iter().position(|r| r.contains((col, row).into())) {
                    self.switch(i);
                } else if self.body.contains((col, row).into()) {
                    let a = self.active;
                    self.tabs[a].click(col, row);
                }
            }
            MouseEventKind::ScrollDown => {
                let a = self.active;
                self.tabs[a].scroll(3);
            }
            MouseEventKind::ScrollUp => {
                let a = self.active;
                self.tabs[a].scroll(-3);
            }
            _ => {}
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let [header, tabs, body, status, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(f.area());
        self.draw_header(f, header);
        self.draw_tabs(f, tabs);
        self.body = body;
        let a = self.active;
        self.tabs[a].render(f, body, &self.ctx);
        self.draw_status(f, status);
        let mut h = self.tabs[a].hints();
        h.extend_from_slice(&[("Tab/1-6", "tabs"), ("F2", "dry run"), ("?", "help"), ("q", "quit")]);
        f.render_widget(Paragraph::new(hints(&h)), footer);
        match &self.modal {
            Some(Modal::Confirm(_, c)) => draw_confirm(f, c),
            Some(Modal::Info(t, l, s)) => draw_info(f, t, l, *s),
            Some(Modal::Help) => draw_help(f, &self.tabs[a].hints()),
            None => {}
        }
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let sys = bfs::system_root();
        let disk = util::sys::disks().into_iter().find(|d| sys.starts_with(&d.mount) || d.mount == sys);
        let mut spans = vec![
            Span::styled(" ▌BROOM ", Style::new().fg(Color::Black).bg(ACCENT).bold()),
            Span::styled(format!(" v{} ", util::VERSION), Style::new().fg(DIM)),
            Span::styled(format!(" {} ", self.os_line), Style::new().fg(TEXT)),
        ];
        if let Some(d) = disk {
            let used = 1.0 - d.free as f64 / d.total.max(1) as f64;
            spans.push(Span::styled(format!(" {} ", d.mount.display()), Style::new().fg(TEXT)));
            spans.push(Span::styled(bar(used, 10), Style::new().fg(bar_color(used))));
            spans.push(Span::styled(format!(" {} free ", util::fmt_size(d.free)), Style::new().fg(TEXT)));
        }
        if self.ctx.admin {
            spans.push(Span::styled(" ADMIN ", Style::new().fg(Color::Black).bg(OK)));
        } else {
            spans.push(Span::styled(" LIMITED ", Style::new().fg(Color::Black).bg(WARN)));
        }
        if bfs::dry() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(" DRY RUN ", Style::new().fg(Color::Black).bg(Color::Magenta).bold()));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_tabs(&mut self, f: &mut Frame, area: Rect) {
        self.tab_rects.clear();
        let mut x = area.x + 1;
        let mut spans = vec![Span::raw(" ")];
        for (i, t) in self.tabs.iter().enumerate() {
            let label = format!(" {} {} ", i + 1, t.title());
            let w = label.chars().count() as u16;
            self.tab_rects.push(Rect::new(x, area.y, w, 1));
            x += w + 1;
            let style = if i == self.active {
                Style::new().fg(Color::Black).bg(ACCENT2).add_modifier(Modifier::BOLD)
            } else if self.tabs[i].busy(0).is_some() {
                Style::new().fg(ACCENT)
            } else {
                Style::new().fg(TEXT)
            };
            spans.push(Span::styled(label, style));
            spans.push(Span::raw(" "));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_status(&mut self, f: &mut Frame, area: Rect) {
        let a = self.active;
        let line = if let Some(b) = self.tabs[a].busy(self.ctx.tick) {
            Line::from(Span::styled(format!(" {b}"), Style::new().fg(ACCENT)))
        } else if let Some((msg, at, color)) = &self.ctx.toast {
            if at.elapsed() < Duration::from_secs(6) {
                Line::from(Span::styled(format!(" {msg}"), Style::new().fg(*color)))
            } else {
                Line::default()
            }
        } else {
            Line::default()
        };
        f.render_widget(Paragraph::new(line), area);
    }
}

fn draw_confirm(f: &mut Frame, c: &Confirm) {
    let h = (c.body.len() as u16 + 6).min(f.area().height.saturating_sub(2));
    let area = centered(f.area(), 76, h);
    f.render_widget(Clear, area);
    let mut lines = c.body.clone();
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled(format!(" Y  {} ", c.yes), Style::new().fg(Color::Black).bg(if c.danger { BAD } else { OK }).bold()),
        Span::raw("   "),
        Span::styled(" N  Cancel ", Style::new().fg(Color::Black).bg(DIM)),
    ]));
    let block = if c.danger { block(&c.title).border_style(Style::new().fg(BAD)) } else { focused_block(&c.title) };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
}

fn draw_info(f: &mut Frame, title: &str, lines: &[Line<'static>], scroll: u16) {
    let area = centered(f.area(), (f.area().width * 4 / 5).max(60), (f.area().height * 4 / 5).max(12));
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(Text::from(lines.to_vec()))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(focused_block(title).title_bottom(Line::from(" ↑↓ scroll · Esc close ").fg(DIM))),
        area,
    );
}

fn draw_help(f: &mut Frame, tab_hints: &[(&str, &str)]) {
    let area = centered(f.area(), 72, 26);
    f.render_widget(Clear, area);
    let mut l: Vec<Line> = theme::logo_lines();
    l.push(Line::default());
    l.push(Line::from("Global").fg(ACCENT).bold());
    for (k, d) in [
        ("1-6 / Tab", "switch tabs (or click them)"),
        ("← →", "switch views inside a tab"),
        ("↑ ↓ PgUp PgDn", "move (mouse wheel works too)"),
        ("F2", "toggle dry run - nothing is changed while on"),
        ("?", "this help"),
        ("q / Ctrl+C", "quit"),
    ] {
        l.push(Line::from(vec![Span::styled(format!("  {k:<16}"), Style::new().fg(ACCENT2)), Span::raw(d.to_string())]));
    }
    l.push(Line::default());
    l.push(Line::from("This tab").fg(ACCENT).bold());
    for (k, d) in tab_hints {
        l.push(Line::from(vec![Span::styled(format!("  {k:<16}"), Style::new().fg(ACCENT2)), Span::raw(d.to_string())]));
    }
    f.render_widget(Paragraph::new(l).block(focused_block("Help")), area);
}
