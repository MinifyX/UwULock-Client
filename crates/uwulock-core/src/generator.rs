//! Random passwords from the system's CSPRNG, with at least one character of
//! every set that is switched on (or as many as its minimum asks for), like
//! Bitwarden's generator — and passphrases, words from EFF's long list, like
//! Bitwarden's too.

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
    /// At least this many of each set, when the set is on; 0 or 1 both mean
    /// "at least one". Bitwarden keeps `minNumber` and `minSpecial`; the
    /// other two are UwULock's. Left out, they are 0.
    #[serde(default)]
    pub min_lowercase: usize,
    #[serde(default)]
    pub min_uppercase: usize,
    #[serde(default, alias = "minDigits")]
    pub min_number: usize,
    #[serde(default, alias = "minSymbols")]
    pub min_special: usize,
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
            min_lowercase: 0,
            min_uppercase: 0,
            min_number: 0,
            min_special: 0,
        }
    }
}

pub const MIN_LENGTH: usize = 5;
pub const MAX_LENGTH: usize = 128;

impl Options {
    /// The sets that are on, each with how many of it the password needs
    /// (at least one). Lower case alone when none is on.
    fn sets(&self) -> Vec<(&'static str, usize)> {
        let mut sets = Vec::new();
        for (on, set, min) in [
            (
                self.lowercase,
                "abcdefghijklmnopqrstuvwxyz",
                self.min_lowercase,
            ),
            (
                self.uppercase,
                "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                self.min_uppercase,
            ),
            (self.digits, "0123456789", self.min_number),
            (self.symbols, "!@#$%^&*", self.min_special),
        ] {
            if on {
                sets.push((set, min.clamp(1, MAX_LENGTH)));
            }
        }
        if sets.is_empty() {
            sets.push((
                "abcdefghijklmnopqrstuvwxyz",
                self.min_lowercase.clamp(1, MAX_LENGTH),
            ));
        }
        sets
    }

    /// How many characters the minimums of the sets that are on add up to
    /// (each set at least one).
    pub fn required(&self) -> usize {
        self.sets().iter().map(|(_, min)| min).sum()
    }

    /// The length [`password`] makes: the one asked for, raised to what the
    /// minimums need, within [`MIN_LENGTH`] ..= [`MAX_LENGTH`]. When it is
    /// longer than `length`, the editor says so ("raised to 24").
    pub fn effective_length(&self) -> usize {
        self.length
            .max(self.required())
            .clamp(MIN_LENGTH, MAX_LENGTH)
    }

    /// Refuses minimums that can't fit into [`MAX_LENGTH`] characters. Any
    /// other combination works: a length below the minimums is raised.
    pub fn check(&self) -> Result<(), crate::Error> {
        if self.required() > MAX_LENGTH {
            return Err(crate::Error::Crypto(format!(
                "the minimums add up to more than {MAX_LENGTH} characters"
            )));
        }
        Ok(())
    }
}

/// A random password. Never fails: a length below the minimums is raised
/// ([`Options::effective_length`]); minimums beyond [`MAX_LENGTH`] (which
/// [`Options::check`] refuses) are cut, the first sets first served.
pub fn password(options: &Options) -> Zeroizing<String> {
    let strip = |set: &str| -> Vec<char> {
        set.chars()
            .filter(|c| !options.avoid_ambiguous || !"lIO01".contains(*c))
            .collect()
    };
    let sets: Vec<(Vec<char>, usize)> = options
        .sets()
        .into_iter()
        .map(|(set, min)| (strip(set), min))
        .collect();
    let length = options.effective_length();
    let all: Vec<char> = sets.iter().flat_map(|(set, _)| set).copied().collect();
    let mut rng = rand::rngs::OsRng;

    let mut chars: Zeroizing<Vec<char>> = Zeroizing::new(Vec::with_capacity(length));
    'sets: for (set, min) in &sets {
        for _ in 0..*min {
            if chars.len() == length {
                break 'sets;
            }
            chars.push(set[rng.gen_range(0..set.len())]);
        }
    }
    while chars.len() < length {
        chars.push(all[rng.gen_range(0..all.len())]);
    }
    chars.shuffle(&mut rng);
    Zeroizing::new(chars.iter().collect())
}

/// The strength of what [`password`] makes with these options, in bits —
/// from how it is made, not from one result: each forced character
/// (a set's minimum) adds log2 of its set's size, each free one log2 of all
/// sets together, plus log2 of the ways the forced and free characters can be
/// arranged — with the sets as they are after `avoid_ambiguous`. Never more
/// than length × log2(all). For the meter, not a promise.
pub fn password_entropy_bits(options: &Options) -> u32 {
    let size = |set: &str| -> f64 {
        set.chars()
            .filter(|c| !options.avoid_ambiguous || !"lIO01".contains(*c))
            .count() as f64
    };
    let length = options.effective_length();
    let mut left = length;
    let mut bits = 0f64;
    let mut forced = Vec::new();
    let mut all = 0f64;
    for (set, min) in options.sets() {
        let take = min.min(left);
        left -= take;
        bits += take as f64 * size(set).log2();
        forced.push(take);
        all += size(set);
    }
    bits += left as f64 * all.log2();
    // log2(length! / (m1! · … · mk! · free!))
    let ln_fact = |n: usize| (1..=n).map(|i| (i as f64).ln()).sum::<f64>();
    let arrangements =
        (ln_fact(length) - forced.iter().map(|m| ln_fact(*m)).sum::<f64>() - ln_fact(left))
            / std::f64::consts::LN_2;
    let total = bits + arrangements;
    total.min(length as f64 * all.log2()).max(0.0) as u32
}

/// A rough strength of a typed password, in bits: length times the bits per
/// character of the sets it actually uses. For the meter, not a promise; a
/// generated one has [`password_entropy_bits`].
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

    fn count(p: &str, set: &str) -> usize {
        p.chars().filter(|c| set.contains(*c)).count()
    }

    #[test]
    fn minimums_are_met() {
        let options = Options {
            length: 16,
            min_lowercase: 3,
            min_uppercase: 4,
            min_number: 5,
            min_special: 2,
            ..Options::default()
        };
        assert_eq!(options.required(), 14);
        assert_eq!(options.effective_length(), 16);
        options.check().unwrap();
        for _ in 0..200 {
            let p = password(&options);
            assert_eq!(p.chars().count(), 16);
            assert!(count(&p, "abcdefghijklmnopqrstuvwxyz") >= 3);
            assert!(count(&p, "ABCDEFGHIJKLMNOPQRSTUVWXYZ") >= 4);
            assert!(count(&p, "0123456789") >= 5);
            assert!(count(&p, "!@#$%^&*") >= 2);
        }
    }

    #[test]
    fn a_short_length_is_raised_to_the_minimums() {
        let options = Options {
            length: 8,
            min_number: 9,
            min_special: 6,
            ..Options::default()
        };
        // 9 + 6 and one each of the letters.
        assert_eq!(options.required(), 17);
        assert_eq!(options.effective_length(), 17);
        let p = password(&options);
        assert_eq!(p.chars().count(), 17);
        assert!(count(&p, "0123456789") >= 9);
        assert!(count(&p, "!@#$%^&*") >= 6);
    }

    #[test]
    fn minimums_of_sets_that_are_off_count_for_nothing() {
        let options = Options {
            length: 10,
            digits: false,
            symbols: false,
            min_number: 50,
            min_special: 50,
            ..Options::default()
        };
        assert_eq!(options.required(), 2);
        let p = password(&options);
        assert_eq!(count(&p, "0123456789!@#$%^&*"), 0);
    }

    #[test]
    fn minimums_beyond_the_longest_password_are_refused_but_never_panic() {
        let options = Options {
            length: 20,
            min_lowercase: 100,
            min_number: 100,
            ..Options::default()
        };
        assert!(options.check().is_err());
        assert_eq!(options.effective_length(), MAX_LENGTH);
        let p = password(&options);
        assert_eq!(p.chars().count(), MAX_LENGTH);
        // With avoid_ambiguous the digits are 2–9 only; still no panic.
        let p = password(&Options {
            avoid_ambiguous: true,
            ..options
        });
        assert_eq!(p.chars().count(), MAX_LENGTH);
    }

    #[test]
    fn options_from_json_with_bitwarden_names() {
        let options: Options = serde_json::from_str(
            r#"{"length": 12, "lowercase": true, "uppercase": true, "digits": true,
                "symbols": true, "avoidAmbiguous": false, "minNumber": 3, "minSpecial": 2,
                "minLowercase": 1, "minUppercase": 4}"#,
        )
        .unwrap();
        assert_eq!(
            (
                options.min_number,
                options.min_special,
                options.min_lowercase,
                options.min_uppercase
            ),
            (3, 2, 1, 4)
        );
        // Older settings without minimums still read.
        let old: Options = serde_json::from_str(
            r#"{"length": 12, "lowercase": true, "uppercase": true, "digits": true,
                "symbols": true, "avoidAmbiguous": false}"#,
        )
        .unwrap();
        assert_eq!(old.required(), 4);
    }

    fn words_of(phrase: &str, separator: &str) -> Vec<String> {
        phrase.split(separator).map(str::to_string).collect()
    }

    #[test]
    fn a_passphrase_is_words_from_the_list() {
        assert_eq!(PassphraseOptions::default().separator, "-");
        let list: Vec<&str> = (0..crypto::WORD_COUNT as usize).map(crypto::word).collect();
        // Split on a space: four words of the list have a `-` of their own (t-shirt, yo-yo, …).
        let phrase = passphrase(&PassphraseOptions {
            separator: " ".into(),
            ..PassphraseOptions::default()
        });
        let words = words_of(&phrase, " ");
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
                separator: " ".into(),
                ..PassphraseOptions::default()
            };
            words_of(&passphrase(&options), " ").len()
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

    #[test]
    fn the_meter_counts_what_the_minimums_force() {
        let mut options = Options {
            length: 8,
            min_number: 6,
            ..Options::default()
        };
        // 9 characters, all forced: 26·26·10^6·8 and 9!/6! arrangements ≈ 41 bits.
        assert_eq!(options.effective_length(), 9);
        assert_eq!(password_entropy_bits(&options), 41);
        // No minimums: close to length × log2(70), never above it.
        options = Options::default();
        let full = (20.0 * 70f64.log2()) as u32;
        let bits = password_entropy_bits(&options);
        assert!(bits <= full && bits + 3 >= full, "{bits} vs {full}");
        // Fewer characters without the look-alikes.
        options.avoid_ambiguous = true;
        assert!(password_entropy_bits(&options) < bits);
        // Digits only.
        let digits = Options {
            length: 10,
            lowercase: false,
            uppercase: false,
            symbols: false,
            ..Options::default()
        };
        assert_eq!(password_entropy_bits(&digits), 33);
    }
}
