//! The passkey providers that live with the platform: the virtual security
//! key on Linux (`linux`), the plugin passkey manager on Windows (`windows`,
//! `registry`) and Credential Manager on Android (`android`,
//! `android_logins`). The rest — the settings, asking the person, the vault's
//! passkeys, the Apple extension — is uwulock-app's (passkeys/mod.rs there);
//! the host (host.rs) starts and stops these after its settings.

#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

#[cfg(any(target_os = "linux", windows, target_os = "android"))]
pub(crate) use uwulock_app::passkeys::*;

#[cfg(target_os = "android")]
pub(crate) mod android;
#[cfg(target_os = "android")]
mod android_logins;
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(any(windows, test))]
mod registry;
#[cfg(windows)]
pub(crate) mod windows;
