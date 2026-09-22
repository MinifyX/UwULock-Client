//! Random passwords from the system's CSPRNG, with at least one character of
//! every set that is switched on, like Bitwarden's generator.

use rand::seq::SliceRandom;
use rand::Rng;
use zeroize::Zeroizing;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    /// Leaves out characters that look alike: l, 1, I, O, 0.
    pub avoid_ambiguous: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            avoid_ambiguous: false,
        }
    }
}

pub const MIN_LENGTH: usize = 5;
pub const MAX_LENGTH: usize = 128;

pub fn password(options: &Options) -> Zeroizing<String> {
    let strip = |set: &str| -> Vec<char> {
        set.chars()
            .filter(|c| !options.avoid_ambiguous || !"lIO01".contains(*c))
            .collect()
    };
    let mut sets: Vec<Vec<char>> = Vec::new();
    if options.lowercase {
        sets.push(strip("abcdefghijklmnopqrstuvwxyz"));
    }
    if options.uppercase {
        sets.push(strip("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
    }
    if options.digits {
        sets.push(strip("0123456789"));
    }
    if options.symbols {
        sets.push(strip("!@#$%^&*"));
    }
    if sets.is_empty() {
        sets.push(strip("abcdefghijklmnopqrstuvwxyz"));
    }
    let length = options.length.clamp(MIN_LENGTH.max(sets.len()), MAX_LENGTH);
    let all: Vec<char> = sets.iter().flatten().copied().collect();
    let mut rng = rand::rngs::OsRng;

    let mut chars: Zeroizing<Vec<char>> = Zeroizing::new(Vec::with_capacity(length));
    for set in &sets {
        chars.push(set[rng.gen_range(0..set.len())]);
    }
    while chars.len() < length {
        chars.push(all[rng.gen_range(0..all.len())]);
    }
    chars.shuffle(&mut rng);
    Zeroizing::new(chars.iter().collect())
}

/// A rough strength, in bits: length times the bits per character of the
/// sets the password actually uses. For the meter, not a promise.
pub fn entropy_bits(password: &str) -> u32 {
    let mut pool = 0u32;
    if password.chars().any(|c| c.is_ascii_lowercase()) {
        pool += 26;
    }
    if password.chars().any(|c| c.is_ascii_uppercase()) {
        pool += 26;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        pool += 10;
    }
    if password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        pool += 33;
    }
    if pool == 0 {
        return 0;
    }
    (password.chars().count() as f64 * f64::from(pool).log2()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_shows_up() {
        for _ in 0..200 {
            let p = password(&Options {
                length: 8,
                ..Options::default()
            });
            assert_eq!(p.chars().count(), 8);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| "!@#$%^&*".contains(c)));
        }
    }

    #[test]
    fn ambiguous_characters_stay_out() {
        let p = password(&Options {
            length: 128,
            avoid_ambiguous: true,
            ..Options::default()
        });
        assert!(!p.chars().any(|c| "lIO01".contains(c)));
    }

    #[test]
    fn length_is_clamped() {
        assert_eq!(
            password(&Options {
                length: 1,
                ..Options::default()
            })
            .len(),
            MIN_LENGTH
        );
        assert_eq!(
            password(&Options {
                length: 999,
                ..Options::default()
            })
            .len(),
            MAX_LENGTH
        );
    }
}
