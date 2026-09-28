//! Random passwords from the system's CSPRNG, with at least one character of
//! every set that is switched on, like Bitwarden's generator — and
//! passphrases, words from EFF's long list, like Bitwarden's too.

use rand::seq::SliceRandom;
use rand::Rng;
use zeroize::Zeroizing;

use crate::crypto;

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

/// How a passphrase is made. Anything left out is Bitwarden's default: six
/// words, joined with `-`, lower case, no number.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PassphraseOptions {
    /// Clamped to 3 ..= 20, as in Bitwarden.
    pub words: usize,
    /// What goes between the words; only its first few characters count.
    pub separator: String,
    /// Each word starts with a capital letter.
    pub capitalize: bool,
    /// One random digit at the end of one random word.
    pub include_number: bool,
}

impl Default for PassphraseOptions {
    fn default() -> Self {
        PassphraseOptions {
            words: 6,
            separator: "-".into(),
            capitalize: false,
            include_number: false,
        }
    }
}

pub const MIN_WORDS: usize = 3;
pub const MAX_WORDS: usize = 20;
/// The longest separator, in characters. Longer ones are cut.
pub const MAX_SEPARATOR: usize = 3;

pub fn passphrase(options: &PassphraseOptions) -> Zeroizing<String> {
    let count = options.words.clamp(MIN_WORDS, MAX_WORDS);
    let separator: String = options.separator.chars().take(MAX_SEPARATOR).collect();
    let mut rng = rand::rngs::OsRng;
    let mut words: Vec<Zeroizing<String>> = (0..count)
        .map(|_| {
            let word = crypto::word(rng.gen_range(0..crypto::WORD_COUNT as usize));
            let mut chars = word.chars();
            Zeroizing::new(match chars.next() {
                Some(first) if options.capitalize => first.to_uppercase().chain(chars).collect(),
                _ => word.to_string(),
            })
        })
        .collect();
    if options.include_number {
        let at = rng.gen_range(0..count);
        let digit = char::from(b'0' + rng.gen_range(0..10u8));
        words[at].push(digit);
    }
    let mut out = Zeroizing::new(String::new());
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            out.push_str(&separator);
        }
        out.push_str(word);
    }
    out
}

/// A passphrase's strength, in bits: each word is one of 7776, and the number
/// one of ten digits on one of the words. Capitals and the separator add
/// nothing, since whoever guesses knows the options too.
pub fn passphrase_entropy_bits(options: &PassphraseOptions) -> u32 {
    let count = options.words.clamp(MIN_WORDS, MAX_WORDS);
    let mut bits = count as f64 * f64::from(crypto::WORD_COUNT).log2();
    if options.include_number {
        bits += (10.0 * count as f64).log2();
    }
    bits as u32
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

    fn words_of(phrase: &str, separator: &str) -> Vec<String> {
        phrase.split(separator).map(str::to_string).collect()
    }

    #[test]
    fn a_passphrase_is_words_from_the_list() {
        let list: Vec<&str> = (0..crypto::WORD_COUNT as usize).map(crypto::word).collect();
        let phrase = passphrase(&PassphraseOptions::default());
        let words = words_of(&phrase, "-");
        assert_eq!(words.len(), 6);
        assert!(words.iter().all(|word| list.contains(&word.as_str())));
    }

    #[test]
    fn capitals_a_number_and_another_separator() {
        let options = PassphraseOptions {
            words: 4,
            separator: " ".into(),
            capitalize: true,
            include_number: true,
        };
        for _ in 0..50 {
            let phrase = passphrase(&options);
            let words = words_of(&phrase, " ");
            assert_eq!(words.len(), 4);
            assert!(words
                .iter()
                .all(|w| w.starts_with(|c: char| c.is_uppercase())));
            let with_digit: Vec<_> = words
                .iter()
                .filter(|w| w.ends_with(|c: char| c.is_ascii_digit()))
                .collect();
            assert_eq!(with_digit.len(), 1, "{}", phrase.as_str());
            assert_eq!(phrase.chars().filter(char::is_ascii_digit).count(), 1);
        }
    }

    #[test]
    fn passphrase_options_are_clamped() {
        let count = |words| {
            let options = PassphraseOptions {
                words,
                ..PassphraseOptions::default()
            };
            words_of(&passphrase(&options), "-").len()
        };
        assert_eq!(count(0), MIN_WORDS);
        assert_eq!(count(99), MAX_WORDS);

        let long = PassphraseOptions {
            separator: "#$%&*".into(),
            ..PassphraseOptions::default()
        };
        let phrase = passphrase(&long);
        assert_eq!(words_of(&phrase, "#$%").len(), 6);
        assert!(!phrase.contains('&'));

        // An empty separator runs the words together.
        let none = PassphraseOptions {
            words: 3,
            separator: String::new(),
            ..PassphraseOptions::default()
        };
        assert!(!passphrase(&none).contains('-'));
    }

    #[test]
    fn passphrase_options_from_json() {
        let options: PassphraseOptions =
            serde_json::from_str(r#"{"words": 5, "includeNumber": true}"#).unwrap();
        assert_eq!(options.words, 5);
        assert_eq!(options.separator, "-");
        assert!(options.include_number && !options.capitalize);
    }

    #[test]
    fn passphrase_strength() {
        let bits = |words, include_number| {
            passphrase_entropy_bits(&PassphraseOptions {
                words,
                include_number,
                ..PassphraseOptions::default()
            })
        };
        // log2(7776) is 12.92: three words 38.77, six 77.55.
        assert_eq!(bits(3, false), 38);
        assert_eq!(bits(6, false), 77);
        // Plus log2(10 · 6) = 5.9 for the digit and its word.
        assert_eq!(bits(6, true), 83);
        assert_eq!(bits(1, false), 38);
    }
}
