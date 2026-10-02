//! The password generator: random passwords and passphrases, each with its
//! strength for the meter.

use crate::{json, Failure, Result};
use uwulock_core::generator::{self, Options, PassphraseOptions};

/// A random password: `{"password", "bits", "length", "required"}`.
/// `length` is the one it has: more than asked for when the minimums
/// (`minLowercase`, `minUppercase`, `minNumber`, `minSpecial`) need more;
/// `required` is what they add up to. Minimums beyond 128 characters are
/// refused as `invalid`.
pub fn password(options: &str) -> Result<String> {
    let options: Options = serde_json::from_str(options)?;
    options
        .check()
        .map_err(|e| Failure::new("invalid", e.to_string()))?;
    let password = generator::password(&options);
    let bits = generator::password_entropy_bits(&options);
    json(&serde_json::json!({
        "password": password.as_str(),
        "bits": bits,
        "length": options.effective_length(),
        "required": options.required(),
    }))
}

/// A passphrase: `{"password", "bits"}`. Its strength comes from the options,
/// not from the letters: the words are what is guessed.
pub fn passphrase(options: &str) -> Result<String> {
    let options: PassphraseOptions = serde_json::from_str(options)?;
    let password = generator::passphrase(&options);
    let bits = generator::passphrase_entropy_bits(&options);
    json(&serde_json::json!({ "password": password.as_str(), "bits": bits }))
}
