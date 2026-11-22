//! Time-based one-time passwords (RFC 6238, HMAC-SHA1, 6 digits, 30 s
//! steps), compatible with common authenticator apps, plus the RFC 4648
//! base32 encoding they use for secrets.

use hmac::{Hmac, Mac};
use sha1::Sha1;

pub const STEP_SECS: u64 = 30;
pub const DIGITS: u32 = 6;
/// Steps accepted on either side of the current one (clock drift).
pub const WINDOW: i64 = 1;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &b in data {
        buffer = (buffer << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Decodes base32, ignoring case, spaces and padding.
pub fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let value = ALPHABET
            .iter()
            .position(|&a| a as char == c.to_ascii_uppercase())? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// The code for one time step (HOTP over the step counter).
pub fn code_at(secret: &[u8], step: u64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let binary = (u32::from(digest[offset]) & 0x7f) << 24
        | u32::from(digest[offset + 1]) << 16
        | u32::from(digest[offset + 2]) << 8
        | u32::from(digest[offset + 3]);
    binary % 10u32.pow(DIGITS)
}

pub fn step_at(unix_secs: u64) -> u64 {
    unix_secs / STEP_SECS
}

/// Checks `code` against the steps around `now`. Returns the matching step,
/// which must be later than `last_used` (no replays).
pub fn verify(secret: &[u8], code: &str, unix_secs: u64, last_used: Option<u64>) -> Option<u64> {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let wanted: u32 = code.parse().ok()?;
    let current = step_at(unix_secs) as i64;
    (-WINDOW..=WINDOW)
        .map(|d| current + d)
        .filter(|&s| s >= 0)
        .map(|s| s as u64)
        .filter(|&s| last_used.map_or(true, |l| s > l))
        .find(|&s| code_at(secret, s) == wanted)
}

/// `otpauth://` URI for authenticator apps.
pub fn provisioning_uri(secret_b32: &str, account: &str, issuer: &str) -> String {
    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
    format!(
        "otpauth://totp/{}:{}?secret={secret_b32}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        enc(issuer),
        enc(account),
        enc(issuer)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn rfc6238_sha1_vectors() {
        // RFC 6238 appendix B, truncated to 6 digits.
        for (time, code) in [
            (59u64, 287_082u32),
            (1_111_111_109, 81_804),
            (1_111_111_111, 50_471),
            (1_234_567_890, 5_924),
            (2_000_000_000, 279_037),
        ] {
            assert_eq!(code_at(RFC_SECRET, step_at(time)), code, "t={time}");
        }
    }

    #[test]
    fn verification_window_and_replay() {
        let now = 1_234_567_890;
        let code = format!("{:06}", code_at(RFC_SECRET, step_at(now)));
        assert_eq!(verify(RFC_SECRET, &code, now, None), Some(step_at(now)));
        // Accepted one step later (clock drift), not two.
        assert!(verify(RFC_SECRET, &code, now + 30, None).is_some());
        assert!(verify(RFC_SECRET, &code, now + 61, None).is_none());
        // A used step cannot be used again.
        assert!(verify(RFC_SECRET, &code, now, Some(step_at(now))).is_none());
        assert!(verify(RFC_SECRET, "12345", now, None).is_none());
        assert!(verify(RFC_SECRET, "abcdef", now, None).is_none());
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        assert!(verify(RFC_SECRET, &spaced, now, None).is_some());
    }

    #[test]
    fn base32_round_trip() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("mzxw 6ytb oi==").unwrap(), b"foobar");
        let bytes: Vec<u8> = (0..20).collect();
        assert_eq!(base32_decode(&base32_encode(&bytes)).unwrap(), bytes);
        assert!(base32_decode("1!").is_none());
    }

    #[test]
    fn provisioning_uri_is_escaped() {
        let uri = provisioning_uri("ABC", "a+b@example.com", "QueryLab");
        assert_eq!(
            uri,
            "otpauth://totp/QueryLab:a%2Bb%40example.com?secret=ABC&issuer=QueryLab&algorithm=SHA1&digits=6&period=30"
        );
    }
}
