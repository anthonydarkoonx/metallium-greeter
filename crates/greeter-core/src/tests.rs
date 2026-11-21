extern crate std;
use super::*;
use alloc::string::ToString;
use std::vec::Vec as StdVec;

/// A pretend system: a volume, accounts (name, passphrase, recovery key), and a log of what it was asked.
struct Mock {
    volume: Volume,
    volume_pass: &'static [u8],
    sessions: bool,
    accounts: StdVec<(String, StdVec<u8>, [u8; 16])>,
    logged_in: StdVec<String>,
    resetting: Option<String>,
    asked: StdVec<String>,
}

impl Mock {
    fn new() -> Mock {
        Mock { volume: Volume::Open, volume_pass: b"volume-pass", sessions: true, accounts: StdVec::new(), logged_in: StdVec::new(), resetting: None, asked: StdVec::new() }
    }
}

impl Backend for Mock {
    fn volume(&mut self) -> Volume {
        self.volume
    }
    fn unlock_volume(&mut self, p: &[u8]) -> Result<(), Fail> {
        self.asked.push("unlock".to_string());
        if p == self.volume_pass {
            self.volume = Volume::Open;
            Ok(())
        } else {
            Err(Fail::Wrong)
        }
    }
    fn accounts(&mut self) -> Option<usize> {
        self.sessions.then_some(self.accounts.len())
    }
    fn create_account(&mut self, name: &str, p: &[u8]) -> Result<[u8; 16], Fail> {
        self.asked.push(std::format!("create {name}"));
        if self.accounts.iter().any(|a| a.0 == name) {
            return Err(Fail::Exists);
        }
        let key = [self.accounts.len() as u8 + 1; 16];
        self.accounts.push((name.to_string(), p.to_vec(), key));
        Ok(key)
    }
    fn login(&mut self, name: &str, p: &[u8]) -> Result<(), Fail> {
        self.asked.push(std::format!("login {name}"));
        match self.accounts.iter().find(|a| a.0 == name && a.1 == p) {
            None => Err(Fail::Wrong),
            Some(_) if self.logged_in.iter().any(|n| n == name) => Err(Fail::Busy),
            Some(_) => {
                self.logged_in.push(name.to_string());
                Ok(())
            }
        }
    }
    fn reset(&mut self, name: &str, key: &[u8; 16]) -> Result<(), Fail> {
        self.asked.push(std::format!("reset {name}"));
        match self.accounts.iter().find(|a| a.0 == name && &a.2 == key) {
            Some(_) => {
                self.resetting = Some(name.to_string());
                Ok(())
            }
            None => Err(Fail::Wrong),
        }
    }
    fn set_passphrase(&mut self, p: &[u8]) -> Result<(), Fail> {
        self.asked.push("setpass".to_string());
        let name = self.resetting.take().ok_or(Fail::Denied)?;
        self.accounts.iter_mut().find(|a| a.0 == name).unwrap().1 = p.to_vec();
        Ok(())
    }
}

fn type_text(g: &mut Greeter, m: &mut Mock, t: &str) {
    for c in t.bytes() {
        g.key(Key::from_byte(c).unwrap(), m);
    }
}

fn line(g: &mut Greeter, m: &mut Mock, t: &str) {
    type_text(g, m, t);
    g.key(Key::Enter, m);
}

fn msg(g: &Greeter) -> String {
    g.view().message.map(|m| m.text).unwrap_or_default()
}

#[test]
fn the_recovery_key_round_trips_and_is_forgiving() {
    for key in [[0u8; 16], [0xFF; 16], [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x0F, 0xED, 0xCB, 0xA9, 0x87, 0x65, 0x43, 0x21]] {
        let text = recovery::encode(&key);
        assert_eq!(text.len(), 26 + 6, "26 characters and six dashes: {text}");
        assert_eq!(recovery::decode(&text), Some(key));
        assert_eq!(recovery::decode(&text.to_lowercase()), Some(key), "case does not matter");
        assert_eq!(recovery::decode(&text.replace('-', " ")), Some(key), "spaces for dashes");
        assert_eq!(recovery::decode(&text.replace('-', "")), Some(key), "no dashes");
    }
    // look-alikes read as their digits
    let key = [0u8; 16];
    assert_eq!(recovery::decode(&recovery::encode(&key).replace('0', "O")), Some(key));
    let one = recovery::encode(&[0x08; 16]); // contains 1s
    assert_eq!(recovery::decode(&one.replace('1', "l")), Some([0x08; 16]));
    // wrong length, bad characters, spare bits set
    assert_eq!(recovery::decode(""), None);
    assert_eq!(recovery::decode("0000-0000"), None);
    assert_eq!(recovery::decode(&std::format!("{}0", recovery::encode(&key))), None, "too long");
    assert_eq!(recovery::decode(&recovery::encode(&key).replace("0000", "U000")), None, "U is not in the alphabet");
    let mut t = recovery::encode(&key).into_bytes();
    let last = t.len() - 1;
    t[last] = b'3'; // the last character carries two spare bits that must be zero
    assert_eq!(recovery::decode(core::str::from_utf8(&t).unwrap()), None);
}

#[test]
fn what_comes_first_depends_on_the_machine() {
    let mut m = Mock::new();
    m.sessions = false;
    assert!(Greeter::new(&mut m).is_done(), "no session service and an open volume: nothing to ask");
    m.volume = Volume::Locked;
    assert_eq!(Greeter::new(&mut m).view().title, "Unlock");
    m.volume = Volume::Damaged;
    let g = Greeter::new(&mut m);
    assert_eq!(g.view().title, "The data volume cannot be read");
    let mut g = g;
    assert_eq!(g.key(Key::Enter, &mut m), Outcome::Continue, "a damaged volume is not clicked away");
    m.volume = Volume::Open;
    m.sessions = true;
    assert_eq!(Greeter::new(&mut m).view().title, "Welcome", "no accounts: first-boot setup");
    m.accounts.push(("a".to_string(), b"x".to_vec(), [1; 16]));
    assert_eq!(Greeter::new(&mut m).view().title, "Log in");
}

#[test]
fn the_volume_is_unlocked_then_the_next_screen_follows() {
    let mut m = Mock::new();
    m.volume = Volume::Locked;
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "wrong");
    assert_eq!(msg(&g), "Wrong passphrase");
    assert_eq!(g.view().fields[0].text, "", "the wrong passphrase is gone");
    line(&mut g, &mut m, "volume-pass");
    assert_eq!(g.view().title, "Welcome", "unlocked: the machine has no accounts, so setup follows");
    // someone else unlocking it while the unlock screen is up
    let mut m = Mock::new();
    m.volume = Volume::Locked;
    let mut g = Greeter::new(&mut m);
    m.volume = Volume::Open;
    g.poll(&mut m);
    assert_eq!(g.view().title, "Welcome");
}

#[test]
fn secrets_are_never_drawn() {
    let mut m = Mock::new();
    m.accounts.push(("alice".to_string(), b"alice-passphrase".to_vec(), [1; 16]));
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "alice");
    type_text(&mut g, &mut m, "hunter22");
    let v = g.view();
    assert_eq!(v.fields[0].text, "alice");
    assert_eq!(v.fields[1].text, "********");
    assert!(v.fields[1].focused && !v.fields[0].focused);
    g.key(Key::Backspace, &mut m);
    assert_eq!(g.view().fields[1].text, "*******");
}

#[test]
fn first_boot_makes_the_administrator_and_shows_the_key_once() {
    let mut m = Mock::new();
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "Bad Name");
    assert!(msg(&g).starts_with("A name is 1 to 16"), "{}", msg(&g));
    for _ in 0..8 {
        g.key(Key::Backspace, &mut m);
    }
    line(&mut g, &mut m, "root");
    line(&mut g, &mut m, "short");
    assert_eq!(msg(&g), "Use at least 8 characters");
    for _ in 0..5 {
        g.key(Key::Backspace, &mut m);
    }
    line(&mut g, &mut m, "a-good-passphrase");
    line(&mut g, &mut m, "a-good-passphrasE");
    assert_eq!(msg(&g), "The two passphrases differ");
    assert!(m.asked.is_empty(), "nothing was asked of the system yet");
    line(&mut g, &mut m, "a-good-passphrase");
    assert_eq!(m.asked, ["create root"]);
    let v = g.view();
    assert_eq!(v.title, "Write down the recovery key");
    assert_eq!(v.key.as_deref(), Some(recovery::encode(&[1; 16]).as_str()));
    // the key must be read, not just clicked past
    line(&mut g, &mut m, "zzzz");
    assert_eq!(msg(&g), "That is not the last group of the key");
    let last = recovery::encode(&[1; 16]).rsplit('-').next().unwrap().to_lowercase();
    line(&mut g, &mut m, &last);
    let v = g.view();
    assert_eq!((v.title.as_str(), v.fields[0].text.as_str(), v.fields[1].focused), ("Log in", "root", true), "the login screen has the name filled in and waits for the passphrase");
    assert_eq!(v.key, None, "the key is gone from the screen");
    assert_eq!(msg(&g), "Account created. Log in.");
}

#[test]
fn a_taken_name_is_said_so_and_nothing_secret_stays_in_the_fields() {
    let mut m = Mock::new();
    m.accounts.push(("root".to_string(), b"x".to_vec(), [9; 16]));
    m.sessions = true;
    let mut g = Greeter::new(&mut m);
    // (with an account there is no setup: reach it as a fresh machine instead)
    let mut m2 = Mock::new();
    let mut g2 = Greeter::new(&mut m2);
    m2.accounts.push(("root".to_string(), b"x".to_vec(), [9; 16]));
    line(&mut g2, &mut m2, "root");
    line(&mut g2, &mut m2, "long-enough-1");
    line(&mut g2, &mut m2, "long-enough-1");
    assert_eq!(msg(&g2), "That name is taken");
    let v = g2.view();
    assert_eq!((v.fields[1].text.as_str(), v.fields[2].text.as_str()), ("", ""), "the passphrases were wiped");
    let _ = &mut g;
}

#[test]
fn logging_in() {
    let mut m = Mock::new();
    m.accounts.push(("alice".to_string(), b"alice-passphrase".to_vec(), [1; 16]));
    m.accounts.push(("bob".to_string(), b"bob-passphrase".to_vec(), [2; 16]));
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "alice");
    line(&mut g, &mut m, "not-it");
    assert_eq!(msg(&g), "Wrong user name or passphrase");
    assert!(g.view().fields[1].focused && g.view().fields[1].text.is_empty());
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "nobody");
    line(&mut g, &mut m, "alice-passphrase");
    assert_eq!(msg(&g), "Wrong user name or passphrase", "the same answer for a name that does not exist");
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "alice");
    assert_eq!(line_outcome(&mut g, &mut m, "alice-passphrase"), Outcome::Done);
    assert_eq!(m.logged_in, ["alice"]);
    let mut g = Greeter::new(&mut m);
    line(&mut g, &mut m, "alice");
    line(&mut g, &mut m, "alice-passphrase");
    assert_eq!(msg(&g), "That user is already logged in on another terminal");
    // Tab goes back to the name
    g.key(Key::Tab, &mut m);
    assert!(g.view().fields[0].focused);
}

fn line_outcome(g: &mut Greeter, m: &mut Mock, t: &str) -> Outcome {
    type_text(g, m, t);
    g.key(Key::Enter, m)
}

#[test]
fn a_forgotten_passphrase_is_replaced_with_the_recovery_key() {
    let mut m = Mock::new();
    m.accounts.push(("alice".to_string(), b"old-passphrase".to_vec(), [7; 16]));
    let mut g = Greeter::new(&mut m);
    g.key(Key::Recover, &mut m);
    assert_eq!(g.view().title, "Recover an account");
    let key = recovery::encode(&[7; 16]);
    line(&mut g, &mut m, "alice");
    line(&mut g, &mut m, "nonsense");
    line(&mut g, &mut m, "brand-new-pass");
    line(&mut g, &mut m, "brand-new-pasS");
    assert!(msg(&g).starts_with("That is not a recovery key"), "{}", msg(&g));
    assert!(m.asked.is_empty(), "nothing is asked of the system with a key that cannot be read");
    // fix the key field (Tab: confirm -> name -> key) and the confirmation (Tab, Tab: key -> new -> again)
    g.key(Key::Tab, &mut m);
    g.key(Key::Tab, &mut m);
    for _ in 0..8 {
        g.key(Key::Backspace, &mut m);
    }
    type_text(&mut g, &mut m, &key.to_lowercase());
    g.key(Key::Tab, &mut m);
    g.key(Key::Tab, &mut m);
    for _ in 0..20 {
        g.key(Key::Backspace, &mut m);
    }
    type_text(&mut g, &mut m, "brand-new-pass");
    g.key(Key::Enter, &mut m);
    assert_eq!(m.asked, ["reset alice", "setpass"]);
    let v = g.view();
    assert_eq!((v.title.as_str(), v.fields[0].text.as_str()), ("Log in", "alice"));
    assert_eq!(msg(&g), "Passphrase changed. Log in.");
    assert_eq!(line_outcome(&mut g, &mut m, "brand-new-pass"), Outcome::Done, "the new passphrase logs in");
}

#[test]
fn a_wrong_recovery_key_and_the_way_back() {
    let mut m = Mock::new();
    m.accounts.push(("alice".to_string(), b"old-passphrase".to_vec(), [7; 16]));
    let mut g = Greeter::new(&mut m);
    g.key(Key::Recover, &mut m);
    line(&mut g, &mut m, "alice");
    line(&mut g, &mut m, &recovery::encode(&[8; 16]));
    line(&mut g, &mut m, "brand-new-pass");
    line(&mut g, &mut m, "brand-new-pass");
    assert_eq!(msg(&g), "Wrong user name or recovery key");
    assert_eq!(m.asked, ["reset alice"], "the passphrase was not touched");
    g.key(Key::Escape, &mut m);
    assert_eq!(g.view().title, "Log in");
    assert_eq!(m.accounts[0].1, b"old-passphrase");
}

#[test]
fn fields_have_limits_and_keys_that_mean_nothing_are_ignored() {
    assert_eq!(Key::from_byte(0x01), None);
    assert_eq!(Key::from_byte(0x80), None, "arrow keys");
    assert_eq!(Key::from_byte(b'\r'), Some(Key::Enter));
    assert_eq!(Key::from_byte(0x12), Some(Key::Recover));
    let mut m = Mock::new();
    m.accounts.push(("alice".to_string(), b"x".to_vec(), [1; 16]));
    let mut g = Greeter::new(&mut m);
    type_text(&mut g, &mut m, &"a".repeat(40));
    assert_eq!(g.view().fields[0].text.len(), MAX_NAME);
    g.key(Key::Tab, &mut m);
    type_text(&mut g, &mut m, &"p".repeat(200));
    assert_eq!(g.view().fields[1].text.len(), MAX_PASSPHRASE);
}
