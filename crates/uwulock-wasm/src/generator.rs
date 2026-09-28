//! The password generator: random passwords and passphrases, each with its
//! strength for the meter.

use crate::{json, Result};
use uwulock_core::generator::{self, Options, PassphraseOptions};

/// A random password: `{"password", "bits"}`.
pub fn password(options: &str) -> Result<String> {
    let options: Options = serde_json::from_str(options)?;
    let password = generator::password(&options);
    let bits = generator::entropy_bits(&password);
    json(&serde_json::json!({ "password": password.as_str(), "bits": bits }))
}

/// A passphrase: `{"password", "bits"}`. Its strength comes from the options,
/// not from the letters: the words are what is guessed.
pub fn passphrase(options: &str) -> Result<String> {
    let options: PassphraseOptions = serde_json::from_str(options)?;
    let password = generator::passphrase(&options);
    let bits = generator::passphrase_entropy_bits(&options);
    json(&serde_json::json!({ "password": password.as_str(), "bits": bits }))
}
