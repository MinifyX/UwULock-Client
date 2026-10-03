//! Android 14+: Credential Manager's provider, the Rust half.
//!
//! `PasskeyProviderService` and `PasskeyActivity` (Kotlin, in the mobile
//! plugin) answer the system; for anything with the vault they call
//! `PasskeyBridge.nativeCall(method, json)`, which lands here. The vault is
//! the app's open one in this process: when UwULock isn't running or is
//! locked, the service offers "Unlock UwULock" instead of passkeys.
//!
//! Where the request comes from matters. A browser on the privileged list
//! (`res/raw/privileged_browsers.json`, Google's list, release builds only)
//! hands over the web origin, already checked by Android against the
//! browser's certificate; UwULock checks the origin against the RP id. Any
//! other app gets its own origin, `android:apk-key-hash:…`, in the client
//! data, and only when the site's Digital Asset Links name that app and its
//! certificate — so an app can't sign in to a site that doesn't trust it.
//! The same check runs before the service lists anything (cached for a few
//! minutes per app and site), so an untrusted app doesn't even get to show
//! the person's account names for a site.
//!
//! Whether the person has to be verified (fingerprint, face, screen lock) is
//! decided here once (`verification`): Kotlin asks before prompting, and
//! `create`/`get` refuse a request that wanted it without it.

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use uwulock_authenticator::ctap2::ES256;
use uwulock_authenticator::rpid;
use uwulock_authenticator::webauthn::{self, b64, from_b64};
use uwulock_core::passkey::attestation_object;

use super::{matching, VaultState};

static APP: OnceLock<AppHandle> = OnceLock::new();

pub(crate) fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

#[no_mangle]
pub extern "system" fn Java_app_uwulock_mobile_PasskeyBridge_nativeCall<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    method: JString<'local>,
    argument: JString<'local>,
) -> jstring {
    let method: String = env.get_string(&method).map(Into::into).unwrap_or_default();
    let argument: String = env
        .get_string(&argument)
        .map(Into::into)
        .unwrap_or_default();
    let answer = match call(&method, &argument) {
        Ok(value) => value,
        Err(error) => json!({ "error": error }),
    };
    env.new_string(answer.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn app() -> Result<&'static AppHandle, String> {
    APP.get().ok_or_else(|| "not-running".to_string())
}

fn unlocked(app: &AppHandle) -> bool {
    super::is_open(&app.state::<VaultState>())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Caller {
    /// The request as the app or browser made it (WebAuthn JSON).
    request_json: String,
    /// A privileged browser's web origin.
    #[serde(default)]
    origin: Option<String>,
    /// A privileged browser's own client data hash, URL-safe base64.
    #[serde(default)]
    client_data_hash: Option<String>,
    #[serde(default)]
    package_name: String,
    /// SHA-256 of each signing certificate, URL-safe base64.
    #[serde(default)]
    cert_hashes: Vec<String>,
    /// The person passed the screen lock or a biometric just now.
    #[serde(default)]
    verified: bool,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    credential_id: Option<String>,
    /// `list` only: Android didn't say who asks (`callingAppInfo` is null).
    /// The names are listed then, as before; signing checks the caller
    /// anyway.
    #[serde(default)]
    unknown_caller: bool,
}

fn call(method: &str, argument: &str) -> Result<Value, String> {
    match method {
        "status" => Ok(match APP.get() {
            None => json!({ "running": false, "unlocked": false }),
            Some(app) => json!({ "running": true, "unlocked": unlocked(app) }),
        }),
        "list" => {
            let app = app()?;
            if !unlocked(app) {
                return Ok(json!({ "locked": true, "passkeys": [] }));
            }
            let caller: Caller = serde_json::from_str(argument).map_err(|e| e.to_string())?;
            let request_json = caller.request_json.as_str();
            let rp_id = webauthn::rp_id_of(request_json)
                .or_else(|| rp_hint(&caller))
                .ok_or("the request names no site")?;
            // Nothing for a caller that may not use the site's passkeys:
            // not even the names.
            let checked = if caller.unknown_caller {
                if rpid::valid(&rp_id) {
                    Ok(())
                } else {
                    Err(format!("{rp_id:?} isn't a site UwULock keeps passkeys for"))
                }
            } else {
                may_use(&caller, &rp_id)
            };
            if let Err(error) = checked {
                tracing::info!(%error, "passkeys not listed for this caller");
                return Ok(json!({ "locked": false, "passkeys": [], "refused": error }));
            }
            let allow: Vec<Vec<u8>> = serde_json::from_str::<Value>(request_json)
                .ok()
                .and_then(|r| r.get("allowCredentials").cloned())
                .and_then(|list| list.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|d| d.get("id").and_then(Value::as_str))
                .filter_map(|id| from_b64(id).ok())
                .collect();
            let vault = app.state::<VaultState>();
            let found = matching(&vault, &rp_id, &allow).map_err(|f| f.message().to_string())?;
            Ok(json!({
                "locked": false,
                "passkeys": found.iter().map(|f| json!({
                    "itemId": f.item_id,
                    "itemName": f.item_name,
                    "credentialId": f.passkey.credential_id_bytes().map(|id| b64(&id)).unwrap_or_default(),
                    "userName": f.passkey.user_name,
                    "userDisplayName": f.passkey.user_display_name,
                })).collect::<Vec<_>>(),
            }))
        }
        "verification" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Ask {
                request_json: String,
                #[serde(default)]
                create: bool,
            }
            let ask: Ask = serde_json::from_str(argument).map_err(|e| e.to_string())?;
            Ok(json!({ "wanted": wants_verification(&ask.request_json, ask.create) }))
        }
        "create" => {
            let caller: Caller = serde_json::from_str(argument).map_err(|e| e.to_string())?;
            create(app()?, &caller)
        }
        "get" => {
            let caller: Caller = serde_json::from_str(argument).map_err(|e| e.to_string())?;
            get(app()?, &caller)
        }
        other => Err(format!("unknown call {other}")),
    }
}

/// Whether the request wants the person verified: `required` or
/// `preferred` (the default) — anything but `discouraged`, as
/// `webauthn::parse_*` reads it. A request that doesn't parse wants it.
fn wants_verification(request_json: &str, create: bool) -> bool {
    if create {
        webauthn::parse_creation(request_json, Vec::new(), None)
            .map(|c| c.wants_verification)
            .unwrap_or(true)
    } else {
        webauthn::parse_request(request_json, Vec::new(), None)
            .map(|a| a.wants_verification)
            .unwrap_or(true)
    }
}

/// The host of a privileged browser's origin: the rpId when the request
/// leaves it out.
fn rp_hint(caller: &Caller) -> Option<String> {
    caller
        .origin
        .as_deref()
        .and_then(|o| o.split("://").nth(1))
        .map(|rest| {
            rest.split([':', '/'])
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .filter(|host| !host.is_empty())
}

/// Whether the caller may use passkeys of `rp_id` at all: a site UwULock
/// takes, and a privileged browser whose origin belongs to it or an app the
/// site's Digital Asset Links name.
fn may_use(caller: &Caller, rp_id: &str) -> Result<(), String> {
    if !rpid::valid(rp_id) {
        return Err(format!("{rp_id:?} isn't a site UwULock keeps passkeys for"));
    }
    if let Some(origin) = caller.origin.as_deref().filter(|o| !o.is_empty()) {
        if !webauthn::origin_allows(origin, rp_id) {
            return Err(format!("{origin} may not use passkeys of {rp_id}"));
        }
        return Ok(());
    }
    let certs = caller
        .cert_hashes
        .iter()
        .map(|hash| from_b64(hash))
        .collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err("the app has no signing certificate".into());
    }
    if !app_allowed(rp_id, &caller.package_name, &certs)? {
        return Err(format!(
            "{rp_id} doesn't let the app {} use its passkeys (Digital Asset Links)",
            caller.package_name
        ));
    }
    Ok(())
}

type LinksKey = (String, String, Vec<Vec<u8>>);

/// Digital Asset Links answers, per site, app and certificates: a yes for
/// five minutes, a no for one. Failed fetches aren't kept.
static LINKS: OnceLock<Mutex<HashMap<LinksKey, (Instant, bool)>>> = OnceLock::new();

fn app_allowed(rp_id: &str, package: &str, certs: &[Vec<u8>]) -> Result<bool, String> {
    let key: LinksKey = (rp_id.to_string(), package.to_string(), certs.to_vec());
    let cache = LINKS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&(at, allowed)) = cache.lock().map_err(|e| e.to_string())?.get(&key) {
        let keep = if allowed { 300 } else { 60 };
        if at.elapsed() < Duration::from_secs(keep) {
            return Ok(allowed);
        }
    }
    let allowed = webauthn::asset_links_allow(&asset_links(rp_id)?, package, certs);
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    cache.insert(key, (Instant::now(), allowed));
    Ok(allowed)
}

/// The client data for the answer: `None` when the browser made its own,
/// with the hash to sign either way.
struct ClientData {
    json: Option<String>,
    hash: Vec<u8>,
}

fn client_data(
    caller: &Caller,
    kind: &str,
    rp_id: &str,
    challenge: &str,
) -> Result<ClientData, String> {
    may_use(caller, rp_id)?;
    if let Some(origin) = caller.origin.as_deref().filter(|o| !o.is_empty()) {
        if let Some(hash) = &caller.client_data_hash {
            let hash = from_b64(hash)?;
            if hash.len() != 32 {
                return Err("the browser's client data hash isn't a SHA-256".into());
            }
            return Ok(ClientData { json: None, hash });
        }
        let json = webauthn::client_data_json(kind, challenge, origin, None);
        let hash = webauthn::sha256(json.as_bytes());
        return Ok(ClientData {
            json: Some(json),
            hash,
        });
    }
    // An app the site trusts (checked above): its own origin.
    let first = caller
        .cert_hashes
        .first()
        .map(|hash| from_b64(hash))
        .transpose()?
        .ok_or("the app has no signing certificate")?;
    let origin = webauthn::apk_origin(&first);
    let json = webauthn::client_data_json(kind, challenge, &origin, Some(&caller.package_name));
    let hash = webauthn::sha256(json.as_bytes());
    Ok(ClientData {
        json: Some(json),
        hash,
    })
}

/// `https://<rp id>/.well-known/assetlinks.json`.
fn asset_links(rp_id: &str) -> Result<String, String> {
    if !rpid::valid(rp_id) {
        return Err("not a host".into());
    }
    let url = format!("https://{rp_id}/.well-known/assetlinks.json");
    tauri::async_runtime::block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{url} answered {}", response.status()));
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > 256 * 1024 {
            return Err("assetlinks.json is too large".into());
        }
        String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
    })
}

/// The login a new passkey goes into: the one the person picked, else a
/// login for the site with the same user name that has no passkey yet,
/// else a new one.
fn target_login(app: &AppHandle, rp_id: &str, user_name: Option<&str>) -> Option<String> {
    let user_name = user_name?;
    let vault = app.state::<VaultState>();
    super::logins_for_site(&vault, rp_id)
        .into_iter()
        .find(|login| !login.has_passkey && login.user_name.as_deref() == Some(user_name))
        .map(|login| login.item_id)
}

fn create(app: &AppHandle, caller: &Caller) -> Result<Value, String> {
    let rp_hint = rp_hint(caller);
    let creation = webauthn::parse_creation(&caller.request_json, Vec::new(), rp_hint.as_deref())?;
    if creation.wants_verification && !caller.verified {
        return Err("the site wants the person verified, and they weren't".into());
    }
    let mut request = creation.request;
    if !request.algorithms.contains(&ES256) {
        return Err("the site takes no ES256 passkeys".into());
    }
    let data = client_data(
        caller,
        "webauthn.create",
        &request.rp.id,
        &creation.challenge,
    )?;
    request.client_data_hash = data.hash;
    let vault = app.state::<VaultState>();
    if !request.exclude_list.is_empty()
        && matching(&vault, &request.rp.id, &request.exclude_list).is_ok_and(|f| !f.is_empty())
    {
        return Err("excluded".into());
    }
    let item_id = caller
        .item_id
        .clone()
        .or_else(|| target_login(app, &request.rp.id, request.user.name.as_deref()));
    let made = tauri::async_runtime::block_on(super::create(
        app,
        &request,
        item_id.as_deref(),
        caller.verified,
    ))
    .map_err(|f| f.message().to_string())?;
    let credential_id = made
        .passkey
        .credential_id_bytes()
        .map_err(|e| e.to_string())?;
    let spki = made.passkey.public_key_spki().map_err(|e| e.to_string())?;
    Ok(json!({
        "response": webauthn::registration_response(
            &credential_id,
            data.json.as_deref(),
            &attestation_object(&made.auth_data),
            &made.auth_data,
            &spki,
        ),
    }))
}

fn get(app: &AppHandle, caller: &Caller) -> Result<Value, String> {
    let rp_hint = rp_hint(caller);
    let assertion = webauthn::parse_request(&caller.request_json, Vec::new(), rp_hint.as_deref())?;
    if assertion.wants_verification && !caller.verified {
        return Err("the site wants the person verified, and they weren't".into());
    }
    let rp_id = assertion.request.rp_id.clone();
    let data = client_data(caller, "webauthn.get", &rp_id, &assertion.challenge)?;
    let item_id = caller.item_id.as_deref().ok_or("no passkey was picked")?;
    let credential_id = from_b64(
        caller
            .credential_id
            .as_deref()
            .ok_or("no passkey was picked")?,
    )?;
    if !assertion.request.allow_list.is_empty()
        && !assertion.request.allow_list.contains(&credential_id)
    {
        return Err("the site didn't ask for this passkey".into());
    }
    let signed = tauri::async_runtime::block_on(super::sign(
        app,
        &rp_id,
        item_id,
        &credential_id,
        &data.hash,
        true,
        caller.verified,
    ))
    .map_err(|f| f.message().to_string())?;
    let user_handle = signed.passkey.user_handle_bytes().ok().flatten();
    Ok(json!({
        "response": webauthn::authentication_response(
            &credential_id,
            data.json.as_deref(),
            &signed.auth_data,
            &signed.signature,
            user_handle.as_deref(),
        ),
    }))
}
