//! One-time codes (RFC 6238) for logins with an authenticator key, the way
//! Bitwarden reads them: a bare base32 secret, an `otpauth://totp/…` URI with
//! its own digits, period and algorithm, or `steam://<secret>` for Steam Guard.

use hmac::{Hmac, Mac};
use zeroize::Zeroizing;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug)]
pub struct Totp {
    secret: Zeroizing<Vec<u8>>,
    pub digits: u32,
    pub period: u64,
    pub algorithm: Algorithm,
    pub steam: bool,
}

const STEAM_CHARS: &[u8] = b"23456789BCDFGHJKMNPQRTVWXY";

impl Totp {
    pub fn parse(text: &str) -> Result<Totp, Error> {
        let text = text.trim();
        let invalid = || Error::Crypto("the authenticator key isn't valid".into());
        if let Some(secret) = strip_prefix_ci(text, "steam://") {
            return Ok(Totp {
                secret: base32(secret).ok_or_else(invalid)?,
                digits: 5,
                period: 30,
                algorithm: Algorithm::Sha1,
                steam: true,
            });
        }
        if strip_prefix_ci(text, "otpauth://").is_some() {
            let url = url::Url::parse(text).map_err(|_| invalid())?;
            let mut totp = Totp {
                secret: Zeroizing::new(Vec::new()),
                digits: 6,
                period: 30,
                algorithm: Algorithm::Sha1,
                steam: false,
            };
            for (key, value) in url.query_pairs() {
                match key.to_ascii_lowercase().as_str() {
                    "secret" => totp.secret = base32(&value).ok_or_else(invalid)?,
                    "digits" => {
                        totp.digits = value
                            .parse()
                            .ok()
                            .filter(|d| (1..=10).contains(d))
                            .ok_or_else(invalid)?
                    }
                    "period" => {
                        totp.period = value.parse().ok().filter(|p| *p > 0).ok_or_else(invalid)?
                    }
                    "algorithm" => {
                        totp.algorithm = match value.to_ascii_uppercase().as_str() {
                            "SHA1" => Algorithm::Sha1,
                            "SHA256" => Algorithm::Sha256,
                            "SHA512" => Algorithm::Sha512,
                            _ => return Err(invalid()),
                        }
                    }
                    "encoder" if value.eq_ignore_ascii_case("steam") => {
                        totp.steam = true;
                        totp.digits = 5;
                    }
                    _ => {}
                }
            }
            if totp.secret.is_empty() {
                return Err(invalid());
            }
            return Ok(totp);
        }
        Ok(Totp {
            secret: base32(text).ok_or_else(invalid)?,
            digits: 6,
            period: 30,
            algorithm: Algorithm::Sha1,
            steam: false,
        })
    }

    /// The code at `unix_seconds`, and how many seconds it stays valid.
    pub fn code_at(&self, unix_seconds: u64) -> (Zeroizing<String>, u64) {
        let counter = unix_seconds / self.period;
        let remaining = self.period - unix_seconds % self.period;
        let hash = match self.algorithm {
            Algorithm::Sha1 => hmac::<Hmac<sha1::Sha1>>(&self.secret, counter),
            Algorithm::Sha256 => hmac::<Hmac<sha2::Sha256>>(&self.secret, counter),
            Algorithm::Sha512 => hmac::<Hmac<sha2::Sha512>>(&self.secret, counter),
        };
        let offset = (hash[hash.len() - 1] & 0x0f) as usize;
        let mut value = u32::from_be_bytes([
            hash[offset] & 0x7f,
            hash[offset + 1],
            hash[offset + 2],
            hash[offset + 3],
        ]);
        let mut code = Zeroizing::new(String::with_capacity(self.digits as usize));
        if self.steam {
            for _ in 0..5 {
                code.push(STEAM_CHARS[(value as usize) % STEAM_CHARS.len()] as char);
                value /= STEAM_CHARS.len() as u32;
            }
        } else {
            let modulus = 10u64.pow(self.digits);
            let number = u64::from(value) % modulus;
            code.push_str(&format!("{number:0width$}", width = self.digits as usize));
        }
        (code, remaining)
    }

    pub fn now(&self) -> (Zeroizing<String>, u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.code_at(now)
    }
}

fn hmac<M: Mac + hmac::digest::KeyInit>(key: &[u8], counter: u64) -> Vec<u8> {
    let mut mac = <M as Mac>::new_from_slice(key).expect("any key length");
    mac.update(&counter.to_be_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// `text` without `prefix`, ignoring ASCII case. `get` rather than indexing:
/// the prefix length can fall inside a multi-byte character of whatever a
/// shared item holds, and slicing there would panic.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|start| start.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

/// RFC 4648 base32, forgiving like authenticator apps: spaces, dashes, lower
/// case and missing padding are fine.
fn base32(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::with_capacity(text.len() * 5 / 8));
    let (mut buffer, mut bits) = (0u64, 0u32);
    let mut any = false;
    for c in text.chars() {
        let value = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u64 - 'A' as u64,
            c @ '2'..='7' => c as u64 - '2' as u64 + 26,
            ' ' | '-' | '=' => continue,
            _ => return None,
        };
        any = true;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    any.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238, appendix B: the ASCII secret "12345678901234567890", 8 digits.
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn rfc_6238_sha1() {
        let totp = Totp::parse(&format!("otpauth://totp/x?secret={RFC_SECRET}&digits=8")).unwrap();
        assert_eq!(totp.code_at(59).0.as_str(), "94287082");
        assert_eq!(totp.code_at(1_111_111_109).0.as_str(), "07081804");
        assert_eq!(totp.code_at(20_000_000_000).0.as_str(), "65353130");
        assert_eq!(totp.code_at(59).1, 1);
    }

    #[test]
    fn rfc_6238_sha256() {
        // The SHA-256 secret is "12345678901234567890123456789012".
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";
        let totp = Totp::parse(&format!(
            "otpauth://totp/x?secret={secret}&digits=8&algorithm=SHA256"
        ))
        .unwrap();
        assert_eq!(totp.code_at(59).0.as_str(), "46119246");
    }

    #[test]
    fn bare_secrets_are_forgiving() {
        let a = Totp::parse("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap();
        let b = Totp::parse(RFC_SECRET).unwrap();
        assert_eq!(a.code_at(59).0, b.code_at(59).0);
        assert_eq!(a.code_at(59).0.len(), 6);
        assert!(Totp::parse("not base32!").is_err());
        assert!(Totp::parse("").is_err());
    }

    #[test]
    fn non_ascii_keys_are_refused_not_a_panic() {
        for text in [
            "0000000é",
            "otpauth:/éé",
            "steam:/é",
            "otpauth://é",
            "ééééé",
            "stéam://x",
        ] {
            assert!(Totp::parse(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn parse_never_panics_on_multi_byte_text() {
        // A multi-byte character at every offset up to and past both prefix
        // lengths, behind every start of either scheme.
        for scheme in ["", "steam://", "otpauth://", "STEAM://", "OtpAuth://"] {
            for cut in 0..=scheme.len() {
                for c in ['é', '€', '𝄞', '\u{0}', '/'] {
                    for pad in 0..12 {
                        let zeros = "0".repeat(pad);
                        let _ = Totp::parse(&format!("{}{zeros}{c}", &scheme[..cut]));
                        let _ = Totp::parse(&format!("{}{c}{zeros}{c}", &scheme[..cut]));
                    }
                }
            }
        }
    }

    #[test]
    fn steam_codes() {
        let totp = Totp::parse(&format!("steam://{RFC_SECRET}")).unwrap();
        let (code, _) = totp.code_at(59);
        assert_eq!(code.len(), 5);
        assert!(code.bytes().all(|c| STEAM_CHARS.contains(&c)));
    }
}
