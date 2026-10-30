//! The host in Swift: [`HostDelegate`] is what `AppleHost` implements, [`Adapter`] the
//! [`Host`] the app gets from it. Values cross as plain types; what the page sees as JSON
//! (an event's payload, a Wi-Fi network, the AutoFill identities) crosses as JSON.
//!
//! The app calls its host on its own threads, never on Swift's main thread, and waits for the
//! answer: a delegate method may block (a Face ID sheet, a system dialog).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;
use uwulock_app::autofill::ProviderState as AppProviderState;
use uwulock_app::hello::Prompt;
use uwulock_app::safari::SafariView;
use uwulock_app::wifi::Network;
use uwulock_app::{Failure, Host};
use zeroize::Zeroizing;

/// What a delegate method answers instead: a kind for the app to branch on (the page's
/// failure kinds, such as `biometric-cancelled`), a message to show.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HostError {
    #[error("{message}")]
    Failed { kind: String, message: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HostError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        HostError::Failed {
            kind: "failed".into(),
            message: error.reason,
        }
    }
}

impl HostError {
    fn message(self) -> String {
        let HostError::Failed { message, .. } = self;
        message
    }

    fn failure(self) -> Failure {
        let HostError::Failed { kind, message } = self;
        Failure::new(intern(&kind), message)
    }
}

/// A failure's kind lives as long as the app ([`Failure::new`]): the few kinds the host says,
/// each kept once.
fn intern(kind: &str) -> &'static str {
    static KINDS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut kinds = KINDS.get_or_init(Default::default).lock();
    if let Some(known) = kinds.get(kind) {
        return known;
    }
    let kept: &'static str = Box::leak(kind.to_owned().into_boxed_str());
    kinds.insert(kept);
    kept
}

/// What the system says about UwULock as its passwords and passkeys provider.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct ProviderState {
    /// This system can have UwULock as its provider.
    pub supported: bool,
    /// Switched on in the system; `None` when it doesn't say.
    pub enabled: Option<bool>,
    /// The system asks the person itself; otherwise the settings open.
    pub direct: bool,
}

impl From<ProviderState> for AppProviderState {
    fn from(state: ProviderState) -> Self {
        AppProviderState {
            supported: state.supported,
            enabled: state.enabled,
            direct: state.direct,
            autofill: None,
        }
    }
}

/// Safari's word on UwULock's extension (macOS).
#[derive(Debug, Clone, uniffi::Record)]
pub struct SafariState {
    /// `macos`, `ios` or `none`.
    pub platform: String,
    /// This build carries the extension.
    pub available: bool,
    /// Switched on in Safari; `None` when Safari doesn't say.
    pub enabled: Option<bool>,
}

/// A file the AutoFill extension left in the outbox.
#[derive(Debug, Clone, uniffi::Record)]
pub struct OutboxFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The system around the app: `AppleHost` in Swift. The defaults of
/// [`Host`](uwulock_app::Host) are a delegate's answers too (`None`, `false`, an empty list).
#[uniffi::export(with_foreign)]
pub trait HostDelegate: Send + Sync {
    /// An event for the app (`vault-status`, `auth-request`, …) with its JSON payload.
    fn emit(&self, event: String, payload_json: String);

    /// Brings the window to the front (a passkey request from a browser).
    fn show_window(&self);

    fn open_url(&self, url: String) -> Result<(), HostError>;

    /// Copies `text`: `sensitive` stays on this device, is hidden from clipboard managers and
    /// goes after `expires_seconds`.
    fn clipboard_copy(
        &self,
        text: String,
        sensitive: bool,
        expires_seconds: Option<u64>,
    ) -> Result<(), HostError>;

    /// Clears the clipboard if it still holds the text with this SHA-256 (or, on iOS, nothing
    /// was copied since).
    fn clipboard_clear(&self, sha256: Vec<u8>);

    /// `faceId`, `touchId`, `opticId` or `biometric`; `None` without one.
    fn biometric_kind(&self) -> Option<String>;

    /// The 32 bytes kept for unlocking account `name`: new ones with `create`. Failure kinds:
    /// `biometric-cancelled`, `biometric-lockout`, `biometric-changed`, `hello`, `unsupported`.
    fn biometric_secret(
        &self,
        name: String,
        create: bool,
        title: String,
        subtitle: String,
        cancel: String,
        reason: String,
    ) -> Result<Vec<u8>, HostError>;

    fn biometric_forget(&self, name: String);

    /// The Mac's screen is locked.
    fn screen_locked(&self) -> bool;

    /// Where downloads go: Documents on iOS, Downloads on a Mac.
    fn download_dir(&self) -> Result<String, HostError>;

    /// Hands a network (JSON: wifi.rs `Network`) to the system: `saved`, `already-saved`,
    /// `declined`; `None` where the system can't.
    fn wifi_join(&self, network_json: String) -> Result<Option<String>, HostError>;

    /// Opens the system's Wi-Fi settings: `false` where it can't.
    fn wifi_settings(&self) -> Result<bool, HostError>;

    fn autofill_provider_status(&self) -> Result<ProviderState, HostError>;

    /// `target`: `credentials`.
    fn autofill_provider_request(&self, target: String) -> Result<ProviderState, HostError>;

    /// Another app handed over credentials (iOS 26).
    fn credential_exchange_pending(&self) -> bool;

    /// The handed-over credentials as JSON; `discard` drops them ("").
    fn credential_exchange_import(&self, discard: bool) -> Result<String, HostError>;

    /// The AutoFill extension's log; `None` without one.
    fn autofill_log(&self) -> Result<Option<String>, HostError>;

    fn autofill_log_clear(&self) -> Result<(), HostError>;

    fn safari_extension_status(&self) -> SafariState;

    fn safari_extension_open(&self) -> Result<(), HostError>;

    /// Why the AutoFill extension can't get its list; `None` when it can.
    fn extension_problem(&self) -> Option<String>;

    /// A line for the AutoFill log.
    fn extension_note(&self, text: String);

    /// The sealed list for the extension, its key (the Keychain item) and the identities for
    /// the QuickType bar (JSON: passkeys/apple.rs `Identity`).
    fn extension_store(
        &self,
        list: Vec<u8>,
        key: Vec<u8>,
        identities_json: String,
    ) -> Result<(), HostError>;

    fn extension_outbox(&self) -> Result<Vec<OutboxFile>, HostError>;

    fn extension_clear_outbox(&self, names: Vec<String>) -> Result<(), HostError>;

    fn extension_set_aside(&self, names: Vec<String>) -> Result<(), HostError>;

    fn extension_clear(&self) -> Result<(), HostError>;
}

/// The app's [`Host`] from a Swift [`HostDelegate`].
pub(crate) struct Adapter {
    delegate: Arc<dyn HostDelegate>,
}

impl Adapter {
    pub(crate) fn new(delegate: Arc<dyn HostDelegate>) -> Self {
        Adapter { delegate }
    }
}

/// The page's value for a `&'static str` the host says.
fn platform(name: &str) -> &'static str {
    match name {
        "macos" => "macos",
        "ios" => "ios",
        _ => "none",
    }
}

impl Host for Adapter {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.delegate.emit(event.to_string(), payload.to_string());
    }

    fn show_window(&self) {
        self.delegate.show_window();
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        self.delegate
            .open_url(url.to_string())
            .map_err(HostError::message)
    }

    fn clipboard_copy(
        &self,
        text: &str,
        sensitive: bool,
        expires: Option<Duration>,
    ) -> Result<(), String> {
        self.delegate
            .clipboard_copy(
                text.to_string(),
                sensitive,
                expires.map(|expires| expires.as_secs()),
            )
            .map_err(HostError::message)
    }

    fn clipboard_clear(&self, sha256: &[u8; 32]) {
        self.delegate.clipboard_clear(sha256.to_vec());
    }

    fn biometric_kind(&self) -> Option<String> {
        self.delegate.biometric_kind()
    }

    fn biometric_secret(
        &self,
        name: &str,
        challenge: &[u8],
        create: bool,
        prompt: &Prompt,
    ) -> Result<Zeroizing<[u8; 32]>, Failure> {
        // The Keychain keeps random bytes; the challenge is Windows Hello's.
        let _ = challenge;
        let bytes = Zeroizing::new(
            self.delegate
                .biometric_secret(
                    name.to_string(),
                    create,
                    prompt.title.clone(),
                    prompt.subtitle.clone(),
                    prompt.cancel.clone(),
                    prompt.reason.clone(),
                )
                .map_err(HostError::failure)?,
        );
        let mut secret = Zeroizing::new([0u8; 32]);
        if bytes.len() != secret.len() {
            return Err(Failure::new(
                "hello",
                "The Keychain gave back something else.",
            ));
        }
        secret.copy_from_slice(&bytes);
        Ok(secret)
    }

    fn biometric_forget(&self, name: &str) {
        self.delegate.biometric_forget(name.to_string());
    }

    fn screen_locked(&self) -> bool {
        self.delegate.screen_locked()
    }

    fn download_dir(&self) -> Result<PathBuf, String> {
        self.delegate
            .download_dir()
            .map(PathBuf::from)
            .map_err(HostError::message)
    }

    fn wifi_join(&self, network: &Network) -> Option<Result<String, Failure>> {
        // The password goes with it, as to Android's plugin.
        let json = serde_json::to_string(network).ok()?;
        match self.delegate.wifi_join(json) {
            Ok(Some(outcome)) => Some(Ok(outcome)),
            Ok(None) => None,
            Err(error) => Some(Err(error.failure())),
        }
    }

    fn wifi_settings(&self) -> Result<bool, Failure> {
        self.delegate.wifi_settings().map_err(HostError::failure)
    }

    fn autofill_provider_status(&self) -> Result<AppProviderState, String> {
        self.delegate
            .autofill_provider_status()
            .map(Into::into)
            .map_err(HostError::message)
    }

    fn autofill_provider_request(&self, target: &str) -> Result<AppProviderState, Failure> {
        self.delegate
            .autofill_provider_request(target.to_string())
            .map(Into::into)
            .map_err(HostError::failure)
    }

    fn credential_exchange_pending(&self) -> bool {
        self.delegate.credential_exchange_pending()
    }

    fn credential_exchange_import(&self, discard: bool) -> Result<String, Failure> {
        self.delegate
            .credential_exchange_import(discard)
            .map_err(HostError::failure)
    }

    fn autofill_log(&self) -> Option<Result<String, String>> {
        match self.delegate.autofill_log() {
            Ok(Some(text)) => Some(Ok(text)),
            Ok(None) => None,
            Err(error) => Some(Err(error.message())),
        }
    }

    fn autofill_log_clear(&self) -> Result<(), Failure> {
        self.delegate
            .autofill_log_clear()
            .map_err(HostError::failure)
    }

    fn safari_extension_status(&self) -> SafariView {
        let state = self.delegate.safari_extension_status();
        SafariView {
            platform: platform(&state.platform),
            available: state.available,
            enabled: state.enabled,
        }
    }

    fn safari_extension_open(&self) -> Result<(), Failure> {
        self.delegate
            .safari_extension_open()
            .map_err(HostError::failure)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_problem(&self) -> Option<String> {
        self.delegate.extension_problem()
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_note(&self, text: &str) {
        self.delegate.extension_note(text.to_string());
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_store(
        &self,
        list: &[u8],
        key: &[u8; 32],
        identities: &[uwulock_app::passkeys::apple::Identity],
    ) -> Result<(), String> {
        let identities = serde_json::to_string(identities).map_err(|error| error.to_string())?;
        // UniFFI's buffers aren't wiped; the key lives on in the Keychain anyway.
        self.delegate
            .extension_store(list.to_vec(), key.to_vec(), identities)
            .map_err(HostError::message)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_outbox(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        self.delegate
            .extension_outbox()
            .map(|files| {
                files
                    .into_iter()
                    .map(|file| (file.name, file.bytes))
                    .collect()
            })
            .map_err(HostError::message)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_clear_outbox(&self, names: &[String]) -> Result<(), String> {
        self.delegate
            .extension_clear_outbox(ours(names))
            .map_err(HostError::message)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_set_aside(&self, names: &[String]) -> Result<(), String> {
        self.delegate
            .extension_set_aside(ours(names))
            .map_err(HostError::message)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_clear(&self) -> Result<(), String> {
        self.delegate.extension_clear().map_err(HostError::message)
    }
}

/// Only outbox names of ours go to Swift, which checks them again.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn ours(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| uwulock_app::passkeys::apple::outbox_name(name).is_some())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_kept_once() {
        let first = intern("biometric-cancelled");
        let again = intern(&String::from("biometric-cancelled"));
        assert!(std::ptr::eq(first, again));
    }
}
