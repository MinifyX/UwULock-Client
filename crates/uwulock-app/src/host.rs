//! What the app needs from the program around it: the window, the
//! clipboard, the system's biometrics, its settings and extensions. The
//! Tauri app implements it (apps/desktop/src-tauri/src/host.rs), a native
//! app would too.
//!
//! The contract:
//!
//! - Every method may be called from any thread, including the async
//!   runtime's; the ones that wait for the system or the person block, and
//!   the app calls those off the runtime's threads.
//! - [`Host::emit`] hands an event to the page as it is; the app's own
//!   listeners ([`App::listen`]) have heard it already.
//! - Errors are the person's words: the app shows them as they are.
//! - What a system doesn't have answers with the default: nothing there,
//!   or `unsupported`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use zeroize::Zeroizing;

use crate::autofill::ProviderState;
use crate::hello::Prompt;
use crate::passkeys::Settings;
use crate::safari::SafariView;
use crate::vault::Failure;
use crate::wifi::Network;
use crate::App;

pub trait Host: Send + Sync + 'static {
    // ── The page ─────────────────────────────────────────────

    /// Sends an event to the page (`vault-status`, `passkey-request`, …;
    /// README.md has the list). The payload is JSON.
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// Brings the window to the front: a passkey request waits in it.
    fn show_window(&self) {}

    /// Opens a web page in the person's browser.
    fn open_url(&self, url: &str) -> Result<(), String>;

    // ── The clipboard ────────────────────────────────────────

    /// Puts `text` on the clipboard. `sensitive`: kept out of clipboard
    /// history and managers where the system can, and (on a phone) gone by
    /// itself after `expires`. The app clears it itself too
    /// ([`Host::clipboard_clear`]).
    fn clipboard_copy(
        &self,
        text: &str,
        sensitive: bool,
        expires: Option<Duration>,
    ) -> Result<(), String>;

    /// Empties the clipboard if it still holds the text with this SHA-256.
    /// Where the clipboard can't be read (the phones), the system's own
    /// expiry does it and this does nothing.
    fn clipboard_clear(&self, sha256: &[u8; 32]);

    // ── Biometrics ───────────────────────────────────────────

    /// What unlocks here (`Windows Hello`, `Touch ID`, `Face ID`,
    /// `fingerprint`, …), `None` when nothing does. May take a moment.
    fn biometric_kind(&self) -> Option<String> {
        None
    }

    /// A 32-byte secret the device hands out only after the person, under
    /// `name` (one per account, [`crate::hello::name`]). `create`: make it
    /// first (replacing one there). Windows Hello signs `challenge` and the
    /// secret comes from the signature; the phones and the Mac keep a random
    /// secret and ignore it. Failures: `cancelled`, `unavailable`, `hello`.
    fn biometric_secret(
        &self,
        name: &str,
        challenge: &[u8],
        create: bool,
        prompt: &Prompt,
    ) -> Result<Zeroizing<[u8; 32]>, Failure> {
        let _ = (name, challenge, create, prompt);
        Err(Failure::new(
            "unavailable",
            "Nothing unlocks with biometrics here.",
        ))
    }

    /// The device forgets the secret `name`.
    fn biometric_forget(&self, name: &str) {
        let _ = name;
    }

    // ── The system ───────────────────────────────────────────

    /// The screen is locked right now (Windows, macOS; Linux tells
    /// [`App::system_locked`] itself).
    fn screen_locked(&self) -> bool {
        false
    }

    /// Where a downloaded file goes: the Downloads folder (or home) on a
    /// computer, UwULock's documents folder on iOS, a scratch folder on
    /// Android ([`Host::publish_download`] hands it on). The error is the
    /// person's words ("No Downloads folder: …").
    fn download_dir(&self) -> Result<PathBuf, String>;

    /// Android: hands a file from [`Host::download_dir`] to the system's
    /// Downloads and deletes it; the name it got there.
    fn publish_download(&self, path: &Path, name: &str) -> Result<String, String> {
        let _ = (path, name);
        Err("Nothing to hand downloads to.".into())
    }

    /// Hands a Wi-Fi network to the system (Android): `saved`,
    /// `already-saved`, `suggested`, `declined`, `disallowed` or what else
    /// it said. `None` where there is nothing to hand it to. A failure
    /// `not-ready`: the phone isn't ready; `unsupported`: too old a system.
    fn wifi_join(&self, network: &Network) -> Option<Result<String, Failure>> {
        let _ = network;
        None
    }

    /// Opens the system's Wi-Fi settings; `false` where there are none to
    /// open.
    fn wifi_settings(&self) -> Result<bool, Failure> {
        Ok(false)
    }

    // ── Passkey and AutoFill providers ───────────────────────

    /// Starts or stops the platform's passkey provider after the settings
    /// (Linux: the virtual security key, Windows: the plugin). Logs what
    /// fails.
    fn passkey_provider_apply(&self, app: &App, settings: &Settings) {
        let _ = (app, settings);
    }

    /// The platform's passkey provider: `linux`, `windows` or `android`,
    /// whether it runs, and why not. `None`: there is none (the Apple
    /// extension is the app's own, passkeys/apple.rs).
    fn passkey_provider_state(&self, app: &App) -> Option<(&'static str, bool, Option<String>)> {
        let _ = app;
        None
    }

    /// Whether UwULock is the system's AutoFill/credential provider.
    fn autofill_provider_status(&self) -> Result<ProviderState, String> {
        Ok(ProviderState::default())
    }

    /// Asks the system to make UwULock its provider (`credentials`, or on
    /// Android `autofill`), and what it says after. Where there is nothing to
    /// ask, just what [`Host::autofill_provider_status`] says.
    fn autofill_provider_request(&self, target: &str) -> Result<ProviderState, Failure> {
        let _ = target;
        Ok(self.autofill_provider_status().unwrap_or_default())
    }

    /// iOS 26: credentials another app handed over wait.
    fn credential_exchange_pending(&self) -> bool {
        false
    }

    /// iOS 26: the handed-over credentials as CXF JSON; `discard` drops
    /// them.
    fn credential_exchange_import(&self, discard: bool) -> Result<String, Failure> {
        let _ = discard;
        Err(Failure::new("unsupported", "only on iOS 26"))
    }

    /// The AutoFill extension's protocol: `None` where there is no
    /// extension, `Err` with why it can't be read.
    fn autofill_log(&self) -> Option<Result<String, String>> {
        None
    }

    /// Empties the AutoFill extension's protocol.
    fn autofill_log_clear(&self) -> Result<(), Failure> {
        Err(Failure::new(
            "unsupported",
            "no AutoFill extension on this system",
        ))
    }

    /// Whether Safari has the extension, and whether it is on (macOS).
    fn safari_extension_status(&self) -> SafariView {
        SafariView::unasked()
    }

    /// Opens Safari's settings on the extension (macOS).
    fn safari_extension_open(&self) -> Result<(), Failure> {
        Err(Failure::new("unsupported", "only on macOS"))
    }

    // ── The Apple AutoFill extension (passkeys/apple.rs) ─────

    /// Why the extension can't be handed its list (an unsigned build, an
    /// iOS too old), `None` when it can.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_problem(&self) -> Option<String> {
        Some("no AutoFill extension in this build".into())
    }

    /// A line into the extension's protocol (iOS).
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_note(&self, text: &str) {
        let _ = text;
    }

    /// Leaves the extension its sealed list, the key that opens it (in the
    /// shared Keychain group, only after the person) and the entries for
    /// the system's list.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_store(
        &self,
        list: &[u8],
        key: &[u8; 32],
        identities: &[crate::passkeys::apple::Identity],
    ) -> Result<(), String> {
        let _ = (list, key, identities);
        Err("no AutoFill extension in this build".into())
    }

    /// What the extension made: names and sealed bytes, those set aside as
    /// `unreadable/<name>` ([`crate::passkeys::apple::outbox_name`]).
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_outbox(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(Vec::new())
    }

    /// Removes these from the outbox: they are in the vault.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_clear_outbox(&self, names: &[String]) -> Result<(), String> {
        let _ = names;
        Ok(())
    }

    /// Moves these to `outbox/unreadable/`.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_set_aside(&self, names: &[String]) -> Result<(), String> {
        let _ = names;
        Ok(())
    }

    /// Removes the list, the Keychain item and the system's entries.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_clear(&self) -> Result<(), String> {
        Ok(())
    }
}

/// A host for unit tests: no page, no clipboard, nothing on the system.
#[cfg(test)]
pub(crate) struct NoHost;

#[cfg(test)]
impl Host for NoHost {
    fn emit(&self, _event: &str, _payload: serde_json::Value) {}

    fn open_url(&self, _url: &str) -> Result<(), String> {
        Err("no browser in tests".into())
    }

    fn clipboard_copy(
        &self,
        _text: &str,
        _sensitive: bool,
        _expires: Option<Duration>,
    ) -> Result<(), String> {
        Err("no clipboard in tests".into())
    }

    fn clipboard_clear(&self, _sha256: &[u8; 32]) {}

    fn download_dir(&self) -> Result<PathBuf, String> {
        Err("no Downloads folder in tests".into())
    }
}
