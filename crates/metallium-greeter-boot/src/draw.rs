//! Drawing a [`View`] into the scanout buffer with the OS's 5x9 bitmap font: plain centred lines on a dark background.
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use greeter_core::View;
use metallium_gfx::font;

const BG: u32 = 0x101820;
const TEXT: u32 = 0xE6EDF3;
const DIM: u32 = 0x8FA3B5;
const ACCENT: u32 = 0x7EE787;
const ERROR: u32 = 0xFF7B72;
const FIELD: u32 = 0x1B2733;

struct Line {
    text: String,
    color: u32,
    scale: usize,
    /// A rectangle behind it (an input field).
    boxed: bool,
}

fn line(text: impl Into<String>, color: u32, scale: usize) -> Line {
    Line { text: text.into(), color, scale, boxed: false }
}

fn char_at(fb: &mut [u32], w: usize, h: usize, x: usize, y: usize, scale: usize, color: u32, ch: u8) {
    let Some(rows) = font::glyph(ch).or_else(|| font::glyph(b'?')) else { return };
    for (r, bits) in rows.iter().enumerate() {
        for col in 0..font::W as usize {
            if bits & (1 << (font::W as usize - 1 - col)) != 0 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let (px, py) = (x + col * scale + dx, y + r * scale + dy);
                        if px < w && py < h {
                            fb[py * w + px] = color;
                        }
                    }
                }
            }
        }
    }
}

fn fill(fb: &mut [u32], w: usize, h: usize, x: usize, y: usize, rw: usize, rh: usize, color: u32) {
    for py in y..(y + rh).min(h) {
        for px in x..(x + rw).min(w) {
            fb[py * w + px] = color;
        }
    }
}

/// Draw the whole screen (`fb` is `w * h` words).
pub fn draw(fb: &mut [u32], w: usize, h: usize, v: &View) {
    fb.fill(BG);
    let unit = (w / 400).clamp(2, 4); // text scale follows the screen
    let mut lines: Vec<Line> = Vec::new();
    lines.push(line("Metallium-OS", DIM, unit));
    lines.push(line("", TEXT, 1));
    lines.push(line(v.title.as_str(), TEXT, unit + 1));
    lines.push(line("", TEXT, 1));
    for t in &v.intro {
        lines.push(line(t.as_str(), DIM, unit));
    }
    if let Some(key) = &v.key {
        lines.push(line("", TEXT, 1));
        lines.push(line(key.as_str(), ACCENT, unit + 1));
    }
    lines.push(line("", TEXT, 1));
    for f in &v.fields {
        let shown = format!("{}: {}{}", f.label, f.text, if f.focused { "_" } else { "" });
        let mut l = line(shown, if f.focused { TEXT } else { DIM }, unit);
        l.boxed = true;
        lines.push(l);
    }
    if let Some(m) = &v.message {
        lines.push(line("", TEXT, 1));
        lines.push(line(m.text.as_str(), if m.error { ERROR } else { ACCENT }, unit));
    }
    // the stack of lines, centred; the hint at the bottom
    let gap = unit * 3;
    let total: usize = lines.iter().map(|l| l.scale * font::H as usize + gap).sum();
    let mut y = h.saturating_sub(total) / 2;
    for l in &lines {
        let lw = l.text.len() * font::ADVANCE as usize * l.scale;
        let x = w.saturating_sub(lw) / 2;
        if l.boxed {
            let pad = unit * 2;
            fill(fb, w, h, w.saturating_sub(lw + 2 * pad) / 2, y.saturating_sub(pad), lw + 2 * pad, l.scale * font::H as usize + 2 * pad, FIELD);
        }
        for (i, c) in l.text.bytes().enumerate() {
            char_at(fb, w, h, x + i * font::ADVANCE as usize * l.scale, y, l.scale, l.color, c);
        }
        y += l.scale * font::H as usize + gap;
    }
    let hw = v.hint.len() * font::ADVANCE as usize * unit;
    for (i, c) in v.hint.bytes().enumerate() {
        char_at(fb, w, h, w.saturating_sub(hw) / 2 + i * font::ADVANCE as usize * unit, h.saturating_sub(font::H as usize * unit * 3), unit, DIM, c);
    }
}
