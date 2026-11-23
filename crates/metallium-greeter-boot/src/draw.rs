//! Drawing a [`View`] into the scanout buffer the way the OS's own console looks: the same terminal box (80 x 28 cells of IBM Plex Mono, 14 px, light grey on black) centred on the
//! same backdrop, lines from the top left, no boxes or centring. The numbers are `user/src/console.rs`'s in Metallium-OS.
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use greeter_core::touch::{Cap, Layout, Metrics, Rect, Target, Touch};
use greeter_core::View;
use metallium_gfx::Canvas;

const BACKDROP: u32 = 0x101820;
const BG: u32 = 0x000000;
const FG: u32 = 0xC0C0C0;
const ACCENT: u32 = 0xFFFFFF;
const DIM: u32 = 0x808080;
const ERR: u32 = 0xFF5555;

const CELL_W: usize = 8;
const CELL_H: usize = 19;
const MARGIN: usize = 4;
const COLS: usize = 80;
const ROWS: usize = 28;
const BOX_W: usize = 2 * MARGIN + COLS * CELL_W;
const BOX_H: usize = 2 * MARGIN + ROWS * CELL_H;

/// One character of the monospace face with the top left of its cell at (x, y), blended with what is there by the glyph's coverage.
fn char_at(fb: &mut [u32], w: usize, x: usize, y: usize, color: u32, ch: u8) {
    let face = &metallium_gfx::typeface::MONO;
    let Some(g) = face.glyph(ch as char) else { return };
    for row in 0..g.h as usize {
        for col in 0..g.w as usize {
            let i = row * g.w as usize + col;
            let byte = face.data[g.off as usize + i / 2];
            let cov = (if i % 2 == 0 { byte & 15 } else { byte >> 4 }) as u32 * 17;
            if cov == 0 {
                continue;
            }
            let (px, py) = (x as i32 + g.bx as i32 + col as i32, y as i32 + g.by as i32 + row as i32);
            if px < 0 || py < 0 || px as usize >= w {
                continue;
            }
            if let Some(p) = fb.get_mut(py as usize * w + px as usize) {
                *p = metallium_gfx::blend(*p, color, cov);
            }
        }
    }
}

/// Draw the whole screen (`fb` is `w * h` words).
pub fn draw(fb: &mut [u32], w: usize, h: usize, v: &View) {
    fb.fill(BACKDROP);
    let (ox, oy) = (w.saturating_sub(BOX_W) / 2, h.saturating_sub(BOX_H) / 2);
    for y in oy..(oy + BOX_H).min(h) {
        for x in ox..(ox + BOX_W).min(w) {
            fb[y * w + x] = BG;
        }
    }
    let mut lines: Vec<(String, u32)> = Vec::new();
    lines.push((String::from("Metallium-OS"), ACCENT));
    lines.push((v.title.clone(), FG));
    lines.push((String::new(), FG));
    for t in &v.intro {
        lines.push((t.clone(), FG));
    }
    if let Some(key) = &v.key {
        lines.push((String::new(), FG));
        lines.push((key.clone(), ACCENT));
    }
    lines.push((String::new(), FG));
    for f in &v.fields {
        lines.push((format!("{}: {}{}", f.label, f.text, if f.focused { "_" } else { "" }), if f.focused { ACCENT } else { FG }));
    }
    if let Some(m) = &v.message {
        lines.push((String::new(), FG));
        lines.push((m.text.clone(), if m.error { ERR } else { ACCENT }));
    }
    for (row, (text, color)) in lines.iter().enumerate().filter(|(r, _)| *r < ROWS - 1) {
        for (i, c) in text.bytes().take(COLS).enumerate() {
            char_at(fb, w, ox + MARGIN + i * CELL_W, oy + MARGIN + row * CELL_H, *color, c);
        }
    }
    // the hint on the last row
    for (i, c) in v.hint.bytes().take(COLS).enumerate() {
        char_at(fb, w, ox + MARGIN + i * CELL_W, oy + MARGIN + (ROWS - 1) * CELL_H, DIM, c);
    }
}

// ---- the touch screen: large fields, buttons, an on-screen keyboard (greeter-core's `touch`) ----

const T_PANEL: u32 = 0x0C1219;
const T_FIELD: u32 = 0x1B2735;
const T_KEY: u32 = 0x2B3747;
const T_KEY_SPECIAL: u32 = 0x3A4A5E;
const T_KEY_DOWN: u32 = 0x5B7797;
const T_ACCENT: u32 = 0x8AB4F8;
const T_ON_ACCENT: u32 = 0x0B1B33;
const T_TEXT: u32 = 0xE8EAED;
const T_DIM: u32 = 0x9AA0A6;
const T_ERR: u32 = 0xFF6B6B;

fn rect(r: Rect) -> metallium_gfx::Rect {
    metallium_gfx::Rect::new(r.x, r.y, r.w, r.h)
}

/// The line heights `Layout::new` needs, from the faces `draw_touch` uses.
pub fn metrics() -> Metrics {
    use metallium_gfx::typeface::{BODY, CAPTION, LARGE, TITLE};
    Metrics { caption: CAPTION.line_height(), body: BODY.line_height(), title: TITLE.line_height(), big: LARGE.line_height() }
}

/// Draw the whole screen for a touch machine (`fb` is `w * h` words).
pub fn draw_touch(fb: &mut [u32], w: usize, h: usize, v: &View, l: &Layout, t: &Touch) {
    use metallium_gfx::typeface::{BODY, CAPTION, LARGE, TITLE};
    let mut c = Canvas::new(fb, w as i32, h as i32, w as i32);
    c.fill(metallium_gfx::Rect::new(0, 0, w as i32, h as i32), BACKDROP);
    c.label(l.margin, l.title_y, &TITLE, ACCENT, 255, &v.title);
    for (i, line) in v.intro.iter().enumerate() {
        c.label(l.margin, l.intro_y + i as i32 * BODY.line_height(), &BODY, T_DIM, 255, line);
    }
    if let Some(key) = &v.key {
        // two lines: the first four groups, then the rest
        let cut = key.match_indices('-').nth(3).map_or(key.len(), |(i, _)| i);
        let (a, b) = (&key[..cut], key.get(cut + 1..).unwrap_or(""));
        c.label(l.margin, l.key_y, &LARGE, ACCENT, 255, a);
        c.label(l.margin, l.key_y + LARGE.line_height(), &LARGE, ACCENT, 255, b);
    }
    for (i, (f, r)) in v.fields.iter().zip(&l.fields).enumerate() {
        let _ = i;
        if f.focused {
            c.fill_round(rect(*r), 10, T_ACCENT, 255);
            c.fill_round(rect(Rect { x: r.x + 2, y: r.y + 2, w: r.w - 4, h: r.h - 4 }), 8, T_FIELD, 255);
        } else {
            c.fill_round(rect(*r), 10, T_FIELD, 255);
        }
        let (label_y, value_y, value_x) = if r.h >= 56 {
            (r.y + 6, r.y + r.h - BODY.line_height() - 8, r.x + 14)
        } else {
            let lw = CAPTION.width(&f.label);
            (r.y + (r.h - CAPTION.line_height()) / 2, r.y + (r.h - BODY.line_height()) / 2, r.x + 14 + lw + 14)
        };
        c.label(r.x + 14, label_y, &CAPTION, if f.focused { T_ACCENT } else { T_DIM }, 255, &f.label);
        let end = value_x + c.label(value_x, value_y, &BODY, T_TEXT, 255, &f.text);
        if f.focused {
            c.fill(metallium_gfx::Rect::new(end + 2, value_y + 2, 2, BODY.line_height() - 4), T_ACCENT);
        }
    }
    if let Some(m) = &v.message {
        c.label(l.margin, l.message_y, &BODY, if m.error { T_ERR } else { T_ACCENT }, 255, &m.text);
    }
    for (i, (a, r)) in v.actions.iter().zip(&l.actions).enumerate() {
        let down = t.pressed == Some(Target::Action(i));
        let primary = v.fields.is_empty();
        let (bg, fg) = if primary { (if down { T_KEY_DOWN } else { T_ACCENT }, T_ON_ACCENT) } else { (if down { T_KEY_DOWN } else { T_FIELD }, T_ACCENT) };
        c.fill_round(rect(*r), 26.min(r.h / 2), bg, 255);
        c.label_centered(rect(*r), &BODY, fg, 255, &a.label);
    }
    if let Some(first) = l.keys.first() {
        let top = first.rect.y - 6;
        c.fill(metallium_gfx::Rect::new(0, top, w as i32, h as i32 - top), T_PANEL);
        let face = if l.big_keys { &LARGE } else { &TITLE };
        for (i, k) in l.keys.iter().enumerate() {
            let down = t.pressed == Some(Target::Key(i));
            let (bg, fg) = match k.cap {
                Cap::Enter => (if down { T_KEY_DOWN } else { T_ACCENT }, T_ON_ACCENT),
                Cap::Shift if t.osk.shift => (T_ACCENT, T_ON_ACCENT),
                Cap::Shift | Cap::Backspace | Cap::To(_) => (if down { T_KEY_DOWN } else { T_KEY_SPECIAL }, T_TEXT),
                _ => (if down { T_KEY_DOWN } else { T_KEY }, T_TEXT),
            };
            c.fill_round(rect(k.rect), 8, bg, 255);
            // a label too wide for its key (Enter's "Recover", say) drops to the smaller face
            let text = t.osk.label(k.cap, &v.submit);
            let f = if face.width(&text) + 12 > k.rect.w { &BODY } else { face };
            c.label_centered(rect(k.rect), f, fg, 255, &text);
        }
    }
}
