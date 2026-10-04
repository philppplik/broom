//! Reusable scrolling list with selection, mouse support and a scrollbar.

use super::theme::*;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Margin, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

#[derive(Default)]
pub struct ListView {
    pub sel: usize,
    pub offset: usize,
    pub len: usize,
    /// inner area of the last render (for mouse hit-testing)
    pub inner: Rect,
}

impl ListView {
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        if self.sel >= len {
            self.sel = len.saturating_sub(1);
        }
    }

    pub fn move_by(&mut self, d: i32) {
        if self.len == 0 {
            return;
        }
        let n = self.sel as i64 + d as i64;
        self.sel = n.clamp(0, self.len as i64 - 1) as usize;
    }

    /// Handle navigation keys. Returns true if consumed.
    pub fn nav(&mut self, k: &KeyEvent) -> bool {
        let page = self.inner.height.max(1) as i32;
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::PageUp => self.move_by(-page),
            KeyCode::PageDown => self.move_by(page),
            KeyCode::Home | KeyCode::Char('g') => self.sel = 0,
            KeyCode::End | KeyCode::Char('G') => self.sel = self.len.saturating_sub(1),
            _ => return false,
        }
        true
    }

    /// Mouse click: select the clicked row. Returns true if it hit a row.
    pub fn click(&mut self, row: u16) -> bool {
        if row < self.inner.y || row >= self.inner.y + self.inner.height {
            return false;
        }
        let idx = self.offset + (row - self.inner.y) as usize;
        if idx < self.len {
            self.sel = idx;
            return true;
        }
        false
    }

    pub fn scroll(&mut self, d: i32) {
        self.move_by(d);
    }

    /// Render `rows` (all of them; only the visible window is drawn).
    pub fn render(&mut self, f: &mut Frame, area: Rect, block: Block<'static>, rows: Vec<Line<'static>>) {
        self.set_len(rows.len());
        let inner = block.inner(area);
        self.inner = inner;
        let h = inner.height as usize;
        if self.sel < self.offset {
            self.offset = self.sel;
        } else if h > 0 && self.sel >= self.offset + h {
            self.offset = self.sel + 1 - h;
        }
        if self.offset + h > self.len {
            self.offset = self.len.saturating_sub(h);
        }
        let visible: Vec<Line> = rows
            .into_iter()
            .enumerate()
            .skip(self.offset)
            .take(h)
            .map(|(i, l)| {
                if i == self.sel {
                    let mut l = l.patch_style(highlight());
                    // pad so the highlight spans the whole row
                    let w: usize = l.width();
                    if w < inner.width as usize {
                        l.spans.push(" ".repeat(inner.width as usize - w).into());
                        l = l.patch_style(highlight());
                    }
                    l
                } else {
                    l
                }
            })
            .collect();
        f.render_widget(Paragraph::new(visible).block(block), area);
        if self.len > h {
            let mut st = ScrollbarState::new(self.len.saturating_sub(h)).position(self.offset);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None),
                area.inner(Margin { vertical: 1, horizontal: 0 }),
                &mut st,
            );
        }
    }
}

/// Inline search box state.
#[derive(Default)]
pub struct Search {
    pub active: bool,
    pub text: String,
}

impl Search {
    /// Feed a key while typing. Returns true if the filter changed.
    pub fn key(&mut self, k: &KeyEvent) -> bool {
        match k.code {
            KeyCode::Esc => {
                self.active = false;
                self.text.clear();
                true
            }
            KeyCode::Enter => {
                self.active = false;
                false
            }
            KeyCode::Backspace => {
                self.text.pop();
                true
            }
            KeyCode::Char(c) => {
                self.text.push(c);
                true
            }
            _ => false,
        }
    }

    pub fn matches(&self, hay: &str) -> bool {
        self.text.is_empty() || hay.to_lowercase().contains(&self.text.to_lowercase())
    }

    pub fn title_suffix(&self) -> String {
        if self.active {
            format!(" · search: {}▏", self.text)
        } else if !self.text.is_empty() {
            format!(" · filter: {} (Esc in / clears)", self.text)
        } else {
            String::new()
        }
    }
}
