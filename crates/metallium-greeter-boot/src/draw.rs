//! Drawing a [`View`] into the scanout buffer the way the OS's own console looks: the same terminal box (80 x 28 cells of IBM Plex Mono, 14 px, light grey on black) centred on the
//! same backdrop, lines from the top left, no boxes or centring. The numbers are `user/src/console.rs`'s in Metallium-OS.
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use greeter_core::View;

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
