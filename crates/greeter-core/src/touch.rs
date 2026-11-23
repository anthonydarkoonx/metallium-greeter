//! Touch input for the greeter: an on-screen keyboard, tappable fields and buttons, and where all of them go on the screen. A phone has no keyboard, so the first account
//! (and every login) has to be typeable with a finger. This is logic only, like the rest of the crate: [`Layout`] says where things are, [`Touch`] turns presses into
//! [`Key`]s for [`crate::Greeter::key`] (or a field to focus), and the boot crate draws the rectangles.
use crate::{Key, View};
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

/// Line heights of the faces the drawing code will use, in pixels.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub caption: i32,
    pub body: i32,
    pub title: i32,
    /// The face the recovery key is shown in.
    pub big: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Page {
    #[default]
    Letters,
    Symbols,
    MoreSymbols,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cap {
    /// The character on the unshifted letters page (letters are upper case while shift is on).
    Char(u8),
    Shift,
    Backspace,
    /// Switch to another page.
    To(Page),
    Space,
    Enter,
}

/// The state of the on-screen keyboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Osk {
    /// One shot: the next letter is a capital.
    pub shift: bool,
    pub page: Page,
}

/// One row of keys: each with a width in tenths of a character key.
type Row = Vec<(Cap, i32)>;

fn chars(s: &str) -> impl Iterator<Item = (Cap, i32)> + '_ {
    s.bytes().map(|c| (Cap::Char(c), 10))
}

impl Osk {
    /// The four rows of the current page.
    pub fn rows(&self) -> [Row; 4] {
        let wide = |c| (c, 15);
        let (r1, r2, r3, to): (Row, Row, Row, Page) = match self.page {
            Page::Letters => (
                chars("qwertyuiop").collect(),
                chars("asdfghjkl").collect(),
                core::iter::once(wide(Cap::Shift)).chain(chars("zxcvbnm")).chain(core::iter::once(wide(Cap::Backspace))).collect(),
                Page::Symbols,
            ),
            Page::Symbols => (
                chars("1234567890").collect(),
                chars("@#$%&-_+()").collect(),
                core::iter::once(wide(Cap::To(Page::MoreSymbols))).chain(chars("*\"':;!?")).chain(core::iter::once(wide(Cap::Backspace))).collect(),
                Page::Letters,
            ),
            Page::MoreSymbols => (
                chars("[]{}<>^~`|").collect(),
                chars("\\/=+-_()%").collect(),
                core::iter::once(wide(Cap::To(Page::Symbols))).chain(chars("#$&*!?:")).chain(core::iter::once(wide(Cap::Backspace))).collect(),
                Page::Letters,
            ),
        };
        let r4: Row = alloc::vec![wide(Cap::To(to)), (Cap::Char(b','), 10), (Cap::Space, 45), (Cap::Char(b'.'), 10), (Cap::Enter, 20)];
        [r1, r2, r3, r4]
    }

    /// What a key says on its face. `submit` is the label of Enter.
    pub fn label(&self, cap: Cap, submit: &str) -> alloc::string::String {
        use alloc::string::String;
        match cap {
            Cap::Char(c) if self.page == Page::Letters && self.shift => String::from(c.to_ascii_uppercase() as char),
            Cap::Char(c) => String::from(c as char),
            Cap::Shift => String::from("Shift"),
            Cap::Backspace => String::from("Del"),
            Cap::To(Page::Letters) => String::from("abc"),
            Cap::To(Page::Symbols) => String::from("?123"),
            Cap::To(Page::MoreSymbols) => String::from("=\\<"),
            Cap::Space => String::new(),
            Cap::Enter => String::from(submit),
        }
    }

    /// Press a key: the [`Key`] it sends, if it sends one (Shift and the page keys only change the keyboard).
    pub fn press(&mut self, cap: Cap) -> Option<Key> {
        match cap {
            Cap::Char(c) => {
                let out = if self.page == Page::Letters && self.shift { c.to_ascii_uppercase() } else { c };
                self.shift = false;
                Some(Key::Char(out))
            }
            Cap::Shift => {
                self.shift = !self.shift;
                None
            }
            Cap::Backspace => Some(Key::Backspace),
            Cap::To(p) => {
                self.page = p;
                self.shift = false;
                None
            }
            Cap::Space => Some(Key::Char(b' ')),
            Cap::Enter => Some(Key::Enter),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyBox {
    pub rect: Rect,
    pub cap: Cap,
}

/// Where everything goes for one [`View`] on a `w` x `h` screen.
#[derive(Clone, Debug)]
pub struct Layout {
    pub margin: i32,
    /// Top of each block of text: the title, the first intro line, the recovery key (2 lines of the big face), the message.
    pub title_y: i32,
    pub intro_y: i32,
    pub key_y: i32,
    pub message_y: i32,
    pub fields: Vec<Rect>,
    pub actions: Vec<Rect>,
    /// Empty when the screen has no field to type in.
    pub keys: Vec<KeyBox>,
    /// Whether the keys are big enough for the big face.
    pub big_keys: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Field(usize),
    Action(usize),
    Key(usize),
}

const GAP: i32 = 6;

impl Layout {
    pub fn new(w: i32, h: i32, view: &View, osk: &Osk, m: Metrics) -> Layout {
        let margin = (w / 32).clamp(16, 48);
        let typing = !view.fields.is_empty();
        // The keyboard: four rows along the bottom, as tall as the screen allows without taking more than a third of it.
        let kh = if typing { ((h * 34 / 100) / 4).min(w / 10 * 115 / 100) } else { 0 };
        let kb_top = if typing { h - (4 * kh + 5 * GAP) } else { h };
        let mut keys = Vec::new();
        if typing {
            let inner = w - 2 * GAP;
            for (r, row) in osk.rows().iter().enumerate() {
                let total: i32 = row.iter().map(|k| k.1).sum();
                let mut x = GAP + (inner - inner * total / 100) / 2;
                let y = kb_top + GAP + r as i32 * (kh + GAP);
                for &(cap, units) in row {
                    let kw = inner * units / 100;
                    keys.push(KeyBox { rect: Rect { x, y, w: kw - GAP, h: kh }, cap });
                    x += kw;
                }
            }
        }
        // The text above it, top to bottom. The fields take the tallest size that fits.
        let button_h = 52;
        let fixed = |fh: i32| {
            let mut y = margin;
            y += m.title + 12;
            y += view.intro.len() as i32 * m.body + if view.intro.is_empty() { 0 } else { 12 };
            if view.key.is_some() {
                y += 2 * m.big + 24;
            }
            y += view.fields.len() as i32 * (fh + 8);
            y += m.body + 8; // the message
            y + view.actions.len() as i32 * (button_h + 8)
        };
        let fh = [64, 56, 48, 44].into_iter().find(|&fh| fixed(fh) <= kb_top).unwrap_or(44);
        let mut y = margin;
        let title_y = y;
        y += m.title + 12;
        let intro_y = y;
        y += view.intro.len() as i32 * m.body + if view.intro.is_empty() { 0 } else { 12 };
        let key_y = y;
        if view.key.is_some() {
            y += 2 * m.big + 24;
        }
        let mut fields = Vec::new();
        for _ in &view.fields {
            fields.push(Rect { x: margin, y, w: w - 2 * margin, h: fh });
            y += fh + 8;
        }
        let message_y = y;
        y += m.body + 8;
        let mut actions = Vec::new();
        for _ in &view.actions {
            actions.push(Rect { x: margin, y, w: w - 2 * margin, h: button_h });
            y += button_h + 8;
        }
        Layout { margin, title_y, intro_y, key_y, message_y, fields, actions, keys, big_keys: kh >= 90 }
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Target> {
        if let Some(i) = self.keys.iter().position(|k| k.rect.inflate_x(GAP / 2).contains(x, y)) {
            return Some(Target::Key(i));
        }
        if let Some(i) = self.actions.iter().position(|r| r.contains(x, y)) {
            return Some(Target::Action(i));
        }
        self.fields.iter().position(|r| r.contains(x, y)).map(Target::Field)
    }
}

impl Rect {
    /// Widened sideways by `d` on each side, so the gap between two keys is not a dead zone.
    fn inflate_x(&self, d: i32) -> Rect {
        Rect { x: self.x - d, y: self.y, w: self.w + 2 * d, h: self.h + GAP }
    }
}

/// What a finished tap asks of the greeter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tap {
    Key(Key),
    Focus(usize),
}

/// Touch state: the keyboard and the press in progress.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Touch {
    pub osk: Osk,
    pub pressed: Option<Target>,
}

impl Touch {
    pub fn down(&mut self, x: i32, y: i32, layout: &Layout) {
        self.pressed = layout.hit(x, y);
    }

    /// Sliding off what was pressed cancels the press.
    pub fn moved(&mut self, x: i32, y: i32, layout: &Layout) {
        if self.pressed.is_some() && layout.hit(x, y) != self.pressed {
            self.pressed = None;
        }
    }

    /// Lifting on what was pressed is a tap.
    pub fn up(&mut self, x: i32, y: i32, layout: &Layout, view: &View) -> Option<Tap> {
        let was = self.pressed.take()?;
        if layout.hit(x, y) != Some(was) {
            return None;
        }
        match was {
            Target::Field(i) => Some(Tap::Focus(i)),
            Target::Action(i) => view.actions.get(i).map(|a| Tap::Key(a.key)),
            Target::Key(i) => self.osk.press(layout.keys.get(i)?.cap).map(Tap::Key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Backend, Fail, Greeter, Outcome, Volume};
    use alloc::string::ToString;

    const M: Metrics = Metrics { caption: 18, body: 22, title: 27, big: 37 };

    struct Fake;
    impl Backend for Fake {
        fn volume(&mut self) -> Volume {
            Volume::Open
        }
        fn unlock_volume(&mut self, _: &[u8]) -> Result<(), Fail> {
            Ok(())
        }
        fn accounts(&mut self) -> Option<usize> {
            Some(0)
        }
        fn create_account(&mut self, _: &str, _: &[u8]) -> Result<[u8; 16], Fail> {
            Ok([7; 16])
        }
        fn login(&mut self, _: &str, _: &[u8]) -> Result<(), Fail> {
            Ok(())
        }
        fn reset(&mut self, _: &str, _: &[u8; 16]) -> Result<(), Fail> {
            Ok(())
        }
        fn set_passphrase(&mut self, _: &[u8]) -> Result<(), Fail> {
            Ok(())
        }
    }

    const SCREENS: [(i32, i32); 3] = [(1024, 768), (1080, 2400), (720, 1280)];

    /// Taps the centre of the key whose face says `label`, on a fresh layout.
    fn tap_key(g: &mut Greeter, t: &mut Touch, b: &mut Fake, label: &str, (w, h): (i32, i32)) {
        let v = g.view();
        let l = Layout::new(w, h, &v, &t.osk, M);
        let k = l.keys.iter().find(|k| t.osk.label(k.cap, &v.submit) == label).unwrap_or_else(|| panic!("no key {label:?} on {:?}", t.osk.page));
        let (x, y) = (k.rect.x + k.rect.w / 2, k.rect.y + k.rect.h / 2);
        t.down(x, y, &l);
        match t.up(x, y, &l, &v) {
            Some(Tap::Key(k)) => {
                g.key(k, b);
            }
            Some(Tap::Focus(i)) => g.focus(i),
            None => {}
        }
    }

    fn type_on(g: &mut Greeter, t: &mut Touch, b: &mut Fake, text: &str, s: (i32, i32)) {
        for c in text.chars() {
            let want = c.to_string();
            // a letter that needs a capital goes through Shift; a symbol through the page that has it
            if c.is_ascii_alphabetic() && t.osk.page != Page::Letters {
                tap_key(g, t, b, "abc", s);
            }
            if c.is_ascii_uppercase() && !t.osk.shift {
                tap_key(g, t, b, "Shift", s);
            }
            if !c.is_ascii_alphabetic() && c != ' ' {
                for _ in 0..3 {
                    let l = Layout::new(s.0, s.1, &g.view(), &t.osk, M);
                    if l.keys.iter().any(|k| k.cap == Cap::Char(c as u8)) {
                        break;
                    }
                    let next = match t.osk.page {
                        Page::Letters => "?123",
                        Page::Symbols => "=\\<",
                        Page::MoreSymbols => "?123",
                    };
                    tap_key(g, t, b, next, s);
                }
            }
            if c == ' ' {
                let v = g.view();
                let l = Layout::new(s.0, s.1, &v, &t.osk, M);
                let k = l.keys.iter().find(|k| k.cap == Cap::Space).unwrap();
                let (x, y) = (k.rect.x + 2, k.rect.y + 2);
                t.down(x, y, &l);
                if let Some(Tap::Key(k)) = t.up(x, y, &l, &v) {
                    g.key(k, b);
                }
            } else {
                tap_key(g, t, b, &want, s);
            }
        }
    }

    #[test]
    fn every_printable_character_can_be_typed() {
        let mut seen = [false; 128];
        for page in [Page::Letters, Page::Symbols, Page::MoreSymbols] {
            for shift in [false, true] {
                let o = Osk { shift, page };
                for row in o.rows() {
                    for (cap, _) in row {
                        let mut p = o;
                        if let Some(Key::Char(c)) = p.press(cap) {
                            seen[c as usize] = true;
                        }
                    }
                }
            }
        }
        let missing: Vec<char> = (0x20u8..0x7f).filter(|&c| !seen[c as usize]).map(|c| c as char).collect();
        assert!(missing.is_empty(), "cannot type {missing:?}");
    }

    #[test]
    fn shift_capitalises_one_letter_then_lets_go() {
        let mut o = Osk::default();
        assert_eq!(o.press(Cap::Shift), None);
        assert_eq!(o.press(Cap::Char(b'a')), Some(Key::Char(b'A')));
        assert_eq!(o.press(Cap::Char(b'a')), Some(Key::Char(b'a')));
        o.press(Cap::To(Page::Symbols));
        o.shift = true;
        assert_eq!(o.press(Cap::Char(b'1')), Some(Key::Char(b'1')), "shift does not touch symbols");
    }

    #[test]
    fn keys_fit_the_screen_and_do_not_overlap() {
        for (w, h) in SCREENS {
            for page in [Page::Letters, Page::Symbols, Page::MoreSymbols] {
                let g = Greeter::new(&mut Fake);
                let l = Layout::new(w, h, &g.view(), &Osk { shift: false, page }, M);
                let n: usize = Osk { shift: false, page }.rows().iter().map(Vec::len).sum();
                assert_eq!(l.keys.len(), n, "{page:?}");
                for (i, a) in l.keys.iter().enumerate() {
                    let r = a.rect;
                    assert!(r.x >= 0 && r.y >= 0 && r.right() <= w && r.bottom() <= h, "{w}x{h} {page:?}");
                    assert!(r.w >= 30 && r.h >= 40, "touch target too small: {r:?}");
                    for b in &l.keys[i + 1..] {
                        let (p, q) = (r, b.rect);
                        assert!(p.right() <= q.x || q.right() <= p.x || p.bottom() <= q.y || q.bottom() <= p.y, "{:?} overlaps {:?}", a.cap, b.cap);
                    }
                }
            }
        }
    }

    #[test]
    fn the_text_fits_above_the_keyboard_on_every_screen() {
        // the tallest screens: the recovery screen's four fields and its Back button, and the login screen's two and its button
        for (w, h) in SCREENS {
            let mut g = Greeter::new(&mut Fake);
            for screen in 0..3 {
                if screen == 1 {
                    g.screen = crate::Screen::Login;
                    g.show(crate::Screen::Login);
                } else if screen == 2 {
                    g.show(crate::Screen::Recovery);
                }
                let v = g.view();
                let l = Layout::new(w, h, &v, &Osk::default(), M);
                let top = l.keys.iter().map(|k| k.rect.y).min().unwrap();
                let last = l.actions.last().map(|r| r.bottom()).or(l.fields.last().map(|r| r.bottom())).unwrap();
                assert!(last + M.body <= top, "{w}x{h} {}: content ends at {last}, keyboard starts at {top}", v.title);
                assert!(l.fields.iter().all(|f| f.h >= 44 && f.right() <= w));
            }
        }
    }

    #[test]
    fn the_recovery_key_screen_has_no_keyboard_and_a_button() {
        for (w, h) in SCREENS {
            let mut g = Greeter::new(&mut Fake);
            g.show(crate::Screen::ShowKey);
            g.shown_key = crate::recovery::encode(&[7; 16]);
            let v = g.view();
            let l = Layout::new(w, h, &v, &Osk::default(), M);
            assert!(l.keys.is_empty() && l.fields.is_empty());
            assert_eq!(l.actions.len(), 1);
            assert!(l.actions[0].bottom() <= h);
            assert!(l.key_y + 2 * M.big <= l.actions[0].y);
            // tapping the button goes on, like Enter
            let mut t = Touch::default();
            let (x, y) = (l.actions[0].x + 5, l.actions[0].y + 5);
            t.down(x, y, &l);
            assert_eq!(t.up(x, y, &l, &v), Some(Tap::Key(Key::Enter)));
        }
    }

    #[test]
    fn the_first_account_can_be_made_with_a_finger() {
        for s in SCREENS {
            let (mut g, mut t, mut b) = (Greeter::new(&mut Fake), Touch::default(), Fake);
            assert_eq!(g.view().title, "Welcome");
            type_on(&mut g, &mut t, &mut b, "ana", s);
            tap_key(&mut g, &mut t, &mut b, "Next", s);
            type_on(&mut g, &mut t, &mut b, "Hunter2-pass!", s);
            tap_key(&mut g, &mut t, &mut b, "Next", s);
            type_on(&mut g, &mut t, &mut b, "Hunter2-pass!", s);
            assert_eq!(g.view().submit, "Create");
            tap_key(&mut g, &mut t, &mut b, "Create", s);
            let v = g.view();
            assert_eq!(v.title, "Write down the recovery key", "{s:?}: {:?}", v.message);
            assert!(v.key.is_some());
            // "I have written it down" goes on to the login screen
            let l = Layout::new(s.0, s.1, &v, &t.osk, M);
            let (x, y) = (l.actions[0].x + 5, l.actions[0].y + 5);
            t.down(x, y, &l);
            if let Some(Tap::Key(k)) = t.up(x, y, &l, &v) {
                assert_eq!(g.key(k, &mut b), Outcome::Continue);
            }
            assert_eq!(g.view().title, "Log in");
        }
    }

    #[test]
    fn tapping_a_field_focuses_it_and_forgot_my_passphrase_opens_recovery() {
        let s = (1080, 2400);
        let (mut g, mut t, mut b) = (Greeter::new(&mut Fake), Touch::default(), Fake);
        g.show(crate::Screen::Login);
        let v = g.view();
        assert!(v.fields[0].focused);
        let l = Layout::new(s.0, s.1, &v, &t.osk, M);
        let (x, y) = (l.fields[1].x + 10, l.fields[1].y + 10);
        t.down(x, y, &l);
        assert_eq!(t.up(x, y, &l, &v), Some(Tap::Focus(1)));
        g.focus(1);
        assert!(g.view().fields[1].focused);
        let (x, y) = (l.actions[0].x + 10, l.actions[0].y + 10);
        t.down(x, y, &l);
        let Some(Tap::Key(k)) = t.up(x, y, &l, &v) else { panic!("no key") };
        g.key(k, &mut b);
        assert_eq!(g.view().title, "Recover an account");
        assert_eq!(g.view().actions[0].key, Key::Escape);
    }

    #[test]
    fn sliding_off_a_key_cancels_it() {
        let g = Greeter::new(&mut Fake);
        let v = g.view();
        let mut t = Touch::default();
        let l = Layout::new(1080, 2400, &v, &t.osk, M);
        let r = l.keys[0].rect;
        t.down(r.x + 5, r.y + 5, &l);
        t.moved(5, 5, &l);
        assert_eq!(t.up(5, 5, &l, &v), None);
        // lifting on a different key is no tap either
        let r2 = l.keys[1].rect;
        t.down(r.x + 5, r.y + 5, &l);
        assert_eq!(t.up(r2.x + 5, r2.y + 5, &l, &v), None);
    }

    #[test]
    fn a_tap_in_the_gap_between_keys_still_lands_on_one() {
        let g = Greeter::new(&mut Fake);
        let l = Layout::new(1080, 2400, &g.view(), &Osk::default(), M);
        let (a, b) = (l.keys[0].rect, l.keys[1].rect);
        let gap_x = (a.right() + b.x) / 2;
        assert!(l.hit(gap_x, a.y + 10).is_some());
    }
}
