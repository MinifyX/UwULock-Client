//! What uwulock-app needs from the Tauri app around it ([`Host`]): the
//! page's events, the window, the clipboard, the system's biometrics, the
//! phone's plugin, the passkey providers that live with the platform and the
//! Apple builds' extensions.
//!
//! The phone's plugin waits for the phone's main thread, and a Tauri command
//! that isn't async runs on it: what the app may call from a command (the
//! clipboard) goes to a thread of its own here. The rest the app calls off
//! the command threads already (host.rs in uwulock-app has the contract).

#[cfg(target_os = "android")]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(desktop)]
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
#[cfg(any(mobile, target_os = "macos"))]
use uwulock_app::autofill::ProviderState;
use uwulock_app::hello::Prompt;
use uwulock_app::passkeys::Settings;
#[cfg(target_os = "macos")]
use uwulock_app::safari::SafariView;
#[cfg(target_os = "android")]
use uwulock_app::wifi::Network;
use uwulock_app::{App, Failure, Host};
use zeroize::Zeroizing;

pub(crate) struct TauriHost {
    handle: AppHandle,
}

impl TauriHost {
    pub(crate) fn new(handle: AppHandle) -> TauriHost {
        TauriHost { handle }
    }
}

#[cfg(mobile)]
fn phone_failure(error: tauri_plugin_uwulock_mobile::Error) -> Failure {
    let kind = match error.code.as_deref() {
        Some("cancelled") => "biometric-cancelled",
        Some("lockout") => "biometric-lockout",
        Some("invalidated") | Some("missing") => "biometric-changed",
        _ => "hello",
    };
    Failure::new(kind, error.message)
}

#[cfg(mobile)]
fn phone() -> Result<tauri::State<'static, tauri_plugin_uwulock_mobile::Mobile<tauri::Wry>>, Failure>
{
    crate::phone::plugin().ok_or_else(|| Failure::new("hello", "The phone isn't ready yet."))
}

#[cfg(mobile)]
fn phone_prompt(prompt: &Prompt) -> tauri_plugin_uwulock_mobile::Prompt {
    tauri_plugin_uwulock_mobile::Prompt {
        title: prompt.title.clone(),
        subtitle: prompt.subtitle.clone(),
        cancel: prompt.cancel.clone(),
    }
}

#[cfg(target_os = "ios")]
fn plugin_missing() -> Failure {
    Failure::new("unsupported", "the phone's plugin isn't there")
}

impl Host for TauriHost {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.handle.emit(event, payload);
    }

    fn show_window(&self) {
        #[cfg(desktop)]
        if let Some(window) = self.handle.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        self.handle
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| e.to_string())
    }

    // ── The clipboard ────────────────────────────────────────

    #[cfg(desktop)]
    fn clipboard_copy(
        &self,
        text: &str,
        sensitive: bool,
        _expires: Option<Duration>,
    ) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("No clipboard: {e}"))?;
        if !sensitive {
            return clipboard
                .set_text(text.to_string())
                .map_err(|e| format!("Couldn't copy: {e}"));
        }
        let set = clipboard.set();
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_history().exclude_from_cloud()
        };
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple;
            set.exclude_from_history()
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let set = {
            use arboard::SetExtLinux;
            set.exclude_from_history()
        };
        set.text(text.to_string())
            .map_err(|e| format!("Couldn't copy: {e}"))
    }

    #[cfg(mobile)]
    fn clipboard_copy(
        &self,
        text: &str,
        sensitive: bool,
        expires: Option<Duration>,
    ) -> Result<(), String> {
        let plugin = crate::phone::plugin().ok_or("No clipboard yet.")?;
        let text = Zeroizing::new(text.to_string());
        std::thread::spawn(move || {
            let copied = if sensitive {
                plugin.copy(&text, expires.map(|d| d.as_secs()))
            } else {
                plugin.copy_text(&text)
            };
            if let Err(error) = copied {
                tracing::warn!(%error, "couldn't copy");
            }
        });
        Ok(())
    }

    #[cfg(desktop)]
    fn clipboard_clear(&self, sha256: &[u8; 32]) {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return;
        };
        if clipboard
            .get_text()
            .is_ok_and(|current| <[u8; 32]>::from(Sha256::digest(current.as_bytes())) == *sha256)
        {
            let _ = clipboard.clear();
            tracing::debug!("clipboard cleared");
        }
    }

    /// The plugin knows which copy is UwULock's and leaves anything newer
    /// alone.
    #[cfg(mobile)]
    fn clipboard_clear(&self, _sha256: &[u8; 32]) {
        if let Some(plugin) = crate::phone::plugin() {
            std::thread::spawn(move || {
                if plugin.clear_clipboard().is_ok() {
                    tracing::debug!("clipboard cleared");
                }
            });
        }
    }

    // ── Biometrics ───────────────────────────────────────────

    fn biometric_kind(&self) -> Option<String> {
        #[cfg(mobile)]
        {
            let status = crate::phone::plugin()?.unlock_status().ok()?;
            status
                .available
                .then(|| status.kind.unwrap_or_else(|| "biometric".into()))
        }
        #[cfg(target_os = "macos")]
        {
            crate::hello::touch::available().then(|| "touchId".to_string())
        }
        #[cfg(windows)]
        {
            crate::hello::platform::available().then(|| "windowsHello".to_string())
        }
        #[cfg(not(any(mobile, target_os = "macos", windows)))]
        None
    }

    fn biometric_secret(
        &self,
        name: &str,
        challenge: &[u8],
        create: bool,
        prompt: &Prompt,
    ) -> Result<Zeroizing<[u8; 32]>, Failure> {
        #[cfg(mobile)]
        {
            let _ = challenge;
            let plugin = phone()?;
            let prompt = phone_prompt(prompt);
            if create {
                plugin.unlock_create(name, &prompt)
            } else {
                plugin.unlock_open(name, &prompt)
            }
            .map_err(phone_failure)
        }
        #[cfg(target_os = "macos")]
        {
            let _ = challenge;
            if create {
                crate::hello::touch::create(name, &prompt.reason, &prompt.cancel)
            } else {
                crate::hello::touch::read(name, &prompt.reason, &prompt.cancel)
            }
        }
        #[cfg(windows)]
        {
            let _ = prompt;
            crate::hello::platform::secret(name, challenge, create)
                .map(Zeroizing::new)
                .map_err(|message| Failure::new("hello", message))
        }
        #[cfg(not(any(mobile, target_os = "macos", windows)))]
        {
            let _ = (name, challenge, create, prompt);
            Err(Failure::new(
                "unavailable",
                "Nothing unlocks with biometrics here.",
            ))
        }
    }

    fn biometric_forget(&self, name: &str) {
        #[cfg(mobile)]
        if let Ok(plugin) = phone() {
            let _ = plugin.unlock_delete(name);
        }
        #[cfg(target_os = "macos")]
        crate::hello::touch::delete(name, false);
        #[cfg(windows)]
        crate::hello::platform::delete(name);
        #[cfg(not(any(mobile, target_os = "macos", windows)))]
        let _ = name;
    }

    // ── The system ───────────────────────────────────────────

    #[cfg(any(windows, target_os = "macos"))]
    fn screen_locked(&self) -> bool {
        crate::session_lock::screen_locked()
    }

    fn download_dir(&self) -> Result<PathBuf, String> {
        let path = self.handle.path();
        #[cfg(desktop)]
        {
            path.download_dir()
                .or_else(|_| path.home_dir())
                .map_err(|e| format!("No Downloads folder: {e}"))
        }
        #[cfg(target_os = "ios")]
        {
            path.document_dir()
                .map_err(|e| format!("No documents folder: {e}"))
        }
        #[cfg(target_os = "android")]
        {
            path.app_cache_dir()
                .map_err(|e| format!("No cache folder: {e}"))
        }
    }

    #[cfg(target_os = "android")]
    fn publish_download(&self, path: &Path, name: &str) -> Result<String, String> {
        crate::phone::plugin()
            .ok_or("The phone isn't ready yet.".to_string())?
            .save_to_downloads(&path.display().to_string(), name)
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "android")]
    fn wifi_join(&self, network: &Network) -> Option<Result<String, Failure>> {
        let Some(plugin) = crate::phone::plugin() else {
            return Some(Err(Failure::new("not-ready", "The phone isn't ready yet.")));
        };
        Some(plugin.connect_wifi(network).map_err(|error| {
            if error.is("unsupported") {
                Failure::new("unsupported", error.message)
            } else {
                Failure::new("failed", error.message)
            }
        }))
    }

    #[cfg(target_os = "android")]
    fn wifi_settings(&self) -> Result<bool, Failure> {
        let plugin = crate::phone::plugin()
            .ok_or_else(|| Failure::new("wifi", "The phone isn't ready yet."))?;
        plugin
            .open_wifi_settings()
            .map(|()| true)
            .map_err(|error| Failure::new("wifi", error.message))
    }

    // ── Passkey and AutoFill providers ───────────────────────

    fn passkey_provider_apply(&self, app: &App, settings: &Settings) {
        #[cfg(target_os = "linux")]
        if settings.security_key {
            if let Err(error) = crate::passkeys::linux::start(app) {
                tracing::warn!(%error, "the virtual security key didn't start");
            }
        } else {
            crate::passkeys::linux::stop(app);
        }
        #[cfg(windows)]
        if settings.windows_plugin {
            if let Err(error) = crate::passkeys::windows::start(app) {
                tracing::warn!(%error, "the Windows passkey plugin didn't start");
            }
        } else {
            crate::passkeys::windows::stop(app);
        }
        let _ = (app, settings);
    }

    fn passkey_provider_state(&self, app: &App) -> Option<(&'static str, bool, Option<String>)> {
        let _ = app;
        #[cfg(target_os = "linux")]
        {
            Some((
                "linux",
                crate::passkeys::linux::DEVICE.lock().is_some(),
                crate::passkeys::linux::problem(),
            ))
        }
        #[cfg(windows)]
        {
            Some((
                "windows",
                crate::passkeys::windows::PLUGIN.lock().is_some(),
                crate::passkeys::windows::problem(),
            ))
        }
        #[cfg(target_os = "android")]
        {
            Some(("android", true, None))
        }
        #[cfg(not(any(target_os = "linux", windows, target_os = "android")))]
        None
    }

    #[cfg(mobile)]
    fn autofill_provider_status(&self) -> Result<ProviderState, String> {
        let plugin = crate::phone::plugin().ok_or("the phone's plugin isn't there")?;
        let state = plugin.provider_status().map_err(|e| e.to_string())?;
        Ok(ProviderState {
            supported: state.supported,
            enabled: state.enabled,
            direct: state.direct,
            autofill: state.autofill,
        })
    }

    #[cfg(target_os = "macos")]
    fn autofill_provider_status(&self) -> Result<ProviderState, String> {
        use crate::apple::autofill;
        let supported = crate::apple::extension::problem().is_none();
        Ok(ProviderState {
            supported,
            enabled: supported.then(autofill::enabled).flatten(),
            direct: supported && autofill::can_ask(),
            autofill: None,
        })
    }

    #[cfg(mobile)]
    fn autofill_provider_request(&self, target: &str) -> Result<ProviderState, Failure> {
        let plugin = crate::phone::plugin()
            .ok_or_else(|| Failure::new("unsupported", "the phone's plugin isn't there"))?;
        let state = plugin
            .provider_request(target)
            .map_err(|e| Failure::new("failed", e.message))?;
        Ok(ProviderState {
            supported: state.supported,
            enabled: state.enabled,
            direct: state.direct,
            autofill: state.autofill,
        })
    }

    #[cfg(target_os = "macos")]
    fn autofill_provider_request(&self, _target: &str) -> Result<ProviderState, Failure> {
        if crate::apple::extension::problem().is_none() {
            crate::apple::autofill::ask(&self.handle);
        }
        Ok(self.autofill_provider_status().unwrap_or_default())
    }

    #[cfg(target_os = "ios")]
    fn credential_exchange_pending(&self) -> bool {
        crate::phone::plugin()
            .and_then(|plugin| plugin.credential_exchange_pending().ok())
            .unwrap_or(false)
    }

    #[cfg(target_os = "ios")]
    fn credential_exchange_import(&self, discard: bool) -> Result<String, Failure> {
        let plugin = crate::phone::plugin().ok_or_else(plugin_missing)?;
        plugin.credential_exchange_import(discard).map_err(|e| {
            Failure::new(
                if e.is("missing") { "missing" } else { "failed" },
                e.message,
            )
        })
    }

    #[cfg(target_os = "ios")]
    fn autofill_log(&self) -> Option<Result<String, String>> {
        match crate::phone::plugin()?.autofill_log() {
            Ok(Some(text)) => Some(Ok(text)),
            Ok(None) => Some(Err("no App Group in this build".into())),
            Err(error) => Some(Err(error.to_string())),
        }
    }

    #[cfg(target_os = "macos")]
    fn autofill_log(&self) -> Option<Result<String, String>> {
        let path = crate::apple::extension::log_path().ok()?;
        Some(
            match uwulock_app::autofill::read_tail(&path, uwulock_app::autofill::LOG_BYTES) {
                Ok(text) => Ok(text),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
                Err(error) => Err(error.to_string()),
            },
        )
    }

    #[cfg(target_os = "ios")]
    fn autofill_log_clear(&self) -> Result<(), Failure> {
        crate::phone::plugin()
            .ok_or_else(plugin_missing)?
            .autofill_log_clear()
            .map_err(|e| Failure::new("failed", e.message))
    }

    #[cfg(target_os = "macos")]
    fn autofill_log_clear(&self) -> Result<(), Failure> {
        let path =
            crate::apple::extension::log_path().map_err(|e| Failure::new("unsupported", e))?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Failure::new("io", e.to_string())),
        }
    }

    #[cfg(target_os = "macos")]
    fn safari_extension_status(&self) -> SafariView {
        use crate::safari::mac;
        let available = mac::bundled();
        SafariView {
            platform: "macos",
            available,
            enabled: available
                .then(|| mac::enabled(&mac::identifier(&self.handle)))
                .flatten(),
        }
    }

    #[cfg(target_os = "macos")]
    fn safari_extension_open(&self) -> Result<(), Failure> {
        use crate::safari::mac;
        if !mac::bundled() {
            return Err(Failure::new(
                "unsupported",
                "this build has no Safari extension",
            ));
        }
        mac::open(&mac::identifier(&self.handle))
    }

    // ── The Apple AutoFill extension ─────────────────────────

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_problem(&self) -> Option<String> {
        crate::apple::extension::problem()
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_note(&self, text: &str) {
        crate::apple::extension::note(text);
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_store(
        &self,
        list: &[u8],
        key: &[u8; 32],
        identities: &[uwulock_app::passkeys::apple::Identity],
    ) -> Result<(), String> {
        crate::apple::extension::store(list, key, identities)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_outbox(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        crate::apple::extension::outbox()
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_clear_outbox(&self, names: &[String]) -> Result<(), String> {
        crate::apple::extension::clear_outbox(names)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_set_aside(&self, names: &[String]) -> Result<(), String> {
        crate::apple::extension::set_aside(names)
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    fn extension_clear(&self) -> Result<(), String> {
        crate::apple::extension::clear()
    }
}
