//! The greeter's screens as a state machine. It knows nothing about drawing or system calls: it takes keys, asks a [`Backend`] to do the things the system does (unlock the data
//! volume, make an account, log in, recover a passphrase), and describes what to show as a [`View`]. The Metallium-OS adapter (`metallium-greeter-boot`) draws that and implements
//! the backend; the tests here use a mock.
//!
//! Screens: **unlock** (the data volume is encrypted and locked), **setup** (the first boot: no accounts yet; makes the administrator and shows the recovery key once), **log in**,
//! **recovery** (a forgotten passphrase, with the recovery key) and, when nothing is left to ask, **done**.
#![no_std]
extern crate alloc;

pub mod recovery;
pub mod touch;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// A key press, as the input driver's character stream gives it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    /// A printable ASCII character.
    Char(u8),
    Backspace,
    Enter,
    Tab,
    Escape,
    /// Ctrl+R: "I forgot my passphrase".
    Recover,
}

impl Key {
    /// From a byte of the keyboard driver's stream (`KBD_POLL`): `None` for what the greeter ignores.
    pub fn from_byte(b: u8) -> Option<Key> {
        match b {
            0x20..=0x7e => Some(Key::Char(b)),
            0x08 | 0x7f => Some(Key::Backspace),
            b'\n' | b'\r' => Some(Key::Enter),
            b'\t' => Some(Key::Tab),
            0x1b => Some(Key::Escape),
            0x12 => Some(Key::Recover),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fail {
    /// A wrong passphrase, recovery key or user name (one answer for all, so nothing says which names exist).
    Wrong,
    /// The user already has a session.
    Busy,
    Exists,
    Denied,
    Bad,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Volume {
    /// Not encrypted, or encrypted and open: nothing to ask.
    Open,
    Locked,
    /// Encrypted, but it does not read: it is never formatted over, and nothing can be done here.
    Damaged,
}

/// What the greeter asks the system to do.
pub trait Backend {
    fn volume(&mut self) -> Volume;
    fn unlock_volume(&mut self, passphrase: &[u8]) -> Result<(), Fail>;
    /// How many accounts there are; `None` if the system has no session service (the greeter then only unlocks the volume).
    fn accounts(&mut self) -> Option<usize>;
    /// Make an account (the first one is the administrator); the recovery key, to be shown once.
    fn create_account(&mut self, name: &str, passphrase: &[u8]) -> Result<[u8; 16], Fail>;
    fn login(&mut self, name: &str, passphrase: &[u8]) -> Result<(), Fail>;
    /// Open an account's files with its recovery key, for a `set_passphrase` that follows.
    fn reset(&mut self, name: &str, key: &[u8; 16]) -> Result<(), Fail>;
    fn set_passphrase(&mut self, passphrase: &[u8]) -> Result<(), Fail>;
}

pub const MIN_PASSPHRASE: usize = 8;
pub const MAX_PASSPHRASE: usize = 120;
pub const MAX_NAME: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Unlock,
    Setup,
    ShowKey,
    Login,
    Recovery,
    Damaged,
    Done,
}

struct Field {
    label: &'static str,
    value: Vec<u8>,
    secret: bool,
    max: usize,
}

impl Field {
    fn new(label: &'static str, secret: bool, max: usize) -> Field {
        Field { label, value: Vec::new(), secret, max }
    }
    fn wipe(&mut self) {
        self.value.iter_mut().for_each(|b| *b = 0);
        self.value.clear();
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.value).into_owned()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ViewField {
    pub label: String,
    /// What to draw: the text, or one `*` for every character of a secret.
    pub text: String,
    pub focused: bool,
}

/// A button the screen offers besides its fields: pressing it is the same as pressing `key` (a touch screen has no Ctrl+R or Esc).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ViewAction {
    pub label: String,
    pub key: Key,
}

/// What to show.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct View {
    pub title: String,
    pub intro: Vec<String>,
    pub fields: Vec<ViewField>,
    /// The recovery key, big, on the screen that shows it (once).
    pub key: Option<String>,
    pub message: Option<Message>,
    pub hint: String,
    /// Buttons for what the keyboard does with a shortcut (forgot my passphrase, back) or has no key for (continue after the recovery key).
    pub actions: Vec<ViewAction>,
    /// What Enter does on this field: "Next", "Log in"... (the label of a touch keyboard's Enter key).
    pub submit: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Continue,
    /// Nothing is left to ask: the system carries on.
    Done,
}

pub struct Greeter {
    screen: Screen,
    fields: Vec<Field>,
    focus: usize,
    message: Option<Message>,
    /// The recovery key being shown, as written down.
    shown_key: String,
    /// The name just made, for the login screen that follows the key screen.
    last_name: String,
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= MAX_NAME && n.as_bytes()[0].is_ascii_lowercase() && n.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
}

/// A character as a person would read it back: case does not matter, and the look-alikes are their digits.
fn normal(c: u8) -> u8 {
    match c.to_ascii_uppercase() {
        b'O' => b'0',
        b'I' | b'L' => b'1',
        c => c,
    }
}

impl Greeter {
    /// The first screen for the machine's state.
    pub fn new(b: &mut dyn Backend) -> Greeter {
        let mut g = Greeter { screen: Screen::Done, fields: Vec::new(), focus: 0, message: None, shown_key: String::new(), last_name: String::new() };
        g.decide(b);
        g
    }

    /// Focus field `i` (a tap on it); ignored if there is no such field.
    pub fn focus(&mut self, i: usize) {
        if i < self.fields.len() {
            self.focus = i;
        }
    }

    pub fn is_done(&self) -> bool {
        self.screen == Screen::Done
    }

    /// Which screen comes next, from what the system says.
    fn decide(&mut self, b: &mut dyn Backend) {
        match b.volume() {
            Volume::Locked => return self.show(Screen::Unlock),
            Volume::Damaged => return self.show(Screen::Damaged),
            Volume::Open => {}
        }
        match b.accounts() {
            None => self.show(Screen::Done),
            Some(0) => self.show(Screen::Setup),
            Some(_) => self.show(Screen::Login),
        }
    }

    fn show(&mut self, s: Screen) {
        for f in &mut self.fields {
            f.wipe();
        }
        self.screen = s;
        self.focus = 0;
        self.message = None;
        self.fields = match s {
            Screen::Unlock => vec![Field::new("Passphrase", true, MAX_PASSPHRASE)],
            Screen::Setup => vec![Field::new("User name", false, MAX_NAME), Field::new("Passphrase", true, MAX_PASSPHRASE), Field::new("Again", true, MAX_PASSPHRASE)],
            Screen::ShowKey => Vec::new(),
            Screen::Login => vec![Field::new("User name", false, MAX_NAME), Field::new("Passphrase", true, MAX_PASSPHRASE)],
            Screen::Recovery => vec![
                Field::new("User name", false, MAX_NAME),
                Field::new("Recovery key", false, 40),
                Field::new("New passphrase", true, MAX_PASSPHRASE),
                Field::new("Again", true, MAX_PASSPHRASE),
            ],
            Screen::Damaged | Screen::Done => Vec::new(),
        };
    }

    fn note(&mut self, text: &str, error: bool) {
        self.message = Some(Message { text: String::from(text), error });
    }

    /// Call now and then: if someone else unlocked the volume, the unlock screen goes away.
    pub fn poll(&mut self, b: &mut dyn Backend) -> Outcome {
        if self.screen == Screen::Unlock && b.volume() != Volume::Locked {
            self.decide(b);
        }
        self.outcome()
    }

    fn outcome(&self) -> Outcome {
        if self.screen == Screen::Done {
            Outcome::Done
        } else {
            Outcome::Continue
        }
    }

    pub fn key(&mut self, k: Key, b: &mut dyn Backend) -> Outcome {
        match (self.screen, k) {
            (Screen::Done | Screen::Damaged, _) => {}
            (Screen::Login, Key::Recover) => {
                self.show(Screen::Recovery);
            }
            (Screen::Recovery, Key::Escape) => {
                self.show(Screen::Login);
            }
            (_, Key::Char(c)) => {
                if let Some(f) = self.fields.get_mut(self.focus) {
                    if f.value.len() < f.max {
                        f.value.push(c);
                    }
                }
                self.message = None;
            }
            (_, Key::Backspace) => {
                if let Some(f) = self.fields.get_mut(self.focus) {
                    f.value.pop();
                }
                self.message = None;
            }
            (_, Key::Tab) => {
                if !self.fields.is_empty() {
                    self.focus = (self.focus + 1) % self.fields.len();
                }
            }
            (_, Key::Enter) => self.enter(b),
            _ => {}
        }
        self.outcome()
    }

    fn enter(&mut self, b: &mut dyn Backend) {
        match self.screen {
            Screen::Unlock => {
                if self.fields[0].value.is_empty() {
                    return;
                }
                let r = b.unlock_volume(&self.fields[0].value);
                self.fields[0].wipe();
                match r {
                    Ok(()) => self.decide(b),
                    Err(Fail::Wrong) => self.note("Wrong passphrase", true),
                    Err(_) => self.note("The volume could not be unlocked", true),
                }
            }
            Screen::Setup => self.enter_setup(b),
            Screen::ShowKey => {
                self.shown_key.clear();
                let name = core::mem::take(&mut self.last_name);
                self.show(Screen::Login);
                self.fields[0].value = name.into_bytes();
                self.focus = 1;
                self.note("Account created. Log in.", false);
            }
            Screen::Login => {
                if self.focus == 0 {
                    if !self.fields[0].value.is_empty() {
                        self.focus = 1;
                    }
                    return;
                }
                let name = self.fields[0].text();
                let r = b.login(&name, &self.fields[1].value);
                self.fields[1].wipe();
                match r {
                    Ok(()) => self.show(Screen::Done),
                    Err(Fail::Wrong) => {
                        self.focus = 1;
                        self.note("Wrong user name or passphrase", true)
                    }
                    Err(Fail::Busy) => self.note("That user is already logged in on another terminal", true),
                    Err(_) => self.note("Could not log in", true),
                }
            }
            Screen::Recovery => self.enter_recovery(b),
            Screen::Damaged | Screen::Done => {}
        }
    }

    fn enter_setup(&mut self, b: &mut dyn Backend) {
        match self.focus {
            0 => {
                if valid_name(&self.fields[0].text()) {
                    self.focus = 1;
                } else {
                    self.note("A name is 1 to 16 characters: lower case letters, digits, - and _, starting with a letter", true);
                }
            }
            1 => {
                if self.fields[1].value.len() < MIN_PASSPHRASE {
                    self.note("Use at least 8 characters", true);
                } else {
                    self.focus = 2;
                }
            }
            _ => {
                if self.fields[1].value != self.fields[2].value {
                    self.fields[2].wipe();
                    self.note("The two passphrases differ", true);
                    return;
                }
                let name = self.fields[0].text();
                let r = b.create_account(&name, &self.fields[1].value);
                self.fields[1].wipe();
                self.fields[2].wipe();
                match r {
                    Ok(key) => {
                        self.shown_key = recovery::encode(&key);
                        self.last_name = name;
                        self.show(Screen::ShowKey);
                    }
                    Err(Fail::Exists) => self.note("That name is taken", true),
                    Err(Fail::Bad) => self.note("That name is not allowed", true),
                    Err(Fail::Denied) => self.note("Only an administrator can make accounts", true),
                    Err(_) => self.note("Could not make the account", true),
                }
            }
        }
    }

    fn enter_recovery(&mut self, b: &mut dyn Backend) {
        if self.focus < 3 {
            if !self.fields[self.focus].value.is_empty() {
                self.focus += 1;
            }
            return;
        }
        let key = recovery::decode(&self.fields[1].text());
        let Some(key) = key else {
            return self.note("That is not a recovery key (26 letters and digits)", true);
        };
        if self.fields[2].value.len() < MIN_PASSPHRASE {
            return self.note("Use at least 8 characters", true);
        }
        if self.fields[2].value != self.fields[3].value {
            self.fields[3].wipe();
            return self.note("The two passphrases differ", true);
        }
        let name = self.fields[0].text();
        match b.reset(&name, &key) {
            Ok(()) => {}
            Err(Fail::Wrong) => return self.note("Wrong user name or recovery key", true),
            Err(Fail::Busy) => return self.note("That user is logged in on another terminal", true),
            Err(_) => return self.note("Could not use the recovery key", true),
        }
        let r = b.set_passphrase(&self.fields[2].value);
        self.fields[2].wipe();
        self.fields[3].wipe();
        match r {
            Ok(()) => {
                self.show(Screen::Login);
                self.fields[0].value = name.into_bytes();
                self.focus = 1;
                self.note("Passphrase changed. Log in.", false);
            }
            Err(_) => self.note("Could not change the passphrase", true),
        }
    }

    pub fn view(&self) -> View {
        let (title, intro, hint): (&str, &[&str], &str) = match self.screen {
            Screen::Unlock => ("Unlock", &["The data volume is encrypted."], "Enter: unlock"),
            Screen::Setup => ("Welcome", &["This machine has no accounts yet.", "Make the administrator's."], "Enter: next    Tab: switch field"),
            Screen::ShowKey => (
                "Write down the recovery key",
                &["If you forget your passphrase, this key is the only way back in.", "It is shown once and kept nowhere. Write it on paper."],
                "Enter: continue",
            ),
            Screen::Login => ("Log in", &[], "Enter: next    Tab: switch field    Ctrl+R: forgot my passphrase"),
            Screen::Recovery => ("Recover an account", &["Use the recovery key written down when the account was made."], "Enter: next    Esc: back"),
            Screen::Damaged => ("The data volume cannot be read", &["It is encrypted and its store does not read.", "Nothing is formatted over it."], ""),
            Screen::Done => ("", &[], ""),
        };
        let action = |label: &str, key| ViewAction { label: String::from(label), key };
        let actions = match self.screen {
            Screen::ShowKey => vec![action("I have written it down", Key::Enter)],
            Screen::Login => vec![action("Forgot my passphrase", Key::Recover)],
            Screen::Recovery => vec![action("Back", Key::Escape)],
            _ => Vec::new(),
        };
        let last = self.focus + 1 >= self.fields.len();
        let submit = match self.screen {
            Screen::Unlock => "Unlock",
            Screen::Setup if last => "Create",
            Screen::Login if last => "Log in",
            Screen::Recovery if last => "Recover",
            _ => "Next",
        };
        View {
            actions,
            submit: String::from(submit),
            title: String::from(title),
            intro: intro.iter().map(|s| String::from(*s)).collect(),
            fields: self
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| ViewField { label: String::from(f.label), text: if f.secret { f.value.iter().map(|_| '*').collect() } else { f.text() }, focused: i == self.focus })
                .collect(),
            key: (self.screen == Screen::ShowKey).then(|| self.shown_key.clone()),
            message: self.message.clone(),
            hint: String::from(hint),
        }
    }
}

#[cfg(test)]
mod tests;
