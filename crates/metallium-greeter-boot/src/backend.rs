//! The greeter's backend on Metallium-OS: the data store's lock (`proto::volume`) and the session service (`sessiond`, programs/src/bin/sessiond.rs in metallium-os).
use alloc::string::String;
use greeter_core::{Backend, Fail, Volume};
use metallium_driver_rt::call;
use metallium_driver_rt::proto::volume::{state, LFS_STATUS, LFS_UNLOCK, MORE};

// the session service's protocol
const OP_BEGIN: u64 = 1;
const OP_SECRET: u64 = 2;
const OP_STATUS: u64 = 4;
const ACT_CREATE: u64 = 1;
const ACT_LOGIN: u64 = 2;
const ACT_RESET: u64 = 3;
const ACT_SETPASS: u64 = 4;
// its statuses
const S_OK: u64 = 0;
const S_BAD: u64 = 1;
const S_EXISTS: u64 = 2;
const S_MORE: u64 = 4;
const S_DENIED: u64 = 5;
const S_LOGIN: u64 = 7;
const S_BUSY: u64 = 8;

pub struct OsBackend {
    /// The data store's endpoint.
    pub store: u64,
    /// The session service's, or 0.
    pub sessions: u64,
}

/// Send `secret` in chunks of 16 bytes; `first` builds the first word of a chunk from (total length, offset). The last reply (the earlier ones are "more").
fn chunks(ep: u64, secret: &[u8], first: impl Fn(u64, u64) -> u64, more: u64) -> Option<[u64; 3]> {
    let mut last = None;
    for off in (0..secret.len().max(1)).step_by(16) {
        let mut chunk = [0u8; 16];
        let n = secret.len().saturating_sub(off).min(16);
        chunk[..n].copy_from_slice(&secret[off..off + n]);
        let w0 = first(secret.len() as u64, off as u64);
        let (st, r) = call(ep, [w0, u64::from_le_bytes(chunk[..8].try_into().unwrap()), u64::from_le_bytes(chunk[8..].try_into().unwrap())]);
        if st != 0 {
            return None;
        }
        last = Some(r);
        if r[0] != more {
            break;
        }
    }
    last
}

impl OsBackend {
    /// A session-service request: BEGIN (the action and the name), then the secret.
    fn request(&self, action: u64, name: &str, secret: &[u8]) -> Option<[u64; 3]> {
        if self.sessions == 0 {
            return None;
        }
        let mut raw = [0u8; 16];
        let n = name.len().min(16);
        raw[..n].copy_from_slice(&name.as_bytes()[..n]);
        let (st, r) = call(self.sessions, [OP_BEGIN | action << 8, u64::from_le_bytes(raw[..8].try_into().unwrap()), u64::from_le_bytes(raw[8..].try_into().unwrap())]);
        if st != 0 || r[0] != S_OK {
            return None;
        }
        chunks(self.sessions, secret, |total, off| OP_SECRET | total << 8 | off << 24, S_MORE)
    }
}

fn fail(status: u64) -> Fail {
    match status {
        S_LOGIN => Fail::Wrong,
        S_BUSY => Fail::Busy,
        S_EXISTS => Fail::Exists,
        S_DENIED => Fail::Denied,
        S_BAD => Fail::Bad,
        _ => Fail::Other,
    }
}

impl Backend for OsBackend {
    fn volume(&mut self) -> Volume {
        match call(self.store, [LFS_STATUS, 0, 0]) {
            (0, r) if r[0] == state::LOCKED => Volume::Locked,
            (0, r) if r[0] == state::DAMAGED => Volume::Damaged,
            _ => Volume::Open,
        }
    }

    fn unlock_volume(&mut self, passphrase: &[u8]) -> Result<(), Fail> {
        match chunks(self.store, passphrase, |total, off| LFS_UNLOCK | total << 8 | off << 24, MORE) {
            Some(r) if r[0] == 0 => Ok(()),
            Some(r) if r[0] == 1 => Err(Fail::Wrong),
            _ => Err(Fail::Other),
        }
    }

    fn accounts(&mut self) -> Option<usize> {
        if self.sessions == 0 {
            return None;
        }
        match call(self.sessions, [OP_STATUS, 0, 0]) {
            (0, r) => Some(r[0] as usize),
            _ => None,
        }
    }

    fn create_account(&mut self, name: &str, passphrase: &[u8]) -> Result<[u8; 16], Fail> {
        let r = self.request(ACT_CREATE, name, passphrase).ok_or(Fail::Other)?;
        if r[0] & 0xFF != S_OK {
            return Err(fail(r[0] & 0xFF));
        }
        let mut key = [0u8; 16];
        key[..8].copy_from_slice(&r[1].to_le_bytes());
        key[8..].copy_from_slice(&r[2].to_le_bytes());
        #[cfg(feature = "log-key")]
        metallium_driver_rt::println!("[greeter] test: recovery key {}", greeter_core::recovery::encode(&key));
        Ok(key)
    }

    fn login(&mut self, name: &str, passphrase: &[u8]) -> Result<(), Fail> {
        let r = self.request(ACT_LOGIN, name, passphrase).ok_or(Fail::Other)?;
        if r[0] == S_OK { Ok(()) } else { Err(fail(r[0])) }
    }

    fn reset(&mut self, name: &str, key: &[u8; 16]) -> Result<(), Fail> {
        let r = self.request(ACT_RESET, name, key).ok_or(Fail::Other)?;
        match r[0] {
            S_OK => Ok(()),
            S_BAD => Err(Fail::Busy), // (the service refuses a reset for a user who has a session)
            s => Err(fail(s)),
        }
    }

    fn set_passphrase(&mut self, passphrase: &[u8]) -> Result<(), Fail> {
        let r = self.request(ACT_SETPASS, &String::new(), passphrase).ok_or(Fail::Other)?;
        if r[0] == S_OK { Ok(()) } else { Err(fail(r[0])) }
    }
}
