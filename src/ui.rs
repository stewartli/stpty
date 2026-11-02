use crossterm::{
    cursor::{MoveTo, Show},
    queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
};

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

pub fn popup_rect(cols: u16, rows: u16, pw: u16, ph: u16) -> Rect {
    let w = (cols as u32 * pw as u32 / 100).clamp(20.min(cols as u32), cols as u32) as u16;
    let h = (rows as u32 * ph as u32 / 100).clamp(6.min(rows as u32), rows as u32) as u16;
    Rect {
        x: (cols - w) / 2,
        y: (rows - h) / 2,
        w,
        h,
    }
}

pub fn split_rects(cols: u16, rows: u16) -> (Rect, Rect) {
    let lw = cols / 2;
    (
        Rect {
            x: 0,
            y: 0,
            w: lw,
            h: rows,
        },
        Rect {
            x: lw,
            y: 0,
            w: cols - lw,
            h: rows,
        },
    )
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn put_text(out: &mut Vec<u8>, x: u16, y: u16, max_w: u16, s: &str) -> std::io::Result<()> {
    queue!(out, MoveTo(x, y), Print(truncate(s, max_w as usize)))
}

pub fn fill(out: &mut Vec<u8>, r: Rect) -> std::io::Result<()> {
    let blank = " ".repeat(r.w as usize);
    queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
    for i in 0..r.h {
        queue!(out, MoveTo(r.x, r.y + i), Print(&blank))?;
    }
    Ok(())
}

fn vt_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::AnsiValue(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
    }
}

pub fn blit(out: &mut Vec<u8>, screen: &vt100::Screen, r: Rect) -> std::io::Result<()> {
    for row in 0..r.h {
        queue!(
            out,
            MoveTo(r.x, r.y + row),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
        let mut prev: Option<crate::Style> = None;
        for col in 0..r.w {
            let Some(cell) = screen.cell(row, col) else {
                queue!(out, Print(' '))?;
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let (mut fg, mut bg) = (cell.fgcolor(), cell.bgcolor());
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            let style: crate::Style = (fg, bg, cell.bold(), cell.italic(), cell.underline());
            if prev != Some(style) {
                queue!(out, SetAttribute(Attribute::Reset))?;
                queue!(
                    out,
                    SetForegroundColor(vt_color(fg)),
                    SetBackgroundColor(vt_color(bg))
                )?;
                if cell.bold() {
                    queue!(out, SetAttribute(Attribute::Bold))?;
                }
                if cell.italic() {
                    queue!(out, SetAttribute(Attribute::Italic))?;
                }
                if cell.underline() {
                    queue!(out, SetAttribute(Attribute::Underlined))?;
                }
                prev = Some(style);
            }
            let s = cell.contents();
            queue!(out, Print(if s.is_empty() { " " } else { s }))?;
        }
        queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
    }

    if !screen.hide_cursor() && screen.scrollback() == 0 {
        let (cr, cc) = screen.cursor_position();
        if cr < r.h && cc < r.w {
            queue!(out, MoveTo(r.x + cc, r.y + cr), Show)?;
        }
    }
    Ok(())
}

pub fn draw_box(out: &mut Vec<u8>, r: Rect, title: &str) -> std::io::Result<()> {
    if r.w < 2 || r.h < 2 {
        return Ok(());
    }
    queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
    let inner = (r.w - 2) as usize;
    let t = truncate(&format!(" {title} "), inner);
    let fill = inner.saturating_sub(t.chars().count());
    queue!(
        out,
        MoveTo(r.x, r.y),
        Print(format!("┌{t}{}┐", "─".repeat(fill)))
    )?;
    for i in 1..r.h - 1 {
        queue!(
            out,
            MoveTo(r.x, r.y + i),
            Print("│"),
            MoveTo(r.x + r.w - 1, r.y + i),
            Print("│")
        )?;
    }
    queue!(
        out,
        MoveTo(r.x, r.y + r.h - 1),
        Print(format!("└{}┘", "─".repeat(inner)))
    )
}
