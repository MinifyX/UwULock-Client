//! The Safari extension the Apple builds carry (docs/extension.md): the same
//! extension as in Chrome and Firefox, with its own login and sync, in an app
//! extension Safari loads from UwULock. The app only says whether it is
//! switched on and opens Safari's settings for it.
//!
//! - **macOS**: `SFSafariExtensionManager` answers whether it is on,
//!   `SFSafariApplication` opens Safari's Extensions settings on it. Only in
//!   builds that carry it (`PlugIns/UwULockSafari.appex`): the disk image's
//!   and the Mac App Store's. Its id is the app's with `.safari`.
//! - **iOS**: no way to ask; the page says where to switch it on.
//! - Elsewhere: nothing.
//!
//! Asking Safari is the host's ([`Host::safari_extension_status`](crate::host::Host::safari_extension_status),
//! [`Host::safari_extension_open`](crate::host::Host::safari_extension_open)); both block, so they run off the async
//! runtime's threads here.

use serde::Serialize;

use crate::vault::{Failure, Result};
use crate::App;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafariView {
    /// `macos`, `ios` or `none`.
    pub platform: &'static str,
    /// This build carries the extension.
    pub available: bool,
    /// Switched on in Safari; `None` when Safari doesn't say (iOS, or no answer in time).
    pub enabled: Option<bool>,
}

impl SafariView {
    /// What a host that can't ask Safari says: on iOS that the extension is
    /// there, elsewhere that there is none.
    pub fn unasked() -> SafariView {
        if cfg!(target_os = "ios") {
            SafariView {
                platform: "ios",
                available: true,
                enabled: None,
            }
        } else {
            SafariView {
                platform: "none",
                ..SafariView::default()
            }
        }
    }
}

impl App {
    pub async fn safari_extension_status(&self) -> SafariView {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || app.host().safari_extension_status())
            .await
            .unwrap_or_default()
    }

    /// Opens Safari's settings on the extension (macOS).
    pub async fn safari_extension_open(&self) -> Result<()> {
        let app = self.clone();
        crate::runtime::spawn_blocking(move || app.host().safari_extension_open())
            .await
            .map_err(|e| Failure::new("failed", e.to_string()))?
    }
}
