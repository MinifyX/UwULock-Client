//! Android: passwords for Credential Manager and the autofill service, the
//! Rust half (`PasskeyBridge.call("logins" | "password", …)`).
//!
//! - `logins` lists the logins for whoever asks — item id, name, user name,
//!   never a password. A locked or closed vault answers `locked`.
//! - `password` hands over one login's user name and password, only after
//!   Kotlin verified the person (`verified`, the screen lock or a strong
//!   biometric) and only when that login is still one for the same caller.
//!
//! Who asks ([`Ask`]): a privileged browser's `origin` (Credential Manager
//! checked it against the browser's certificate; for autofill Kotlin
//! checked the browser's certificate against the same list), or an app by
//! package and certificates, with the page of its WebView as `webDomain`. An
//! app's website logins need the site's Digital Asset Links
//! (`get_login_creds`), see [`super::logins`].

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use uwulock_authenticator::webauthn::from_b64;
use uwulock_core::vault::Item;

use super::logins::{self, Asker};
use super::VaultState;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Ask {
    /// A privileged browser's page: `https://host`.
    #[serde(default)]
    origin: Option<String>,
    #[serde(default)]
    package_name: String,
    /// SHA-256 of each signing certificate, URL-safe base64.
    #[serde(default)]
    cert_hashes: Vec<String>,
    /// The host an app's WebView shows: only ever a site to ask for its
    /// Digital Asset Links.
    #[serde(default)]
    web_domain: Option<String>,
    /// `password`: the login picked.
    #[serde(default)]
    item_id: Option<String>,
    /// `password`: the person passed the screen lock or a biometric just now.
    #[serde(default)]
    verified: bool,
}

pub(super) fn call(app: &AppHandle, method: &str, argument: &str) -> Result<Value, String> {
    let ask: Ask = serde_json::from_str(argument).map_err(|e| e.to_string())?;
    match method {
        "logins" => {
            let Some(items) = open_items(app) else {
                return Ok(json!({ "locked": true, "logins": [] }));
            };
            let asker = asker(&ask)?;
            let found = logins::offers(&items, &asker);
            Ok(json!({
                "locked": false,
                "logins": found.iter().map(|o| json!({
                    "itemId": o.item_id,
                    "name": o.name,
                    "userName": o.user_name,
                })).collect::<Vec<_>>(),
            }))
        }
        "password" => {
            if !ask.verified {
                return Err("passwords only after the screen lock or a biometric".into());
            }
            let item_id = ask.item_id.as_deref().ok_or("no login was picked")?;
            let items = open_items(app).ok_or("locked")?;
            let asker = asker(&ask)?;
            let item = items
                .iter()
                .find(|item| item.id == item_id)
                .filter(|item| logins::offered(item, &asker))
                .ok_or("this login isn't one for the app or site that asks")?;
            let login = item.login.as_ref().ok_or("not a login")?;
            Ok(json!({
                "userName": login.username.as_ref().map(|u| u.as_str()).unwrap_or_default(),
                "password": login.password.as_ref().map(|p| p.as_str()).unwrap_or_default(),
            }))
        }
        other => Err(format!("unknown call {other}")),
    }
}

/// The open account's logins that may be filled at all; `None` when the
/// vault is locked.
fn open_items(app: &AppHandle) -> Option<Vec<Item>> {
    let vault = app.state::<VaultState>();
    let (account_id, _) = vault.active_account().ok()?;
    let guard = vault.unlocked.read();
    let unlocked = guard.get(&account_id)?;
    Some(
        unlocked
            .vault
            .items
            .iter()
            .filter(|item| logins::fillable(item))
            .cloned()
            .collect(),
    )
}

fn asker(ask: &Ask) -> Result<Asker, String> {
    if let Some(origin) = ask.origin.as_deref().filter(|o| !o.is_empty()) {
        let (scheme, rest) = origin.split_once("://").ok_or("not an origin")?;
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let target = logins::web_target(Some(scheme), host).ok_or("not an origin")?;
        return Ok(Asker::Web(target));
    }
    let package = ask.package_name.trim();
    if package.is_empty() {
        return Err("nobody asks".into());
    }
    let certs = ask
        .cert_hashes
        .iter()
        .map(|hash| from_b64(hash))
        .collect::<Result<Vec<_>, _>>()?;
    // Without certificates (Android didn't let UwULock see the app) no site
    // can vouch for it: its own logins only.
    let sites = if certs.is_empty() {
        Vec::new()
    } else {
        logins::app_sites(package, ask.web_domain.as_deref())
            .into_iter()
            .filter(
                |site| match super::android::app_allowed(site, package, &certs) {
                    Ok(allowed) => allowed,
                    Err(error) => {
                        tracing::debug!(%error, site, "no Digital Asset Links");
                        false
                    }
                },
            )
            .collect()
    };
    Ok(Asker::App {
        package: package.to_string(),
        sites,
    })
}
