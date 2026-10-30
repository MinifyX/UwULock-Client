//! UwULock's app logic ([`uwulock_app`]) for the native Apple app (apps/apple), through UniFFI.
//!
//! One object, [`UwuLockCore`]: the app with its accounts in a folder and a [`HostDelegate`]
//! (Swift's `AppleHost`) for what only the system does. Every command of the Tauri app goes
//! through [`UwuLockCore::call`] with the same name and the same camelCase JSON arguments, and
//! answers with the same JSON; a failure is the page's `{"kind", "message"}`
//! ([`CoreError::Failure`]). Events reach the delegate's `emit` with their JSON payload.
//!
//! The Swift bindings, the static library and the XCFramework: scripts/apple-xcframework.sh.

mod dispatch;
mod host;

use std::sync::Arc;

use uwulock_app::{runtime, App, Failure, Storage};

pub use host::{HostDelegate, HostError, OutboxFile, ProviderState, SafariState};

uniffi::setup_scaffolding!();

/// What a command answers instead of its JSON.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// The page's failure as JSON: `{"kind": "…", "message": "…"}` (errors.ts).
    #[error("{json}")]
    Failure { json: String },
}

impl From<Failure> for CoreError {
    fn from(failure: Failure) -> Self {
        let json = serde_json::to_string(&failure).unwrap_or_else(|_| {
            r#"{"kind":"failed","message":"The answer didn't serialise."}"#.into()
        });
        CoreError::Failure { json }
    }
}

/// The app: its accounts, its vault, its background work.
#[derive(uniffi::Object)]
pub struct UwuLockCore {
    app: App,
}

#[uniffi::export]
impl UwuLockCore {
    /// The app with its accounts in `data_dir` (made if it isn't there). Nothing runs yet: see
    /// [`UwuLockCore::start`].
    #[uniffi::constructor]
    pub fn new(data_dir: String, host: Arc<dyn HostDelegate>) -> Result<Arc<Self>, CoreError> {
        let storage = Storage::new(data_dir.into()).map_err(|error| {
            Failure::new("storage", format!("The data folder isn't usable: {error}"))
        })?;
        tracing::info!(path = %storage.dir().display(), "data folder");
        let app = App::new(storage, Arc::new(host::Adapter::new(host)));
        Ok(Arc::new(UwuLockCore { app }))
    }

    /// Starts syncing, live changes, the extras, the screen lock watch and the passkey
    /// providers. Once.
    pub fn start(&self) {
        self.app.start();
    }

    /// Runs `command` with `args_json` (an object; empty for none) and answers with its JSON:
    /// the Tauri app's commands, names and arguments as the page sends them.
    pub async fn call(&self, command: String, args_json: String) -> Result<String, CoreError> {
        let app = self.app.clone();
        let task = match dispatch::parse(&command, &args_json)? {
            dispatch::Call::Now(call) => {
                runtime::spawn_blocking(move || dispatch::run_now(&app, call))
            }
            dispatch::Call::Later(call) => runtime::spawn(dispatch::run_later(app, call)),
        };
        task.await
            .map_err(|error| Failure::new("failed", format!("{command} stopped: {error}")))?
            .map_err(CoreError::from)
    }

    /// `stage_send_file` with the file's bytes (the page sends them as the raw body); `None`
    /// drops what was staged.
    pub async fn stage_send_file(&self, bytes: Option<Vec<u8>>) -> Result<(), CoreError> {
        let app = self.app.clone();
        runtime::spawn_blocking(move || app.stage_send_file(bytes.as_deref()))
            .await
            .map_err(|error| Failure::new("failed", format!("stage_send_file stopped: {error}")))?
            .map_err(CoreError::from)
    }

    /// The app went to the background (Tauri's `Suspended`).
    pub fn suspended(&self) {
        self.app.suspended();
    }

    /// The app is in front again (Tauri's `Resumed`).
    pub fn resumed(&self) {
        self.app.resumed();
    }

    /// The system locked, slept or started the screen saver: `why` for the log.
    pub fn system_locked(&self, why: String) {
        self.app.system_locked(&why);
    }

    /// Every command [`UwuLockCore::call`] knows.
    pub fn commands(&self) -> Vec<String> {
        dispatch::COMMANDS
            .iter()
            .map(|name| name.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_is_the_pages_json() {
        let CoreError::Failure { json } = CoreError::from(Failure::new("locked", "Locked."));
        assert_eq!(json, r#"{"kind":"locked","message":"Locked."}"#);
    }
}
