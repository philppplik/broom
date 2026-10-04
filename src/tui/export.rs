//! Turn a rendered frame into plain text or a terminal-styled SVG (used for README screenshots).

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

pub fn to_text(b: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..b.area.height {
        let mut line = String::new();
        for x in 0..b.area.width {
            line.push_str(b[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Windows Terminal "Campbell" palette.
fn hex(c: Color, fg: bool) -> String {
    let s = match c {
        Color::Reset => {
            if fg {
                "#CCCCCC"
            } else {
                "#0C0C0C"
            }
        }
        Color::Black => "#0C0C0C",
        Color::Red => "#C50F1F",
        Color::Green => "#13A10E",
        Color::Yellow => "#C19C00",
        Color::Blue => "#0037DA",
        Color::Magenta => "#881798",
        Color::Cyan => "#3A96DD",
        Color::Gray => "#CCCCCC",
        Color::DarkGray => "#767676",
        Color::LightRed => "#E74856",
        Color::LightGreen => "#16C60C",
        Color::LightYellow => "#F9F1A5",
        Color::LightBlue => "#3B78FF",
        Color::LightMagenta => "#B4009E",
        Color::LightCyan => "#61D6D6",
        Color::White => "#F2F2F2",
        Color::Rgb(r, g, b) => return format!("#{r:02X}{g:02X}{b:02X}"),
        Color::Indexed(_) => "#CCCCCC",
    };
    s.to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn to_svg(b: &Buffer, title: &str) -> String {
    let (cw, lh, pad, top) = (8.4f64, 18.0f64, 14.0f64, 36.0f64);
    let w = pad * 2.0 + b.area.width as f64 * cw;
    let h = top + b.area.height as f64 * lh + 12.0;
    let mut o = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n\
         <style>text{{font-family:'Cascadia Mono','Cascadia Code',Consolas,'DejaVu Sans Mono',monospace;font-size:14px;white-space:pre}}</style>\n\
         <rect width=\"100%\" height=\"100%\" rx=\"10\" fill=\"#0C0C0C\"/>\n\
         <rect width=\"100%\" height=\"28\" rx=\"10\" fill=\"#1F1F1F\"/><rect y=\"18\" width=\"100%\" height=\"10\" fill=\"#1F1F1F\"/>\n\
         <circle cx=\"18\" cy=\"14\" r=\"5.5\" fill=\"#FF5F57\"/><circle cx=\"36\" cy=\"14\" r=\"5.5\" fill=\"#FEBC2E\"/><circle cx=\"54\" cy=\"14\" r=\"5.5\" fill=\"#28C840\"/>\n\
         <text x=\"{:.0}\" y=\"18\" fill=\"#9D9D9D\" text-anchor=\"middle\" style=\"font-size:12px\">{}</text>\n",
        w / 2.0,
        esc(title)
    );
    for y in 0..b.area.height {
        let ty = top + y as f64 * lh;
        let mut x = 0u16;
        while x < b.area.width {
            let c = &b[(x, y)];
            let style = (c.fg, c.bg, c.modifier);
            let start = x;
            let mut text = String::new();
            while x < b.area.width {
                let d = &b[(x, y)];
                if (d.fg, d.bg, d.modifier) != style {
                    break;
                }
                text.push_str(d.symbol());
                x += 1;
            }
            let px = pad + start as f64 * cw;
            let len = (x - start) as f64 * cw;
            let (fg, bg) = if style.2.contains(Modifier::REVERSED) { (style.1, style.0) } else { (style.0, style.1) };
            if bg != Color::Reset {
                o.push_str(&format!(
                    "<rect x=\"{px:.1}\" y=\"{ty:.1}\" width=\"{len:.1}\" height=\"{lh}\" fill=\"{}\"/>\n",
                    hex(bg, false)
                ));
            }
            if !text.trim().is_empty() {
                let weight = if style.2.contains(Modifier::BOLD) { " font-weight=\"bold\"" } else { "" };
                o.push_str(&format!(
                    "<text x=\"{px:.1}\" y=\"{:.1}\" fill=\"{}\"{weight} textLength=\"{len:.1}\" lengthAdjust=\"spacingAndGlyphs\" xml:space=\"preserve\">{}</text>\n",
                    ty + 13.5,
                    hex(fg, true),
                    esc(&text)
                ));
            }
        }
    }
    o.push_str("</svg>\n");
    o
}
