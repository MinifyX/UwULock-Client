//! UwULock's own code on Android, called from Rust only — the page gets no
//! permission for it. iOS has the native app (apps/apple) instead.
//!
//! - **Unlocking with fingerprint or face** (`unlock_*`). The phone makes 32
//!   random bytes and keeps them where only a biometric check gets them out
//!   again: encrypted under an AES key in the Android Keystore
//!   (hardware-backed where the phone has it) that only works right after
//!   BiometricPrompt confirmed a strong biometric, and that dies when a new
//!   finger or face is enrolled. The app stretches those bytes into the key that seals a copy of the user
//!   key (`hello.rs` in the app), exactly as with Windows Hello's signature.
//!   The bytes never reach the page.
//! - **Copying a secret**. Android marks the clip as sensitive (no preview,
//!   no clipboard history, Android 13+) and clears it later only when it is
//!   still UwULock's.
//!
//! - **Joining a Wi-Fi network** (`connect_wifi`): Android 11+ shows its own
//!   sheet to add the network (`ACTION_WIFI_ADD_NETWORKS`), Android 10 gets it
//!   as a network suggestion (docs/mobile.md).
//!
//! More phone features that need Android APIs belong here too: a method on
//! [`Mobile`] and a `@Command` in
//! `android/src/main/java/UwuLockMobilePlugin.kt` (docs/mobile.md).
//!
//! On the desktop the plugin does nothing; the app only uses it on phones.

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

/// Whether this phone can unlock UwULock with a biometric.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockStatus {
    pub available: bool,
    /// `fingerprint`, `face`, `iris` or `biometric` (when the phone has more
    /// than one or doesn't say).
    #[serde(default)]
    pub kind: Option<String>,
    /// Why not, when not: `none-enrolled`, `no-hardware`, `no-passcode`, …
    #[serde(default)]
    pub reason: Option<String>,
}

/// The words of the system's biometric dialog, in the app's language.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub title: String,
    pub subtitle: String,
    /// Android's button to give up and type the master password instead.
    pub cancel: String,
}

/// What the phone said no with. `code` is one of `cancelled`, `lockout`,
/// `invalidated` (a new finger or face was enrolled, or the key is gone),
/// `missing` (nothing kept for this name), `unavailable`, `failed`.
#[derive(Debug, Clone)]
pub struct Error {
    pub code: Option<String>,
    pub message: String,
}

impl Error {
    pub fn is(&self, code: &str) -> bool {
        self.code.as_deref() == Some(code)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// Whether UwULock is the system's AutoFill provider for passwords and
/// passkeys (the Credential Manager provider and the autofill service).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderState {
    /// This phone can have UwULock as its provider at all (Android 14+ for
    /// Credential Manager, 8+ for autofill).
    pub supported: bool,
    /// UwULock is switched on; `None` when the system doesn't say.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// The system can ask the person directly; otherwise the button opens
    /// the settings.
    #[serde(default)]
    pub direct: bool,
    /// UwULock is the autofill service too (apps and browsers without
    /// Credential Manager). `None` when the system doesn't say.
    #[serde(default)]
    pub autofill: Option<bool>,
}

#[cfg(mobile)]
mod mobile {
    use super::{Error, Prompt, ProviderState, UnlockStatus};
    use serde::{Deserialize, Serialize};
    use tauri::plugin::mobile::PluginInvokeError;
    use tauri::plugin::PluginHandle;
    use tauri::Runtime;

    /// The plugin, as the app holds it (`app.state::<Mobile<_>>()`). Every
    /// call blocks until the phone answers — biometric ones until the person
    /// did — so they run on a blocking thread, never the main one.
    pub struct Mobile<R: Runtime>(pub(crate) PluginHandle<R>);

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Named<'a> {
        name: &'a str,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Prompted<'a> {
        name: &'a str,
        #[serde(flatten)]
        prompt: &'a Prompt,
    }

    #[derive(Deserialize)]
    struct Secret {
        secret: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Copy<'a> {
        text: &'a str,
        /// Android ignores it: Rust clears the copy (host.rs in the app).
        expires_in_seconds: Option<u64>,
    }

    fn error(error: PluginInvokeError) -> Error {
        match error {
            PluginInvokeError::InvokeRejected(response) => Error {
                code: response.code,
                message: response
                    .message
                    .unwrap_or_else(|| "The phone said no.".into()),
            },
            other => Error {
                code: None,
                message: other.to_string(),
            },
        }
    }

    fn secret(answer: Secret) -> Result<zeroize::Zeroizing<[u8; 32]>, Error> {
        use base64::Engine as _;
        let bytes = zeroize::Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(answer.secret.as_bytes())
                .map_err(|e| Error {
                    code: Some("failed".into()),
                    message: e.to_string(),
                })?,
        );
        let mut out = zeroize::Zeroizing::new([0u8; 32]);
        if bytes.len() != out.len() {
            return Err(Error {
                code: Some("failed".into()),
                message: "The phone handed back a secret of the wrong size.".into(),
            });
        }
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    impl<R: Runtime> Mobile<R> {
        pub fn unlock_status(&self) -> Result<UnlockStatus, Error> {
            self.0.run_mobile_plugin("unlockStatus", ()).map_err(error)
        }

        /// New random bytes for `name`, replacing what was kept before. The
        /// person confirms with a biometric.
        pub fn unlock_create(
            &self,
            name: &str,
            prompt: &Prompt,
        ) -> Result<zeroize::Zeroizing<[u8; 32]>, Error> {
            self.0
                .run_mobile_plugin::<Secret>("unlockCreate", Prompted { name, prompt })
                .map_err(error)
                .and_then(secret)
        }

        /// The bytes kept for `name`, after the person showed a finger or face.
        pub fn unlock_open(
            &self,
            name: &str,
            prompt: &Prompt,
        ) -> Result<zeroize::Zeroizing<[u8; 32]>, Error> {
            self.0
                .run_mobile_plugin::<Secret>("unlockOpen", Prompted { name, prompt })
                .map_err(error)
                .and_then(secret)
        }

        pub fn unlock_delete(&self, name: &str) -> Result<(), Error> {
            self.0
                .run_mobile_plugin("unlockDelete", Named { name })
                .map_err(error)
        }

        pub fn copy(&self, text: &str, expires_in_seconds: Option<u64>) -> Result<(), Error> {
            self.0
                .run_mobile_plugin(
                    "copySecret",
                    Copy {
                        text,
                        expires_in_seconds,
                    },
                )
                .map_err(error)
        }

        /// An ordinary copy of text that isn't secret: not kept on the
        /// device, never expiring, not cleared on locking.
        pub fn copy_text(&self, text: &str) -> Result<(), Error> {
            self.0
                .run_mobile_plugin(
                    "copyText",
                    Copy {
                        text,
                        expires_in_seconds: None,
                    },
                )
                .map_err(error)
        }

        /// The bars around the page follow the page's theme.
        pub fn set_appearance(&self, dark: bool, background: &str) -> Result<(), Error> {
            #[derive(Serialize)]
            struct Appearance<'a> {
                dark: bool,
                background: &'a str,
            }
            self.0
                .run_mobile_plugin("setAppearance", Appearance { dark, background })
                .map_err(error)
        }

        /// Moves a file from the app's cache into Downloads and
        /// answers with the name it got there.
        pub fn save_to_downloads(&self, path: &str, name: &str) -> Result<String, Error> {
            #[derive(Serialize)]
            struct Save<'a> {
                path: &'a str,
                name: &'a str,
            }
            #[derive(Deserialize)]
            struct Saved {
                name: String,
            }
            self.0
                .run_mobile_plugin::<Saved>("saveToDownloads", Save { path, name })
                .map(|saved| saved.name)
                .map_err(error)
        }

        /// Adds a Wi-Fi network the person confirms in the system's
        /// sheet (Android 11+) or suggests it (Android 10). `network` is the
        /// app's request (`ssid`, `security`, `password`, `hidden`, and for
        /// Enterprise `eap`, `phase2`, `identity`, `anonymousIdentity`,
        /// `domain`). Answers `saved`, `already-saved`, `suggested`,
        /// `declined` or `disallowed` (Android 10, suggestions turned off);
        /// fails with `unsupported` (not on this Android), `invalid` (Android
        /// refused the values) or `failed`.
        pub fn connect_wifi<T: Serialize>(&self, network: &T) -> Result<String, Error> {
            #[derive(Deserialize)]
            struct Added {
                outcome: String,
            }
            self.0
                .run_mobile_plugin::<Added>("connectWifi", network)
                .map(|added| added.outcome)
                .map_err(error)
        }

        /// Opens the system's Wi-Fi settings — where a network
        /// `connect_wifi` can't take is added by hand.
        pub fn open_wifi_settings(&self) -> Result<(), Error> {
            self.0
                .run_mobile_plugin("openWifiSettings", ())
                .map_err(error)
        }

        /// Whether UwULock is the AutoFill provider ([`ProviderState`]).
        pub fn provider_status(&self) -> Result<ProviderState, Error> {
            self.0
                .run_mobile_plugin("providerStatus", ())
                .map_err(error)
        }

        /// Asks the system to make UwULock the provider: Android opens its
        /// sheet for `target` (`credentials`: Credential Manager,
        /// `autofill`: the autofill service). Answers the state afterwards,
        /// as far as the system tells.
        pub fn provider_request(&self, target: &str) -> Result<ProviderState, Error> {
            #[derive(Serialize)]
            struct Target<'a> {
                target: &'a str,
            }
            self.0
                .run_mobile_plugin("providerRequest", Target { target })
                .map_err(error)
        }

        /// Empties the clipboard if it still holds UwULock's last copy.
        pub fn clear_clipboard(&self) -> Result<(), Error> {
            self.0
                .run_mobile_plugin("clearClipboard", ())
                .map_err(error)
        }
    }
}

#[cfg(mobile)]
pub use mobile::Mobile;

/// The plugin. On a phone it registers the native half and puts a
/// [`Mobile`] in the app's state; on the desktop it is empty.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("uwulock-mobile")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let handle =
                api.register_android_plugin("app.uwulock.mobile", "UwuLockMobilePlugin")?;
            #[cfg(mobile)]
            {
                use tauri::Manager as _;
                app.manage(Mobile(handle));
            }
            #[cfg(not(mobile))]
            let _ = (app, api);
            Ok(())
        })
        .build()
}
