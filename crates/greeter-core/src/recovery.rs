//! The recovery key as a person handles it: 128 bits written as 26 base-32 characters (Crockford's alphabet: no I, L, O or U, and the look-alikes read as their
//! digits), shown in groups of four. Reading it back ignores case, dashes and spaces.
use alloc::string::String;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const CHARS: usize = 26;

/// The key as written down: `ABCD-EFGH-...` (26 characters in groups of four, the last of two).
pub fn encode(key: &[u8; 16]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits, mut n) = (0u32, 0u32, 0usize);
    let push = |c: u8, n: &mut usize, out: &mut String| {
        if *n > 0 && n.is_multiple_of(4) {
            out.push('-');
        }
        out.push(c as char);
        *n += 1;
    };
    for &b in key {
        acc = acc << 8 | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            push(ALPHABET[(acc >> bits & 31) as usize], &mut n, &mut out);
        }
    }
    if bits > 0 {
        push(ALPHABET[(acc << (5 - bits) & 31) as usize], &mut n, &mut out);
    }
    out
}

/// The 16 bytes from what a person typed; `None` if it is not exactly 26 characters of the alphabet (look-alikes accepted) or the two spare bits are not zero.
pub fn decode(text: &str) -> Option<[u8; 16]> {
    let mut key = [0u8; 16];
    let (mut acc, mut bits, mut at, mut count) = (0u32, 0u32, 0usize, 0usize);
    for c in text.bytes() {
        if c == b'-' || c == b' ' {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            b'O' => b'0',
            b'I' | b'L' => b'1',
            c => c,
        };
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        count += 1;
        if count > CHARS {
            return None;
        }
        acc = acc << 5 | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            if at >= 16 {
                return None;
            }
            key[at] = (acc >> bits) as u8;
            at += 1;
        }
    }
    // 26 characters carry 130 bits: 128 of key and two spare, which must be zero (a cheap check for a mistyped last character)
    (count == CHARS && at == 16 && acc & ((1 << bits) - 1) == 0).then_some(key)
}
