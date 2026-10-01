//! UwULock Server's extras (UwULock-Server's `docs/uwu-api.md`): own and
//! automatic icons, entry versions, renewal reminders, file requests, masked
//! addresses, travel mode, send domains — and sharing an item as a Send,
//! which Bitwarden and Vaultwarden can do too.
//!
//! Each is offered only when the server's `/uwu/v1/info` lists it
//! ([`Unlocked::has`]); an account on Bitwarden or Vaultwarden sees nothing of
//! this but "share as Send". Like the rest of the app, secrets stay here: a
//! version's password reaches the page only when someone clicks the eye.
//!
//! What UwULock keeps beyond Bitwarden's objects (own icons of personal items,
//! the labels and link secrets of file requests) is under the account's
//! extras key, which is opened — or made — the first time something needs it
//! ([`extras_key`]).
//!
//! Entry versions: the desktop app has no key rotation of its own, so it never
//! re-encrypts them (contract §8.5); a rotation in the web vault does.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_opener::OpenerExt;
use uwulock_bitwarden::api::parse_cipher;
use uwulock_bitwarden::delta::{IconRef, MaskedLink, Unseen};
use uwulock_bitwarden::extras::{open_icon, seal_icon};
use uwulock_bitwarden::file_request::{
    self, LinkSecret, PublicInfo, SealedFile, SubmissionKey, TEXT_MAX_CHARS,
};
use uwulock_bitwarden::send::{self as send_core, shareable_value, TextSend};
use uwulock_bitwarden::uwu::{
    FileRequest, FileRequestBody, Info, Limits, MaskedAddress, MaskedConnection, NewMaskedAddress,
    SendDomain, Submission, UwuError,
};
use uwulock_bitwarden::vault::{Item, ItemKind, Organization};
use uwulock_bitwarden::{icons, Client, SymmetricKey};
use zeroize::Zeroizing;

use crate::vault::{
    access_token, emit_status, entry_id, host_of, iso_from_unix, now, patch_cache, prepare,
    sync_account, Failure, Patch, Result, Unlocked, VaultState,
};

/// A PNG, shared between the cache and the answers made from it.
type Png = Arc<Vec<u8>>;

/// What the extras opened in one unlock. Part of [`Unlocked`], so locking
/// drops it with everything else.
#[derive(Default)]
pub(crate) struct Cache {
    /// Own icons by item: the revision the PNG is of; `None` for one that
    /// didn't open.
    own: HashMap<String, (Option<String>, Option<Png>)>,
    /// Automatic icons by host; `None` when the server has none.
    automatic: HashMap<String, Option<Png>>,
    /// Versions opened for comparing, by version id: the item's id and the
    /// version as an item.
    versions: HashMap<String, (String, Item)>,
    /// Travel mode's hidden count, when it was asked for since the last sync.
    travel_hidden: Option<u32>,
    /// The extras key opened as a different one than this device took last
    /// time: its id, until the person has seen the warning.
    key_changed: Option<String>,
}

/// The server's refusal as the page gets it: the contract's code as the kind
/// where the page says something of its own about it.
pub(crate) fn uwu_failure(error: UwuError) -> Failure {
    let kind = match error.code() {
        Some("feature_off") => "feature-off",
        Some("not_connected") => "not-connected",
        Some("revoked") => "revoked",
        Some("quota") => "quota",
        Some("too_large") => "too-large",
        Some("rate_limited") => "rate-limited",
        Some("upstream") => "upstream",
        Some("travel_active") => "travel-active",
        Some("space_changed") => "space-changed",
        _ => return Failure::from(uwulock_bitwarden::Error::from(error)),
    };
    Failure::new(kind, error.to_string())
}

/// The account on screen, a client for its server and a token that works.
pub(crate) struct Ctx {
    pub(crate) account_id: String,
    pub(crate) client: Client,
    pub(crate) token: Zeroizing<String>,
}

pub(crate) async fn ctx(state: &VaultState) -> Result<Ctx> {
    let (account_id, account) = state.active_account()?;
    let client = state.client(account.server.clone())?;
    let token = access_token(state, &account_id).await?;
    Ok(Ctx {
        account_id,
        client,
        token,
    })
}

/// Runs `f` on the account's open vault.
pub(crate) fn with<T>(
    state: &VaultState,
    id: &str,
    f: impl FnOnce(&mut Unlocked) -> Result<T>,
) -> Result<T> {
    let mut guard = state.unlocked.write();
    let unlocked = guard.get_mut(id).ok_or_else(Failure::locked)?;
    f(unlocked)
}

/// Refuses unless the server offers `feature`.
fn need(state: &VaultState, feature: &str) -> Result<()> {
    let (id, _) = state.active_account()?;
    with(state, &id, |u| {
        if u.has(feature) {
            Ok(())
        } else {
            Err(Failure::new(
                "feature-off",
                format!("The server doesn't offer {feature}."),
            ))
        }
    })
}

/// The account's extras key: opened from the server, made there if there is
/// none, wrapped again if an official client rotated the user key. Kept for
/// the rest of the unlock.
///
/// The key's id is kept with the account: a key that isn't the one this
/// device took before is still used (the server can't have chosen it — both
/// its wraps are made with keys only the account has), but the page is told,
/// since everything under the old one is gone.
pub(crate) async fn extras_key(
    app: &AppHandle,
    state: &VaultState,
    ctx: &Ctx,
) -> Result<SymmetricKey> {
    let (user_key, private) = with(state, &ctx.account_id, |u| {
        Ok((
            u.extras
                .clone()
                .map(Ok)
                .unwrap_or_else(|| Err(u.user_key.clone())),
            u.vault.private_key().cloned(),
        ))
    })?;
    let user_key = match user_key {
        Ok(known) => return Ok(known),
        Err(user_key) => user_key,
    };
    match ctx
        .client
        .extras_key(&ctx.token, &user_key, private.as_ref())
        .await
    {
        Ok(Some(key)) => {
            let id = uwulock_bitwarden::extras::key_id(&key);
            let known = state.account(&ctx.account_id)?.extras_key_id;
            let different = known.as_ref().is_some_and(|known| *known != id);
            if known.is_none() {
                state.update_account(&ctx.account_id, |a| a.extras_key_id = Some(id.clone()));
            }
            with(state, &ctx.account_id, |u| {
                u.extras = Some(key.clone());
                u.extras_cache.key_changed = different.then_some(id);
                Ok(())
            })?;
            if different {
                tracing::warn!("the extras key is a different one than this device took before");
                changed(app);
            }
            Ok(key)
        }
        Ok(None) => Err(Failure::new(
            "extras-lost",
            "The key for UwULock's extras can't be opened any more.",
        )),
        Err(error) => Err(uwu_failure(error)),
    }
}

/// The person saw that the extras key changed: it is the one from now on.
#[tauri::command]
pub(crate) fn uwu_extras_key_seen(app: AppHandle, state: State<'_, VaultState>) -> Result<()> {
    let (id, _) = state.active_account()?;
    let seen = with(&state, &id, |u| Ok(u.extras_cache.key_changed.take()))?;
    if let Some(key_id) = seen {
        state.update_account(&id, |a| a.extras_key_id = Some(key_id));
    }
    changed(&app);
    Ok(())
}

fn changed(app: &AppHandle) {
    let state = app.state::<VaultState>();
    let _ = app.emit("uwu-changed", status_of(&state));
}

fn vault_changed(app: &AppHandle) {
    let _ = app.emit("vault-changed", ());
    emit_status(app);
}

/// `uwu-changed` after every sync: the page learns the new badges, marks and
/// icons without asking.
pub(crate) fn start(app: &AppHandle) {
    let handle = app.clone();
    app.listen_any("vault-changed", move |_| {
        let app = handle.clone();
        // Not from inside the event's own dispatch.
        tauri::async_runtime::spawn(async move {
            if let Ok((id, _)) = app.state::<VaultState>().active_account() {
                // A new sync may have a newer count.
                let _ = with(&app.state::<VaultState>(), &id, |u| {
                    u.extras_cache.travel_hidden = None;
                    Ok(())
                });
            }
            changed(&app);
        });
    });
}

// ── Status ─────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Travel {
    enabled: bool,
    hidden_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderMark {
    due: Option<String>,
    every_months: Option<u32>,
    is_due: bool,
}

/// What the page needs to know of UwULock's extras for the account on screen.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UwuStatus {
    /// A UwULock Server (else everything below is empty).
    uwu: bool,
    features: Vec<String>,
    travel: Travel,
    unseen: Unseen,
    organizations: Vec<Organization>,
    send_domains: Vec<SendDomain>,
    /// Reminders by item.
    reminders: BTreeMap<String, ReminderMark>,
    /// Masked addresses by item.
    masked: BTreeMap<String, MaskedLink>,
    /// Items with an own icon, with its revision.
    own_icons: BTreeMap<String, Option<String>>,
    /// Automatic icons from the server.
    automatic_icons: bool,
    limits: Option<Limits>,
    /// The extras key isn't the one this device took before (someone started
    /// over): the page warns until [`uwu_extras_key_seen`].
    extras_key_changed: bool,
}

fn status_of(state: &VaultState) -> UwuStatus {
    let Ok((id, _)) = state.active_account() else {
        return UwuStatus::default();
    };
    let guard = state.unlocked.read();
    let Some(u) = guard.get(&id) else {
        return UwuStatus::default();
    };
    let organizations = u.vault.organizations.clone();
    let Some(info) = &u.info else {
        return UwuStatus {
            organizations,
            ..UwuStatus::default()
        };
    };
    let travel = &u.uwu.travel;
    UwuStatus {
        uwu: true,
        features: info.offered(),
        travel: Travel {
            enabled: u.uwu.travelling(),
            hidden_count: u.extras_cache.travel_hidden.or_else(|| {
                travel
                    .as_ref()
                    .and_then(|t| t.get("hiddenCount"))
                    .and_then(Value::as_u64)
                    .map(|n| n as u32)
            }),
        },
        unseen: u.uwu.unseen.clone(),
        organizations,
        send_domains: info.send_domains.clone(),
        reminders: u
            .uwu
            .reminders
            .iter()
            .filter_map(|r| {
                let id = r.get("cipherId")?.as_str()?.to_string();
                Some((
                    id,
                    ReminderMark {
                        due: r.get("due").and_then(Value::as_str).map(str::to_string),
                        every_months: r
                            .get("everyMonths")
                            .and_then(Value::as_u64)
                            .map(|n| n as u32),
                        is_due: r.get("isDue").and_then(Value::as_bool).unwrap_or(false),
                    },
                ))
            })
            .collect(),
        masked: if info.has("masked-addresses") {
            u.uwu.masked_links.clone().into_iter().collect()
        } else {
            BTreeMap::new()
        },
        own_icons: u
            .uwu
            .icons
            .iter()
            .map(|(id, icon)| (id.clone(), icon.revision_date.clone()))
            .collect(),
        automatic_icons: info.has("icons")
            && info.icons.as_ref().is_some_and(|icons| icons.automatic),
        limits: info.limits.clone(),
        extras_key_changed: u.extras_cache.key_changed.is_some(),
    }
}

#[tauri::command]
pub(crate) fn uwu_status(state: State<'_, VaultState>) -> UwuStatus {
    status_of(&state)
}

/// Travel mode as the server has it right now, with how many items it hides.
#[tauri::command]
pub(crate) async fn uwu_travel(app: AppHandle, state: State<'_, VaultState>) -> Result<Travel> {
    need(&state, "travel-mode")?;
    let ctx = ctx(&state).await?;
    let travel = ctx.client.travel(&ctx.token).await.map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        u.extras_cache.travel_hidden = Some(travel.hidden_count);
        Ok(())
    })?;
    changed(&app);
    Ok(Travel {
        enabled: travel.enabled,
        hidden_count: Some(travel.hidden_count),
    })
}

/// Places in the web vault the app links to. The page names one; the address
/// is made here.
#[tauri::command]
pub(crate) fn open_web_vault_at(
    app: AppHandle,
    state: State<'_, VaultState>,
    place: String,
    id: Option<String>,
) -> Result<()> {
    let (account_id, account) = state.active_account()?;
    let web = account.server.web();
    let path = match place.as_str() {
        "organization" => {
            let org = id.unwrap_or_default();
            let known = with(&state, &account_id, |u| {
                Ok(u.vault.organizations.iter().any(|o| o.id == org))
            })?;
            if !known {
                return Err(Failure::new("invalid", "No such organisation."));
            }
            format!("#/organizations/{}", uwulock_bitwarden::uwu::uwu_path(&org))
        }
        "masked" => "#/settings/masked".into(),
        "travel" => "#/settings/travel".into(),
        "keys" => "#/settings".into(),
        "file-requests" => "#/file-requests".into(),
        _ => return Err(Failure::new("invalid", format!("unknown place {place}"))),
    };
    app.opener()
        .open_url(format!("{web}/{path}"), None::<&str>)
        .map_err(|e| Failure::new("io", format!("Couldn't open the browser: {e}")))
}

// ── Icons (§7) ─────────────────────────────────────────────

/// The first address of a login, as it is stored.
fn first_uri(item: &Item) -> Option<String> {
    item.login
        .as_ref()?
        .uris
        .iter()
        .map(|u| u.uri.to_string())
        .find(|u| !u.trim().is_empty())
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", B64.encode(png))
}

/// Icons for these items, as `data:` URLs: the own icon, else the server's
/// automatic one, else none (the page draws its tile). Fetched by Rust, never
/// by the page, and kept for the rest of the unlock.
#[tauri::command]
pub(crate) async fn item_icons(
    app: AppHandle,
    state: State<'_, VaultState>,
    ids: Vec<String>,
    automatic: Option<bool>,
) -> Result<HashMap<String, String>> {
    // The person may not want the server to learn which sites are in the vault.
    let automatic = automatic.unwrap_or(true);
    let (account_id, account) = state.active_account()?;

    // What is known here already, and what has to be asked for.
    struct Plan {
        own_wanted: Vec<String>,
        personal_wanted: bool,
        hosts: HashMap<String, Vec<String>>,
        icons_url: Option<String>,
    }
    let plan = with(&state, &account_id, |u| {
        let Some(info) = &u.info else {
            return Ok(None);
        };
        let own_on = info.has("own-icons");
        let auto_on = automatic
            && info.has("icons")
            && info.icons.as_ref().is_some_and(|icons| icons.automatic);
        let icons_url = info
            .icons
            .as_ref()
            .and_then(|i| i.url.clone())
            .unwrap_or_else(|| format!("{}/icons", account.server.web()));
        let mut plan = Plan {
            own_wanted: Vec::new(),
            personal_wanted: false,
            hosts: HashMap::new(),
            icons_url: auto_on.then_some(icons_url),
        };
        for id in ids.iter().take(2000) {
            let Some(item) = u.vault.item(id) else {
                continue;
            };
            if own_on {
                if let Some(icon) = u.uwu.icons.get(id) {
                    let cached = u.extras_cache.own.get(id);
                    if cached.is_none_or(|(rev, _)| *rev != icon.revision_date) {
                        plan.own_wanted.push(id.clone());
                        plan.personal_wanted |= icon.key_type != "organization";
                    }
                    continue;
                }
            }
            if !auto_on {
                continue;
            }
            let Some(host) = first_uri(item).and_then(|uri| host_of(&uri)) else {
                continue;
            };
            if icons::for_server(&host) && !u.extras_cache.automatic.contains_key(&host) {
                plan.hosts.entry(host).or_default().push(id.clone());
            }
        }
        Ok(Some(plan))
    })?;
    let Some(plan) = plan else {
        return Ok(HashMap::new());
    };

    if !plan.own_wanted.is_empty() || !plan.hosts.is_empty() {
        let ctx = ctx(&state).await?;
        if !plan.own_wanted.is_empty() {
            fetch_own_icons(&app, &state, &ctx, &plan.own_wanted, plan.personal_wanted).await;
        }
        if let Some(url) = plan.icons_url.filter(|_| !plan.hosts.is_empty()) {
            fetch_automatic_icons(&state, &ctx, &url, plan.hosts.into_keys().collect()).await;
        }
    }

    with(&state, &account_id, |u| {
        let mut out = HashMap::new();
        for id in &ids {
            let own = u
                .uwu
                .icons
                .get(id)
                .and(u.extras_cache.own.get(id))
                .and_then(|(_, png)| png.clone());
            let icon = own.or_else(|| {
                if !automatic {
                    return None;
                }
                let host = u
                    .vault
                    .item(id)
                    .and_then(first_uri)
                    .and_then(|x| host_of(&x))?;
                u.extras_cache.automatic.get(&host).cloned().flatten()
            });
            if let Some(png) = icon {
                out.insert(id.clone(), data_url(&png));
            }
        }
        Ok(out)
    })
}

async fn fetch_own_icons(
    app: &AppHandle,
    state: &VaultState,
    ctx: &Ctx,
    ids: &[String],
    personal: bool,
) {
    // Without the extras key, organisation icons still open.
    let extras = if personal {
        match extras_key(app, state, ctx).await {
            Ok(key) => Some(key),
            Err(error) => {
                tracing::warn!(kind = error.kind(), "own icons: no extras key");
                None
            }
        }
    } else {
        None
    };
    let icons = match ctx.client.own_icons(&ctx.token, ids).await {
        Ok(icons) => icons,
        Err(error) => {
            tracing::warn!(%error, "own icons didn't come");
            return;
        }
    };
    let _ = with(state, &ctx.account_id, |u| {
        let mut answered = HashSet::new();
        for icon in icons {
            answered.insert(icon.cipher_id.clone());
            let key = match icon.key_type.as_str() {
                "organization" => u
                    .vault
                    .item(&icon.cipher_id)
                    .and_then(|item| item.organization_id.clone())
                    .and_then(|org| u.vault.outer_key(Some(&org), &u.user_key).ok().cloned()),
                _ => extras.clone(),
            };
            let png = key
                .zip(icon.data.as_deref())
                .and_then(|(key, data)| open_icon(data, &key).ok())
                .map(|png| Arc::new(png.to_vec()));
            let revision = u
                .uwu
                .icons
                .get(&icon.cipher_id)
                .and_then(|i| i.revision_date.clone())
                .or(icon.revision_date);
            u.extras_cache.own.insert(icon.cipher_id, (revision, png));
        }
        // Asked for, not there: nothing to show until the sync says otherwise.
        for id in ids.iter().filter(|id| !answered.contains(*id)) {
            let revision = u.uwu.icons.get(id).and_then(|i| i.revision_date.clone());
            u.extras_cache.own.insert(id.clone(), (revision, None));
        }
        Ok(())
    });
}

async fn fetch_automatic_icons(state: &VaultState, ctx: &Ctx, url: &str, hosts: Vec<String>) {
    let client = Arc::new(ctx.client.clone());
    let limit = Arc::new(tokio::sync::Semaphore::new(6));
    let mut tasks = tokio::task::JoinSet::new();
    for host in hosts.into_iter().take(500) {
        let (client, limit, url) = (client.clone(), limit.clone(), url.to_string());
        tasks.spawn(async move {
            let _permit = limit.acquire_owned().await;
            let icon = client.automatic_icon(&url, &host).await;
            (host, icon)
        });
    }
    let mut found = Vec::new();
    while let Some(done) = tasks.join_next().await {
        match done {
            Ok((host, Ok(icon))) => found.push((host, icon.map(Arc::new))),
            // A network error isn't "no icon": asked again next time.
            Ok((_, Err(error))) => tracing::debug!(%error, "automatic icon"),
            Err(_) => {}
        }
    }
    let _ = with(state, &ctx.account_id, |u| {
        u.extras_cache.automatic.extend(found);
        Ok(())
    });
}

/// Seals a PNG as the item's own icon and stores it: under the extras key for
/// a personal item, the organisation's key for an organisation's.
async fn store_icon(app: &AppHandle, state: &VaultState, id: &str, png: Vec<u8>) -> Result<()> {
    let ctx = ctx(state).await?;
    let item = prepare(state, &ctx.account_id, id)?;
    let (key, key_type) = match &item.organization_id {
        Some(org) => (
            with(state, &ctx.account_id, |u| {
                Ok(u.vault.outer_key(Some(org), &u.user_key)?.clone())
            })?,
            "organization",
        ),
        None => (extras_key(app, state, &ctx).await?, "extras"),
    };
    let data = seal_icon(&png, &key)?;
    let stored = ctx
        .client
        .put_own_icon(&ctx.token, id, &data, key_type)
        .await
        .map_err(uwu_failure)?;
    with(state, &ctx.account_id, |u| {
        u.uwu.icons.insert(
            id.to_string(),
            IconRef {
                revision_date: stored.revision_date.clone(),
                key_type: key_type.into(),
            },
        );
        u.extras_cache
            .own
            .insert(id.to_string(), (stored.revision_date, Some(Arc::new(png))));
        Ok(())
    })?;
    changed(app);
    Ok(())
}

/// An own icon from the page: a PNG it already cropped and scaled, which is
/// read and written again here (at most 128 pixels, nothing but the image).
#[tauri::command]
pub(crate) async fn set_own_icon(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    png: String,
) -> Result<()> {
    state.touch();
    need(&state, "own-icons")?;
    let bytes = B64
        .decode(png.trim().trim_start_matches("data:image/png;base64,"))
        .map_err(|_| Failure::new("invalid", "The icon didn't arrive as PNG."))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(Failure::new("too-large", "The icon is too large."));
    }
    let png = icons::to_png(&bytes)?;
    store_icon(&app, &state, &id, png).await
}

/// The icon of a device on the local network (a NAS, a router), fetched from
/// the device by this computer and stored as the item's own icon.
#[tauri::command]
pub(crate) async fn fetch_device_icon(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    need(&state, "own-icons")?;
    let (account_id, _) = state.active_account()?;
    let uri = with(&state, &account_id, |u| {
        let item = u
            .vault
            .item(&id)
            .ok_or_else(|| Failure::new("not-found", "This item isn't in the vault any more."))?;
        item.login
            .iter()
            .flat_map(|l| l.uris.iter())
            .map(|u| u.uri.to_string())
            .find(|uri| host_of(uri).is_some_and(|h| icons::is_local_host(&h)))
            .ok_or_else(|| Failure::new("not-local", "This item has no local address."))
    })?;
    let png = icons::device_icon(&uri)
        .await
        .map_err(|e| Failure::new("device-icon", format!("The device gave no icon: {e}")))?;
    store_icon(&app, &state, &id, png).await
}

#[tauri::command]
pub(crate) async fn delete_own_icon(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    need(&state, "own-icons")?;
    let ctx = ctx(&state).await?;
    prepare(&state, &ctx.account_id, &id)?;
    ctx.client
        .delete_own_icon(&ctx.token, &id)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        u.uwu.icons.remove(&id);
        u.extras_cache.own.remove(&id);
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

// ── Entry versions (§8) ────────────────────────────────────

/// One value that differs between a version and the item as it is now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// The value's name, as `reveal_field` knows it: `password`, `uri:0`, …
    field: String,
    /// A custom field's own name.
    label: Option<String>,
    /// `changed`, `added` (not in the version, there now) or `removed`.
    kind: &'static str,
    /// Its value is revealed on request only.
    secret: bool,
    /// The version's value and the current one; `None` for secrets.
    before: Option<String>,
    after: Option<String>,
}

/// The names of an item's values, in the order they are shown, and whether
/// each is secret.
fn value_names(item: &Item) -> Vec<(String, bool)> {
    let mut names: Vec<(String, bool)> = vec![("name".into(), false)];
    if let Some(login) = &item.login {
        names.push(("username".into(), false));
        names.push(("password".into(), true));
        names.push(("totp".into(), true));
        for index in 0..login.uris.len() {
            names.push((format!("uri:{index}"), false));
        }
    }
    if item.card.is_some() {
        for (name, secret) in [
            ("card-name", false),
            ("card-brand", false),
            ("card-number", true),
            ("card-expiry", false),
            ("card-code", true),
        ] {
            names.push((name.into(), secret));
        }
    }
    if item.identity.is_some() {
        for (name, secret) in [
            ("title", false),
            ("firstName", false),
            ("middleName", false),
            ("lastName", false),
            ("username", false),
            ("company", false),
            ("email", false),
            ("phone", false),
            ("address1", false),
            ("address2", false),
            ("address3", false),
            ("postalCode", false),
            ("city", false),
            ("state", false),
            ("country", false),
            ("ssn", true),
            ("passportNumber", true),
            ("licenseNumber", true),
        ] {
            names.push((format!("identity:{name}"), secret));
        }
    }
    if item.ssh_key.is_some() {
        names.push(("ssh-private".into(), true));
        names.push(("ssh-public".into(), false));
        names.push(("ssh-fingerprint".into(), false));
    }
    names.push(("notes".into(), false));
    for (index, field) in item.fields.iter().enumerate() {
        let secret = !matches!(
            field.kind,
            uwulock_bitwarden::vault::FieldKind::Text
                | uwulock_bitwarden::vault::FieldKind::Boolean
        );
        names.push((format!("field:{index}"), secret));
    }
    names
}

/// One value of an item by name — the names of [`value_names`], the
/// authenticator key included (for comparing, never for sharing).
fn version_value(item: &Item, name: &str) -> Option<Zeroizing<String>> {
    let filled = |v: Option<&Zeroizing<String>>| v.filter(|v| !v.is_empty()).cloned();
    match name {
        "name" => filled(Some(&item.name)),
        "totp" => filled(item.login.as_ref().and_then(|l| l.totp.as_ref())),
        "card-brand" => filled(item.card.as_ref().and_then(|c| c.brand.as_ref())),
        _ => shareable_value(item, name),
    }
}

/// What differs between a version (`old`) and the item now (`new`).
fn compare(old: &Item, new: &Item) -> Vec<Change> {
    // A value is secret when it is on either side: custom fields compare by
    // index, so a deleted hidden field moves a text field into its place, and
    // the hidden value of the version must not show in clear because of that.
    let mut names = value_names(new);
    for (name, secret) in value_names(old) {
        match names.iter_mut().find(|(n, _)| *n == name) {
            Some((_, known)) => *known |= secret,
            None => names.push((name, secret)),
        }
    }
    let label = |item: &Item, name: &str| {
        let index: usize = name.strip_prefix("field:")?.parse().ok()?;
        item.fields
            .get(index)?
            .name
            .as_ref()
            .map(|n| n.to_string())
            .filter(|n| !n.is_empty())
    };
    names
        .into_iter()
        .filter_map(|(name, secret)| {
            let before = version_value(old, &name);
            let after = version_value(new, &name);
            let kind = match (&before, &after) {
                (None, None) => return None,
                (Some(a), Some(b)) if a.as_str() == b.as_str() => return None,
                (Some(_), Some(_)) => "changed",
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
            };
            let shown = |v: Option<Zeroizing<String>>| {
                (!secret).then(|| v.map(|v| v.to_string())).flatten()
            };
            Some(Change {
                label: label(new, &name).or_else(|| label(old, &name)),
                field: name,
                kind,
                secret,
                before: shown(before),
                after: shown(after),
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionView {
    id: String,
    revision_date: Option<String>,
    replaced_date: Option<String>,
    size: u64,
    /// Part of it didn't decrypt.
    broken: bool,
    changes: Vec<Change>,
}

/// The item's versions, newest first, each compared with the item as it is.
#[tauri::command]
pub(crate) async fn item_versions(
    state: State<'_, VaultState>,
    id: String,
) -> Result<Vec<VersionView>> {
    need(&state, "versions")?;
    let ctx = ctx(&state).await?;
    let current = prepare(&state, &ctx.account_id, &id)?;
    let versions = ctx
        .client
        .versions(&ctx.token, &id)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        u.extras_cache.versions.retain(|_, (item, _)| *item != id);
        let mut out = Vec::new();
        for version in versions {
            // A version has the item's organisation, whatever it says itself.
            let mut cipher = match parse_cipher(version.cipher.clone(), Some(&id)) {
                Ok(cipher) => cipher,
                Err(error) => {
                    tracing::warn!(%error, "a version doesn't read");
                    continue;
                }
            };
            cipher.organization_id = current.organization_id.clone();
            let Some(opened) = u.vault.open_cipher(&cipher, &u.user_key)? else {
                continue;
            };
            out.push(VersionView {
                changes: compare(&opened, &current),
                broken: opened.broken,
                id: version.id.clone(),
                revision_date: version.revision_date,
                replaced_date: version.replaced_date,
                size: version.size,
            });
            u.extras_cache
                .versions
                .insert(version.id, (id.clone(), opened));
        }
        Ok(out)
    })
}

/// One value of a version — or, with no `version_id`, of the item as it is
/// now, named the same way (the authenticator key is its key here, not the
/// current code) — for the eye in the comparison.
#[tauri::command]
pub(crate) fn reveal_version_field(
    state: State<'_, VaultState>,
    id: String,
    version_id: Option<String>,
    field: String,
) -> Result<String> {
    state.touch();
    let (account_id, _) = state.active_account()?;
    // The item's own re-prompt covers its versions.
    let current = prepare(&state, &account_id, &id)?;
    let missing = || Failure::new("not-found", "This version has no such value.");
    let Some(version_id) = version_id else {
        return version_value(&current, &field)
            .map(|v| v.to_string())
            .ok_or_else(missing);
    };
    with(&state, &account_id, |u| {
        let (item_id, version) = u
            .extras_cache
            .versions
            .get(&version_id)
            .ok_or_else(|| Failure::new("not-found", "This version isn't open."))?;
        if *item_id != id {
            return Err(Failure::new("not-found", "This version isn't open."));
        }
        version_value(version, &field)
            .map(|v| v.to_string())
            .ok_or_else(missing)
    })
}

/// Brings a version back. The server refuses when the item changed since the
/// last sync; then the app syncs and says so.
#[tauri::command]
pub(crate) async fn restore_version(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    version_id: String,
) -> Result<()> {
    state.touch();
    need(&state, "versions")?;
    let ctx = ctx(&state).await?;
    let item = prepare(&state, &ctx.account_id, &id)?;
    match ctx
        .client
        .restore_version(&ctx.token, &id, &version_id, item.revision_date.as_deref())
        .await
    {
        Ok(cipher) => {
            if entry_id(&cipher).is_some() {
                patch_cache(&state, &ctx.account_id, Patch::Cipher(cipher))?;
            } else {
                sync_account(&app, &ctx.account_id).await?;
            }
            vault_changed(&app);
            Ok(())
        }
        Err(error) if error.code() == Some("conflict") || error.status() == Some(409) => {
            if let Err(failure) = sync_account(&app, &ctx.account_id).await {
                tracing::warn!(kind = failure.kind(), "sync after a conflict failed");
            }
            Err(Failure::new(
                "version-conflict",
                "The item changed elsewhere since the last sync.",
            ))
        }
        Err(error) => Err(uwu_failure(error)),
    }
}

/// One version, or all of them with no `version_id`.
#[tauri::command]
pub(crate) async fn delete_versions(
    state: State<'_, VaultState>,
    id: String,
    version_id: Option<String>,
) -> Result<()> {
    state.touch();
    need(&state, "versions")?;
    let ctx = ctx(&state).await?;
    prepare(&state, &ctx.account_id, &id)?;
    ctx.client
        .delete_versions(&ctx.token, &id, version_id.as_deref())
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        match &version_id {
            Some(version) => {
                u.extras_cache.versions.remove(version);
            }
            None => u.extras_cache.versions.retain(|_, (item, _)| *item != id),
        }
        Ok(())
    })
}

// ── Reminders (§10) ────────────────────────────────────────

fn is_day(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    matches!(parts.as_slice(), [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && [y, m, d].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
            && (1..=12).contains(&m.parse::<u32>().unwrap_or(0))
            && (1..=31).contains(&d.parse::<u32>().unwrap_or(0)))
}

/// Remind to renew the password: on a date, every so many months, or both.
#[tauri::command]
pub(crate) async fn set_reminder(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    due: Option<String>,
    every_months: Option<u32>,
) -> Result<()> {
    state.touch();
    need(&state, "reminders")?;
    let due = due.filter(|d| !d.trim().is_empty());
    if due.is_none() && every_months.is_none() {
        return Err(Failure::new(
            "invalid",
            "A reminder needs a date or an interval.",
        ));
    }
    if due.as_deref().is_some_and(|d| !is_day(d)) {
        return Err(Failure::new("invalid", "A date is YYYY-MM-DD."));
    }
    if every_months.is_some_and(|m| !(1..=60).contains(&m)) {
        return Err(Failure::new("invalid", "Every 1 to 60 months."));
    }
    let ctx = ctx(&state).await?;
    prepare(&state, &ctx.account_id, &id)?;
    let reminder = ctx
        .client
        .set_reminder(&ctx.token, &id, due.as_deref(), every_months)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        let mut value = serde_json::to_value(&reminder).unwrap_or(Value::Null);
        value["object"] = json!("reminder");
        u.uwu
            .reminders
            .retain(|r| r.get("cipherId").and_then(Value::as_str) != Some(id.as_str()));
        u.uwu.reminders.push(value);
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_reminder(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    need(&state, "reminders")?;
    let ctx = ctx(&state).await?;
    ctx.client
        .delete_reminder(&ctx.token, &id)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        u.uwu
            .reminders
            .retain(|r| r.get("cipherId").and_then(Value::as_str) != Some(id.as_str()));
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

// ── File requests (§11) ────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRequestView {
    id: String,
    /// The owner's own name for it; `None` when it didn't open (after the
    /// extras key was reset, requests lose their labels).
    label: Option<String>,
    title: Option<String>,
    note: Option<String>,
    owner: Option<String>,
    link: Option<String>,
    password_set: bool,
    expiration_date: Option<String>,
    deletion_date: Option<String>,
    max_submissions: Option<u32>,
    submission_count: u32,
    max_files: u32,
    max_file_bytes: Option<u64>,
    text_allowed: bool,
    send_domain_id: Option<String>,
    disabled: bool,
    unseen: u32,
    bytes: u64,
    /// The details encrypt for a key that isn't the account's own: no link.
    foreign_key: bool,
}

/// Where links point: the main host, and the send domains.
fn link_bases(state: &VaultState, account_id: &str) -> Result<(String, Vec<SendDomain>)> {
    let (_, account) = state.active_account()?;
    with(state, account_id, |u| {
        let info: Option<&Info> = u.info.as_ref();
        let main = info
            .and_then(|i| i.public_url.clone())
            .unwrap_or_else(|| account.server.web());
        let domains = info.map(|i| i.send_domains.clone()).unwrap_or_default();
        Ok((main, domains))
    })
}

fn request_link(
    main: &str,
    domains: &[SendDomain],
    request: &FileRequest,
    secret: &LinkSecret,
) -> String {
    match request
        .send_domain_id
        .as_ref()
        .and_then(|id| domains.iter().find(|d| d.id == *id))
    {
        Some(domain) => file_request::link(&domain.url, &request.access_id, secret, true),
        None => file_request::link(main, &request.access_id, secret, false),
    }
}

/// A file request as the page shows it. Its details (`publicInfo`) have to
/// name `own`, the account's public key: details naming another were made by
/// someone else who knew the link secret, and the uploads to that link would
/// be theirs to read. Such a request is `foreignKey` and has no link to hand
/// out; saving it with a new link makes it the account's again.
fn request_view(
    request: &FileRequest,
    extras: &SymmetricKey,
    own: Option<&uwulock_bitwarden::crypto::PublicKey>,
    main: &str,
    domains: &[SendDomain],
) -> FileRequestView {
    let label = request
        .name
        .as_deref()
        .and_then(|n| file_request::open_label(n, extras).ok())
        .map(|n| n.to_string());
    let secret = request
        .link_secret
        .as_deref()
        .and_then(|s| LinkSecret::open(s, extras).ok());
    let info = secret
        .as_ref()
        .and_then(|secret| PublicInfo::open(request.public_info.as_deref()?, secret).ok());
    let foreign_key = info
        .as_ref()
        .is_some_and(|info| !own.is_some_and(|own| info.is_for(own)));
    let secret = secret.filter(|_| !foreign_key);
    FileRequestView {
        id: request.id.clone(),
        label,
        title: info.as_ref().map(|i| i.title.clone()),
        note: info.as_ref().and_then(|i| i.note.clone()),
        owner: info.as_ref().and_then(|i| i.owner.clone()),
        link: secret
            .as_ref()
            .map(|secret| request_link(main, domains, request, secret)),
        password_set: request.password_set,
        expiration_date: request.expiration_date.clone(),
        deletion_date: request.deletion_date.clone(),
        max_submissions: request.max_submissions,
        submission_count: request.submission_count,
        max_files: request.max_files,
        max_file_bytes: request.max_file_bytes,
        text_allowed: request.text_allowed,
        send_domain_id: request.send_domain_id.clone(),
        disabled: request.disabled,
        unseen: request.unseen,
        bytes: request.bytes,
        foreign_key,
    }
}

#[tauri::command]
pub(crate) async fn file_requests(
    app: AppHandle,
    state: State<'_, VaultState>,
) -> Result<Vec<FileRequestView>> {
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    let extras = extras_key(&app, &state, &ctx).await?;
    let own = public_key(&state, &ctx.account_id).ok();
    let (main, domains) = link_bases(&state, &ctx.account_id)?;
    let requests = ctx
        .client
        .file_requests(&ctx.token)
        .await
        .map_err(uwu_failure)?;
    Ok(requests
        .iter()
        .map(|r| request_view(r, &extras, own.as_ref(), &main, &domains))
        .collect())
}

/// A file request as the page fills it in.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRequestInput {
    label: String,
    title: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    /// Days from now until it expires; `None` on a change keeps the date.
    #[serde(default)]
    expires_in_days: Option<u32>,
    #[serde(default)]
    max_submissions: Option<u32>,
    max_files: u32,
    /// MiB per file; `None` is the server's limit.
    #[serde(default)]
    max_file_mib: Option<u64>,
    text_allowed: bool,
    /// Empty: none (or, on a change, keep the one there is).
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    send_domain_id: Option<String>,
    #[serde(default)]
    disabled: bool,
}

/// The largest file a request takes: the person's own limit, else the one it
/// had, else the server's. The server wants a number, never `null`.
fn file_limit(mib: Option<u64>, had: Option<u64>, info: Option<&Info>) -> u64 {
    mib.map(|m| m.saturating_mul(1024 * 1024))
        .or(had)
        .or_else(|| {
            info.and_then(|i| i.limits.as_ref())
                .and_then(|l| l.max_file_bytes)
        })
        // A server that names no limit: Bitwarden's own for a file.
        .unwrap_or(500 * 1024 * 1024)
}

/// Checks the input against the contract and the server's limits.
fn checked_input(input: &FileRequestInput, info: Option<&Info>) -> Result<()> {
    let limits = info.and_then(|i| i.limits.clone()).unwrap_or_default();
    let invalid = |m: &str| Err(Failure::new("invalid", m));
    if input.title.trim().is_empty() {
        return invalid("A file request needs a title.");
    }
    let max_days = limits.file_request_max_days.unwrap_or(90);
    if input
        .expires_in_days
        .is_some_and(|d| d == 0 || d > max_days)
    {
        return invalid("That expiry is outside what the server allows.");
    }
    if input
        .max_submissions
        .is_some_and(|n| !(1..=100).contains(&n))
    {
        return invalid("Between 1 and 100 uploads.");
    }
    let max_files = limits.file_request_max_files.unwrap_or(20).min(20);
    if input.max_files > max_files {
        return invalid("More files than the server allows.");
    }
    if input.max_files == 0 && !input.text_allowed {
        return invalid("Allow files, a message or both.");
    }
    if let (Some(mib), Some(max)) = (input.max_file_mib, limits.max_file_bytes) {
        if mib.saturating_mul(1024 * 1024) > max {
            return invalid("Files that large are more than the server allows.");
        }
    }
    if input.note.as_deref().is_some_and(|n| n.len() > 2000) || input.title.len() > 200 {
        return invalid("Title or note too long.");
    }
    if let Some(domain) = &input.send_domain_id {
        if !info.is_some_and(|i| i.send_domains.iter().any(|d| d.id == *domain)) {
            return invalid("No such send domain.");
        }
    }
    Ok(())
}

fn in_days(days: u32) -> String {
    iso_from_unix(now() + u64::from(days) * 86_400, 0)
}

/// What the request's owner's public key is, from the account's own key
/// pair: the uploader's page encrypts for it.
fn public_key(
    state: &VaultState,
    account_id: &str,
) -> Result<uwulock_bitwarden::crypto::PublicKey> {
    with(state, account_id, |u| {
        u.vault
            .private_key()
            .map(|p| p.public())
            .ok_or_else(|| Failure::new("no-key-pair", "This account has no key pair."))
    })
}

#[tauri::command]
pub(crate) async fn create_file_request(
    app: AppHandle,
    state: State<'_, VaultState>,
    input: FileRequestInput,
) -> Result<FileRequestView> {
    state.touch();
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    let info = with(&state, &ctx.account_id, |u| Ok(u.info.clone()))?;
    checked_input(&input, info.as_ref())?;
    let extras = extras_key(&app, &state, &ctx).await?;
    let public = public_key(&state, &ctx.account_id)?;
    let secret = LinkSecret::generate();
    let public_info = PublicInfo::new(
        input.title.trim(),
        input.note.as_deref().map(str::trim),
        input.owner.as_deref().map(str::trim),
        &public,
    )?;
    let password = input.password.as_deref().filter(|p| !p.is_empty());
    let label = if input.label.trim().is_empty() {
        input.title.trim()
    } else {
        input.label.trim()
    };
    let body = FileRequestBody {
        name: file_request::seal_label(label, &extras),
        link_secret: secret.seal(&extras),
        public_info: public_info.seal(&secret)?,
        password_hash: password.map(|p| secret.password_hash(p)),
        remove_password: false,
        expiration_date: in_days(input.expires_in_days.unwrap_or(7)),
        max_submissions: input.max_submissions,
        max_files: input.max_files,
        max_file_bytes: file_limit(input.max_file_mib, None, info.as_ref()),
        text_allowed: input.text_allowed,
        send_domain_id: input.send_domain_id.clone(),
        disabled: input.disabled,
    };
    let made = ctx
        .client
        .create_file_request(&ctx.token, &body)
        .await
        .map_err(uwu_failure)?;
    let (main, domains) = link_bases(&state, &ctx.account_id)?;
    Ok(request_view(&made, &extras, Some(&public), &main, &domains))
}

/// Changes a request. `new_link` makes a new secret, so every link handed out
/// so far stops working; a password has to be given again then, since the
/// uploader's page derives its hash from the secret.
#[tauri::command]
pub(crate) async fn update_file_request(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    input: FileRequestInput,
    new_link: bool,
    remove_password: bool,
) -> Result<FileRequestView> {
    state.touch();
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    let info = with(&state, &ctx.account_id, |u| Ok(u.info.clone()))?;
    checked_input(&input, info.as_ref())?;
    let extras = extras_key(&app, &state, &ctx).await?;
    let requests = ctx
        .client
        .file_requests(&ctx.token)
        .await
        .map_err(uwu_failure)?;
    let existing = requests
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| Failure::new("not-found", "This file request is gone."))?;
    let secret = if new_link {
        LinkSecret::generate()
    } else {
        existing
            .link_secret
            .as_deref()
            .and_then(|s| LinkSecret::open(s, &extras).ok())
            .ok_or_else(|| {
                Failure::new(
                    "new-link-needed",
                    "The link of this request doesn't open any more.",
                )
            })?
    };
    let public = public_key(&state, &ctx.account_id)?;
    // Someone else who knew the secret made the details: keeping that secret
    // would let them do it again.
    let foreign = !new_link
        && existing
            .public_info
            .as_deref()
            .and_then(|info| PublicInfo::open(info, &secret).ok())
            .is_some_and(|info| !info.is_for(&public));
    if foreign {
        return Err(Failure::new(
            "new-link-needed",
            "This request's link encrypts for a key that isn't yours; it needs a new link.",
        ));
    }
    let password = input.password.as_deref().filter(|p| !p.is_empty());
    if new_link && existing.password_set && password.is_none() && !remove_password {
        return Err(Failure::new(
            "password-again",
            "A new link needs the password again.",
        ));
    }
    let public_info = PublicInfo::new(
        input.title.trim(),
        input.note.as_deref().map(str::trim),
        input.owner.as_deref().map(str::trim),
        &public,
    )?;
    let label = if input.label.trim().is_empty() {
        input.title.trim()
    } else {
        input.label.trim()
    };
    let body = FileRequestBody {
        name: file_request::seal_label(label, &extras),
        link_secret: secret.seal(&extras),
        public_info: public_info.seal(&secret)?,
        password_hash: password.map(|p| secret.password_hash(p)),
        remove_password: remove_password && password.is_none(),
        expiration_date: match input.expires_in_days {
            Some(days) => in_days(days),
            None => existing
                .expiration_date
                .clone()
                .unwrap_or_else(|| in_days(7)),
        },
        max_submissions: input.max_submissions,
        max_files: input.max_files,
        max_file_bytes: file_limit(input.max_file_mib, existing.max_file_bytes, info.as_ref()),
        text_allowed: input.text_allowed,
        send_domain_id: input.send_domain_id.clone(),
        disabled: input.disabled,
    };
    let changed_request = ctx
        .client
        .update_file_request(&ctx.token, &id, &body)
        .await
        .map_err(uwu_failure)?;
    let (main, domains) = link_bases(&state, &ctx.account_id)?;
    Ok(request_view(
        &changed_request,
        &extras,
        Some(&public),
        &main,
        &domains,
    ))
}

#[tauri::command]
pub(crate) async fn delete_file_request(state: State<'_, VaultState>, id: String) -> Result<()> {
    state.touch();
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    ctx.client
        .delete_file_request(&ctx.token, &id)
        .await
        .map_err(uwu_failure)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionFileView {
    id: String,
    name: Option<String>,
    size: u64,
    /// A type that runs when opened: the page warns before saving it.
    risky: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionView {
    id: String,
    creation_date: Option<String>,
    seen: bool,
    text: Option<String>,
    /// As the uploader typed it: nobody checked it.
    sender_name: Option<String>,
    sender_email: Option<String>,
    files: Vec<SubmissionFileView>,
    /// It didn't open with this account's key.
    broken: bool,
}

fn private_key(
    state: &VaultState,
    account_id: &str,
) -> Result<uwulock_bitwarden::crypto::PrivateKey> {
    with(state, account_id, |u| {
        u.vault
            .private_key()
            .cloned()
            .ok_or_else(|| Failure::new("no-key-pair", "This account has no key pair."))
    })
}

fn submission_view(submission: &Submission, key: Option<&SubmissionKey>) -> SubmissionView {
    let open = |value: &Option<String>| {
        let key = key?;
        value
            .as_deref()
            .filter(|v| !v.is_empty())
            .and_then(|v| key.open_text(v).ok())
            .map(|t| t.to_string())
    };
    let sender = key.and_then(|key| key.open_sender(submission.sender.as_deref()?).ok());
    SubmissionView {
        id: submission.id.clone(),
        creation_date: submission.creation_date.clone(),
        seen: submission.seen,
        text: open(&submission.text),
        sender_name: sender.as_ref().and_then(|s| s.name.clone()),
        sender_email: sender.as_ref().and_then(|s| s.email.clone()),
        files: submission
            .files
            .iter()
            .map(|file| {
                let name = key
                    .and_then(|key| key.open_file(&sealed(file)).ok())
                    .map(|(name, _)| name.to_string());
                SubmissionFileView {
                    id: file.id.clone(),
                    risky: name
                        .as_deref()
                        .is_some_and(|n| runs_when_opened(&safe_file_name(n))),
                    name,
                    size: file.size,
                }
            })
            .collect(),
        broken: key.is_none(),
    }
}

fn sealed(file: &uwulock_bitwarden::uwu::SubmissionFile) -> SealedFile {
    SealedFile {
        file_name: file.file_name.clone(),
        key: file.key.clone(),
    }
}

#[tauri::command]
pub(crate) async fn file_request_submissions(
    state: State<'_, VaultState>,
    request_id: String,
) -> Result<Vec<SubmissionView>> {
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    let private = private_key(&state, &ctx.account_id)?;
    let submissions = ctx
        .client
        .submissions(&ctx.token, &request_id)
        .await
        .map_err(uwu_failure)?;
    Ok(submissions
        .iter()
        .map(|s| {
            let key = SubmissionKey::open(&s.wrapped_key, &private).ok();
            submission_view(s, key.as_ref())
        })
        .collect())
}

/// One submission, opened.
async fn open_submission(
    state: &VaultState,
    ctx: &Ctx,
    request_id: &str,
    submission_id: &str,
) -> Result<(Submission, SubmissionKey)> {
    let private = private_key(state, &ctx.account_id)?;
    let submission = ctx
        .client
        .submissions(&ctx.token, request_id)
        .await
        .map_err(uwu_failure)?
        .into_iter()
        .find(|s| s.id == submission_id)
        .ok_or_else(|| Failure::new("not-found", "This upload is gone."))?;
    let key = SubmissionKey::open(&submission.wrapped_key, &private)?;
    Ok((submission, key))
}

/// A name an uploader chose, safe as a file name here: no folders, no
/// control characters, nothing Windows reserves, not hidden, not too long.
pub(crate) fn safe_file_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut clean: String = last
        .chars()
        .map(|c| {
            // Bidirectional overrides would make `exe.pdf` look like `fdp.exe`.
            let invisible = matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}');
            if c.is_control() || invisible || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    clean = clean
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string();
    let stem = clean
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        clean.insert(0, '_');
    }
    if clean.is_empty() {
        clean = "upload".into();
    }
    // At most 120 characters, keeping the extension.
    if clean.chars().count() > 120 {
        let (stem, ext) = match clean.rsplit_once('.') {
            Some((s, e)) if e.len() <= 10 => (s.to_string(), format!(".{e}")),
            _ => (clean.clone(), String::new()),
        };
        clean = stem
            .chars()
            .take(120 - ext.chars().count())
            .collect::<String>()
            + &ext;
    }
    clean
}

/// `name`, or `name (2)`, … — whichever doesn't exist in `dir` yet.
#[cfg_attr(target_os = "android", allow(dead_code))]
fn free_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Whether a file of this name runs something when it is opened: programs,
/// scripts, installers, shortcuts, disk images and documents with macros.
/// Anyone can upload to a file request, so the page warns before saving one.
/// The list: Windows, then macOS, then Linux and scripts anywhere.
pub(crate) fn runs_when_opened(name: &str) -> bool {
    const RISKY: &str = "\
        exe com scr pif bat cmd msi msix msixbundle msp mst appx appxbundle appinstaller \
        application ps1 psm1 psd1 ps1xml vbs vbe js jse wsf wsh wsc hta cpl lnk url scf reg \
        inf dll sys ocx gadget chm hlp settingcontent-ms library-ms search-ms xll xlam docm \
        dotm xlsm xltm pptm potm ppsm one iso img vhd vhdx jar jnlp \
        app command tool pkg mpkg dmg terminal workflow scpt \
        sh bash zsh csh ksh run bin desktop appimage deb rpm flatpakref snap py pyw pl rb php";
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let ext = ext.trim().to_ascii_lowercase();
    RISKY.split_whitespace().any(|risky| risky == ext)
}

/// Marks a file that came from the internet as such, so the system treats it
/// like a download from a browser: `Zone.Identifier` (Internet zone) on
/// Windows — SmartScreen and Office's Protected View look at it — and
/// `com.apple.quarantine` on macOS (Gatekeeper). Linux has no such mark.
/// A failure (FAT32, a file system without streams or attributes) is logged:
/// the file is saved anyway, and the page warned before.
#[cfg(desktop)]
fn mark_downloaded(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        if let Err(error) = std::fs::write(&stream, "[ZoneTransfer]\r\nZoneId=3\r\n") {
            tracing::warn!(%error, "couldn't mark the file as downloaded");
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        // Flags 0081: downloaded, not yet opened; the agent that saved it.
        let value = format!("0081;{seconds:08x};UwULock;");
        let (Ok(file), Ok(name)) = (
            std::ffi::CString::new(path.as_os_str().as_bytes()),
            std::ffi::CString::new("com.apple.quarantine"),
        ) else {
            return;
        };
        // SAFETY: both are NUL-terminated, the value is `value.len()` bytes.
        let result = unsafe {
            libc::setxattr(
                file.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            tracing::warn!(%error, "couldn't mark the file as downloaded");
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = path;
}

/// Saves a submitted file, decrypted, into the Downloads folder (the page
/// asked first, and warned for a type that runs when opened: `allow_risky`
/// says it did). Marked as downloaded from the internet. Returns where it went.
#[tauri::command]
pub(crate) async fn save_submission_file(
    app: AppHandle,
    state: State<'_, VaultState>,
    request_id: String,
    submission_id: String,
    file_id: String,
    allow_risky: Option<bool>,
) -> Result<String> {
    state.touch();
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    let (submission, key) = open_submission(&state, &ctx, &request_id, &submission_id).await?;
    let file = submission
        .files
        .iter()
        .find(|f| f.id == file_id)
        .ok_or_else(|| Failure::new("not-found", "This file is gone."))?;
    let (name, file_key) = key.open_file(&sealed(file))?;
    let file_name = safe_file_name(&name);
    if runs_when_opened(&file_name) && allow_risky != Some(true) {
        return Err(Failure::new(
            "risky-file",
            "This type of file runs when it is opened; the page didn't warn.",
        ));
    }
    // No file is larger than the server takes, whatever it sends: its limit
    // (Bitwarden's 500 MiB when it names none), plus what encryption adds.
    let info = with(&state, &ctx.account_id, |u| Ok(u.info.clone()))?;
    let max = file_limit(None, None, info.as_ref()).saturating_add(1024);
    let encrypted = ctx
        .client
        .submission_file(&ctx.token, &request_id, &submission_id, &file_id, max)
        .await
        .map_err(uwu_failure)?;
    let contents = file_key.decrypt(&encrypted)?;
    let saved = save_download(&app, &file_name, contents.as_slice()).await?;
    tracing::info!("a file request's file saved");
    Ok(saved)
}

/// Where a saved file goes, and what the page shows of it: the Downloads
/// folder on a computer (marked as downloaded from the internet).
#[cfg(desktop)]
pub(crate) async fn save_download(
    app: &AppHandle,
    file_name: &str,
    contents: &[u8],
) -> Result<String> {
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| Failure::new("io", format!("No Downloads folder: {e}")))?;
    let path = free_path(&dir, file_name);
    std::fs::write(&path, contents)
        .map_err(|e| Failure::new("io", format!("Couldn't save the file: {e}")))?;
    mark_downloaded(&path);
    Ok(path.display().to_string())
}

/// [`save_download`] for a private key: on Linux and macOS readable by this
/// user only (`0600`, which `ssh` also insists on), from the moment it exists.
#[cfg(all(desktop, unix))]
pub(crate) async fn save_download_private(
    app: &AppHandle,
    file_name: &str,
    contents: &[u8],
) -> Result<String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| Failure::new("io", format!("No Downloads folder: {e}")))?;
    let path = free_path(&dir, file_name);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut file| file.write_all(contents))
        .map_err(|e| Failure::new("io", format!("Couldn't save the file: {e}")))?;
    mark_downloaded(&path);
    Ok(path.display().to_string())
}

/// Windows (the Downloads folder is the user's own) and the phones: as any
/// other download.
#[cfg(not(all(desktop, unix)))]
pub(crate) async fn save_download_private(
    app: &AppHandle,
    file_name: &str,
    contents: &[u8],
) -> Result<String> {
    save_download(app, file_name, contents).await
}

/// Android: through the app's cache into Downloads (the plugin, MediaStore).
#[cfg(target_os = "android")]
pub(crate) async fn save_download(
    app: &AppHandle,
    file_name: &str,
    contents: &[u8],
) -> Result<String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| Failure::new("io", format!("No cache folder: {e}")))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| Failure::new("io", format!("Couldn't save the file: {e}")))?;
    let path = dir.join(format!("download-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, contents)
        .map_err(|e| Failure::new("io", format!("Couldn't save the file: {e}")))?;
    let name = file_name.to_string();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let plugin = crate::phone::plugin().ok_or("The phone isn't ready yet.".to_string());
        let result = plugin.and_then(|plugin| {
            plugin
                .save_to_downloads(&path.display().to_string(), &name)
                .map_err(|e| e.to_string())
        });
        // The plugin deletes the cached copy; this catches a failure before it got there.
        let _ = std::fs::remove_file(&path);
        result
    })
    .await
    .map_err(|e| Failure::new("io", e.to_string()))?
    .map_err(|e| Failure::new("io", e))?;
    Ok(format!("Downloads/{saved}"))
}

/// iOS: UwULock's own folder, which the Files app shows ("On My iPhone").
#[cfg(target_os = "ios")]
pub(crate) async fn save_download(
    app: &AppHandle,
    file_name: &str,
    contents: &[u8],
) -> Result<String> {
    let dir = app
        .path()
        .document_dir()
        .map_err(|e| Failure::new("io", format!("No documents folder: {e}")))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| Failure::new("io", format!("Couldn't save the file: {e}")))?;
    let path = free_path(&dir, file_name);
    std::fs::write(&path, contents)
        .map_err(|e| Failure::new("io", format!("Couldn't save the file: {e}")))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    Ok(format!("UwULock/{}", name.unwrap_or_default()))
}

#[tauri::command]
pub(crate) async fn mark_submission_seen(
    app: AppHandle,
    state: State<'_, VaultState>,
    request_id: String,
    submission_id: String,
) -> Result<()> {
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    ctx.client
        .submission_seen(&ctx.token, &request_id, &submission_id)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        let unseen = &mut u.uwu.unseen.file_request_submissions;
        *unseen = unseen.saturating_sub(1);
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_submission(
    state: State<'_, VaultState>,
    request_id: String,
    submission_id: String,
) -> Result<()> {
    state.touch();
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    ctx.client
        .delete_submission(&ctx.token, &request_id, &submission_id)
        .await
        .map_err(uwu_failure)
}

/// "Take over as item": a secure note with the message and who sent it, the
/// files moved into its attachments (their keys wrapped again for the item),
/// and the submission gone. Returns the new item's id.
#[tauri::command]
pub(crate) async fn take_over_submission(
    app: AppHandle,
    state: State<'_, VaultState>,
    request_id: String,
    submission_id: String,
    name: String,
    sender_label: String,
) -> Result<String> {
    state.touch();
    need(&state, "file-requests")?;
    let ctx = ctx(&state).await?;
    let (submission, key) = open_submission(&state, &ctx, &request_id, &submission_id).await?;
    let view = submission_view(&submission, Some(&key));

    let mut notes = view.text.clone().unwrap_or_default();
    if view.sender_name.is_some() || view.sender_email.is_some() {
        let who = match (&view.sender_name, &view.sender_email) {
            (Some(n), Some(e)) => format!("{n} <{e}>"),
            (Some(n), None) => n.clone(),
            (None, Some(e)) => e.clone(),
            (None, None) => String::new(),
        };
        if !notes.is_empty() {
            notes.push_str("\n\n");
        }
        notes.push_str(&format!("{}: {who}", sender_label.trim()));
    }
    let mut item = Item::new(ItemKind::Note);
    item.name = Zeroizing::new(if name.trim().is_empty() {
        "File request".into()
    } else {
        name.trim().chars().take(200).collect()
    });
    item.notes =
        (!notes.is_empty()).then(|| Zeroizing::new(notes.chars().take(TEXT_MAX_CHARS).collect()));
    let request = with(&state, &ctx.account_id, |u| Ok(item.seal(&u.user_key)?))?;
    let created = ctx.client.create_cipher(&ctx.token, request, &[]).await?;
    let cipher_id = entry_id(&created)
        .map(str::to_string)
        .ok_or_else(|| Failure::new("server", "The server didn't say which item it made."))?;
    // The item's key: its own, if the server's answer gave it one, else the user key.
    let item_key = with(&state, &ctx.account_id, |u| {
        let cipher = parse_cipher(created.clone(), None)?;
        Ok(u.vault
            .open_cipher(&cipher, &u.user_key)?
            .and_then(|item| item.key)
            .unwrap_or_else(|| u.user_key.clone()))
    })?;

    let mut latest = created;
    for file in &submission.files {
        let (file_name, file_key) = key.open_file(&sealed(file))?;
        let for_item = file_key.for_item(&file_name, &item_key);
        latest = ctx
            .client
            .attach_submission_file(
                &ctx.token,
                &request_id,
                &submission_id,
                &file.id,
                &cipher_id,
                &for_item,
            )
            .await
            .map_err(uwu_failure)?;
    }
    if entry_id(&latest).is_some() {
        patch_cache(&state, &ctx.account_id, Patch::Cipher(latest))?;
    }
    if let Err(error) = ctx
        .client
        .delete_submission(&ctx.token, &request_id, &submission_id)
        .await
    {
        tracing::warn!(%error, "the taken-over submission stayed");
    }
    vault_changed(&app);
    Ok(cipher_id)
}

// ── Masked addresses (§13) ─────────────────────────────────

#[tauri::command]
pub(crate) async fn masked_connection(state: State<'_, VaultState>) -> Result<MaskedConnection> {
    need(&state, "masked-addresses")?;
    let ctx = ctx(&state).await?;
    ctx.client
        .masked_connection(&ctx.token)
        .await
        .map_err(uwu_failure)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskedView {
    #[serde(flatten)]
    address: MaskedAddress,
    /// The linked item's name, if it is in this vault.
    item_name: Option<String>,
}

#[tauri::command]
pub(crate) async fn masked_addresses(state: State<'_, VaultState>) -> Result<Vec<MaskedView>> {
    need(&state, "masked-addresses")?;
    let ctx = ctx(&state).await?;
    let addresses = ctx
        .client
        .masked_addresses(&ctx.token)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        Ok(addresses
            .into_iter()
            .filter(|a| a.state != "deleted")
            .map(|address| {
                // The link kept on the server, else the one in the sync.
                let cipher = address.cipher_id.clone().or_else(|| {
                    u.uwu
                        .masked_links
                        .iter()
                        .find(|(_, link)| link.id == address.id)
                        .map(|(cipher, _)| cipher.clone())
                });
                MaskedView {
                    item_name: cipher
                        .as_deref()
                        .and_then(|id| u.vault.item(id))
                        .map(|item| item.name.to_string()),
                    address: MaskedAddress {
                        cipher_id: cipher,
                        ..address
                    },
                }
            })
            .collect())
    })
}

fn remember_link(u: &mut Unlocked, address: &MaskedAddress) {
    u.uwu.masked_links.retain(|_, link| link.id != address.id);
    if let Some(cipher) = &address.cipher_id {
        u.uwu.masked_links.insert(
            cipher.clone(),
            MaskedLink {
                id: address.id.clone(),
                email: address.email.clone(),
            },
        );
    }
}

/// A new masked address, for a site (the item's first address when an item
/// is named and no site is given), linked to the item if it is saved.
#[tauri::command]
pub(crate) async fn create_masked_address(
    app: AppHandle,
    state: State<'_, VaultState>,
    for_domain: Option<String>,
    description: Option<String>,
    cipher_id: Option<String>,
) -> Result<MaskedAddress> {
    state.touch();
    need(&state, "masked-addresses")?;
    let ctx = ctx(&state).await?;
    let from_item = match &cipher_id {
        Some(id) => with(&state, &ctx.account_id, |u| {
            let item = u.vault.item(id).ok_or_else(|| {
                Failure::new("not-found", "This item isn't in the vault any more.")
            })?;
            Ok((first_uri(item), item.name.to_string()))
        })?,
        None => (None, String::new()),
    };
    let for_domain = for_domain
        .filter(|d| !d.trim().is_empty())
        .or(from_item.0)
        .map(|d| {
            let d = d.trim().to_string();
            if d.contains("://") {
                d
            } else {
                format!("https://{d}")
            }
        })
        .unwrap_or_default();
    let description = description
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(from_item.1);
    let address = ctx
        .client
        .create_masked_address(
            &ctx.token,
            &NewMaskedAddress {
                for_domain,
                description: description.chars().take(200).collect(),
                cipher_id,
                ..NewMaskedAddress::default()
            },
        )
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        remember_link(u, &address);
        Ok(())
    })?;
    changed(&app);
    Ok(address)
}

/// Switches an address on or off (`enabled`, `disabled`), or links it to an
/// item (`link` set: its id, or `null` to unlink).
#[tauri::command]
pub(crate) async fn update_masked_address(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
    address_state: Option<String>,
    link: Option<Option<String>>,
) -> Result<MaskedAddress> {
    state.touch();
    need(&state, "masked-addresses")?;
    let mut change = serde_json::Map::new();
    if let Some(next) = address_state {
        if !matches!(next.as_str(), "enabled" | "disabled") {
            return Err(Failure::new("invalid", "enabled or disabled"));
        }
        change.insert("state".into(), json!(next));
    }
    if let Some(cipher) = link {
        change.insert("cipherId".into(), json!(cipher));
    }
    let ctx = ctx(&state).await?;
    let address = ctx
        .client
        .update_masked_address(&ctx.token, &id, &Value::Object(change))
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        remember_link(u, &address);
        Ok(())
    })?;
    changed(&app);
    Ok(address)
}

#[tauri::command]
pub(crate) async fn delete_masked_address(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    need(&state, "masked-addresses")?;
    let ctx = ctx(&state).await?;
    ctx.client
        .delete_masked_address(&ctx.token, &id)
        .await
        .map_err(uwu_failure)?;
    with(&state, &ctx.account_id, |u| {
        u.uwu.masked_links.retain(|_, link| link.id != id);
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

// ── Sharing an item as a Send (§14) ────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOptions {
    /// "Only for these addresses" (Bitwarden's email verification).
    emails: bool,
    /// Send domains to choose from; empty: only the main host.
    domains: Vec<SendDomain>,
    /// The account's default send domain.
    default_domain_id: Option<String>,
}

#[tauri::command]
pub(crate) async fn send_options(state: State<'_, VaultState>) -> Result<SendOptions> {
    let (account_id, _) = state.active_account()?;
    let (uwu, emails, domains) = with(&state, &account_id, |u| {
        Ok((
            u.info.is_some(),
            u.has("send-emails"),
            if u.has("send-domains") {
                u.info
                    .as_ref()
                    .map(|i| i.send_domains.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
        ))
    })?;
    let default_domain_id = if uwu && !domains.is_empty() {
        let ctx = ctx(&state).await?;
        match ctx.client.uwu_account(&ctx.token).await {
            Ok(account) => account.send_domain_id,
            Err(error) => {
                tracing::warn!(%error, "the account's send domain didn't come");
                None
            }
        }
    } else {
        None
    };
    Ok(SendOptions {
        emails,
        domains,
        default_domain_id,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareInput {
    /// `(value name, label)`: which values, labelled in the person's language.
    fields: Vec<(String, String)>,
    /// 1 to 31.
    deletion_days: u32,
    #[serde(default)]
    max_access: Option<u32>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    emails: Vec<String>,
    /// Only with send domains; `None` is the main host.
    #[serde(default)]
    send_domain_id: Option<String>,
    /// The text stays hidden on the recipient's page until a click.
    #[serde(default)]
    hide_text: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSend {
    id: String,
    link: String,
    deletion_date: String,
}

/// Shares the chosen values of an item as a text Send — never its
/// authenticator key — and returns the link.
#[tauri::command]
pub(crate) async fn share_as_send(
    state: State<'_, VaultState>,
    id: String,
    input: ShareInput,
) -> Result<SharedSend> {
    state.touch();
    let ctx = ctx(&state).await?;
    let item = prepare(&state, &ctx.account_id, &id)?;
    if input.fields.is_empty() {
        return Err(Failure::new("invalid", "Choose something to share."));
    }
    if input
        .fields
        .iter()
        .any(|(name, _)| send_core::withheld(&item, name))
    {
        return Err(Failure::new(
            "hidden-by-org",
            "The organisation hides this item's passwords from you.",
        ));
    }
    if !(1..=31).contains(&input.deletion_days) {
        return Err(Failure::new("invalid", "A Send lasts 1 to 31 days."));
    }
    let (emails_on, domains, info) = with(&state, &ctx.account_id, |u| {
        Ok((
            u.has("send-emails"),
            if u.has("send-domains") {
                u.info
                    .as_ref()
                    .map(|i| i.send_domains.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            u.info.clone(),
        ))
    })?;
    let emails: Vec<String> = input
        .emails
        .iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    if !emails.is_empty() && !emails_on {
        return Err(Failure::new(
            "feature-off",
            "This server can't limit a Send to addresses.",
        ));
    }
    let domain = match &input.send_domain_id {
        Some(chosen) => Some(
            domains
                .iter()
                .find(|d| d.id == *chosen)
                .cloned()
                .ok_or_else(|| Failure::new("invalid", "No such send domain."))?,
        ),
        None => None,
    };
    let deletion_date = iso_from_unix(now() + u64::from(input.deletion_days) * 86_400, 0);
    let text = send_core::share_text(&item, &input.fields);
    let sealed = with(&state, &ctx.account_id, |u| {
        Ok(TextSend {
            name: item.name.to_string(),
            notes: None,
            text,
            hidden: input.hide_text,
            max_access_count: input.max_access.filter(|n| *n > 0),
            deletion_date: deletion_date.clone(),
            expiration_date: None,
            password: input
                .password
                .clone()
                .filter(|p| !p.is_empty())
                .map(Zeroizing::new),
            emails: emails.clone(),
            hide_email: false,
        }
        .seal(&u.user_key)?)
    })?;
    let answer = ctx.client.create_send(&ctx.token, &sealed.request).await?;
    let send_id = entry_id(&answer).unwrap_or_default().to_string();
    let access_id = answer
        .get("accessId")
        .or_else(|| answer.get("AccessId"))
        .and_then(Value::as_str)
        .ok_or_else(|| Failure::new("server", "The server didn't say how to reach the Send."))?
        .to_string();

    // The server gave it the account's default domain; another one is set now.
    let mut domain = domain;
    if info.is_some() && !domains.is_empty() {
        let default = ctx
            .client
            .uwu_account(&ctx.token)
            .await
            .ok()
            .and_then(|a| a.send_domain_id);
        let chosen = domain.as_ref().map(|d| d.id.clone());
        if chosen != default {
            if let Err(error) = ctx
                .client
                .set_send_domain(&ctx.token, &send_id, chosen.as_deref())
                .await
            {
                // The Send works on every host; only the link's host is off.
                tracing::warn!(%error, "the Send's domain didn't change");
                domain = default.and_then(|id| domains.iter().find(|d| d.id == id).cloned());
            }
        }
    }
    let link = match &domain {
        Some(domain) => send_core::link(&domain.url, &access_id, sealed.seed.as_ref(), true),
        None => {
            let (_, account) = state.active_account()?;
            let main = info
                .and_then(|i| i.public_url)
                .unwrap_or_else(|| account.server.web());
            send_core::link(&main, &access_id, sealed.seed.as_ref(), false)
        }
    };
    tracing::info!("an item shared as a Send");
    Ok(SharedSend {
        id: send_id,
        link,
        deletion_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_bitwarden::vault::{Field, FieldKind, Login, LoginUri};

    fn item() -> Item {
        let mut item = Item::new(ItemKind::Login);
        item.name = Zeroizing::new("Shop".into());
        item.login = Some(Login {
            username: Some(Zeroizing::new("nyu".into())),
            password: Some(Zeroizing::new("new password".into())),
            totp: None,
            uris: vec![LoginUri {
                uri: Zeroizing::new("https://shop.example.com".into()),
                match_kind: None,
                checksum: None,
            }],
            ..Login::default()
        });
        item.fields.push(Field {
            name: Some(Zeroizing::new("PIN".into())),
            value: Some(Zeroizing::new("1234".into())),
            kind: FieldKind::Hidden,
            linked_id: None,
        });
        item
    }

    #[test]
    fn a_comparison_names_what_changed_and_hides_secrets() {
        let new = item();
        let mut old = item();
        old.login.as_mut().unwrap().password = Some(Zeroizing::new("old password".into()));
        old.login.as_mut().unwrap().username = Some(Zeroizing::new("nyu-old".into()));
        old.login.as_mut().unwrap().totp = Some(Zeroizing::new("JBSWY3DPEHPK3PXP".into()));
        old.notes = Some(Zeroizing::new("gone".into()));
        old.fields.clear();

        let changes = compare(&old, &new);
        let by = |field: &str| changes.iter().find(|c| c.field == field).cloned();
        assert!(by("name").is_none() && by("uri:0").is_none());
        let username = by("username").unwrap();
        assert_eq!(
            (
                username.kind,
                username.before.as_deref(),
                username.after.as_deref()
            ),
            ("changed", Some("nyu-old"), Some("nyu"))
        );
        let password = by("password").unwrap();
        assert!(password.secret && password.before.is_none() && password.after.is_none());
        assert_eq!(password.kind, "changed");
        assert_eq!(by("totp").unwrap().kind, "removed");
        assert_eq!(by("notes").unwrap().kind, "removed");
        let pin = by("field:0").unwrap();
        assert_eq!(
            (pin.kind, pin.label.as_deref(), pin.secret),
            ("added", Some("PIN"), true)
        );
        assert!(compare(&new, &new).is_empty());
    }

    #[test]
    fn a_field_hidden_on_either_side_stays_hidden() {
        // The version had a hidden "PIN" first and a text "Note" second; the
        // PIN was deleted since, so the note is field 0 now.
        let mut old = item();
        old.fields.push(Field {
            name: Some(Zeroizing::new("Note".into())),
            value: Some(Zeroizing::new("plain".into())),
            kind: FieldKind::Text,
            linked_id: None,
        });
        let mut new = old.clone();
        new.fields.remove(0);

        let changes = compare(&old, &new);
        let first = changes.iter().find(|c| c.field == "field:0").unwrap();
        assert_eq!(first.kind, "changed");
        assert!(first.secret && first.before.is_none() && first.after.is_none());
        let second = changes.iter().find(|c| c.field == "field:1").unwrap();
        assert_eq!(second.kind, "removed");
        assert!(!second.secret && second.before.as_deref() == Some("plain"));

        // And the other way round: a text field that is hidden now.
        let changes = compare(&new, &old);
        let first = changes.iter().find(|c| c.field == "field:0").unwrap();
        assert!(first.secret && first.before.is_none() && first.after.is_none());
    }

    #[test]
    fn version_values_include_the_authenticator_key() {
        let mut version = item();
        version.login.as_mut().unwrap().totp = Some(Zeroizing::new("JBSWY3DPEHPK3PXP".into()));
        assert_eq!(
            version_value(&version, "totp").unwrap().as_str(),
            "JBSWY3DPEHPK3PXP"
        );
        assert_eq!(version_value(&version, "name").unwrap().as_str(), "Shop");
        assert!(version_value(&version, "history:0").is_none());
    }

    #[test]
    fn programs_and_scripts_are_risky() {
        for name in [
            "setup.exe",
            "Rechnung.PDF.js",
            "x.ps1",
            "a.lnk",
            "b.docm",
            "c.dmg",
            "d.sh",
        ] {
            assert!(runs_when_opened(name), "{name}");
        }
        for name in [
            "scan.pdf",
            "photo.jpeg",
            "notes.txt",
            "exe",
            "archive.zip",
            "table.xlsx",
        ] {
            assert!(!runs_when_opened(name), "{name}");
        }
        // The name is cleaned first: a trailing dot doesn't hide the type.
        assert!(runs_when_opened(&safe_file_name("setup.exe.")));
    }

    #[test]
    #[cfg(windows)]
    fn a_saved_file_is_in_the_internet_zone() {
        let path = std::env::temp_dir().join(format!("uwulock-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"x").unwrap();
        mark_downloaded(&path);
        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let mark = std::fs::read_to_string(stream);
        let _ = std::fs::remove_file(&path);
        assert!(mark.unwrap().contains("ZoneId=3"));
    }

    #[test]
    fn uploaded_file_names_stay_in_their_folder() {
        assert_eq!(safe_file_name("scan.pdf"), "scan.pdf");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("C:\\Users\\x\\evil.exe"), "evil.exe");
        assert_eq!(safe_file_name(".bashrc"), "bashrc");
        assert_eq!(safe_file_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(safe_file_name("CON.txt"), "_CON.txt");
        assert_eq!(safe_file_name("com1"), "_com1");
        assert_eq!(safe_file_name(""), "upload");
        assert_eq!(safe_file_name("..."), "upload");
        assert_eq!(safe_file_name("x\u{202e}fdp.exe"), "x_fdp.exe");
        let long = format!("{}.pdf", "a".repeat(300));
        let short = safe_file_name(&long);
        assert_eq!(short.chars().count(), 120);
        assert!(short.ends_with(".pdf"));

        let dir = std::env::temp_dir().join(format!("uwulock-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_path(&dir, "a.txt"), dir.join("a.txt"));
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(free_path(&dir, "a.txt"), dir.join("a (2).txt"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn days_are_checked() {
        assert!(is_day("2027-01-15"));
        assert!(!is_day("2027-1-15"));
        assert!(!is_day("2027-13-01"));
        assert!(!is_day("next week"));
    }

    #[test]
    fn file_request_input_within_the_limits() {
        let input = |max_files, text_allowed| FileRequestInput {
            label: String::new(),
            title: "Passport".into(),
            note: None,
            owner: None,
            expires_in_days: Some(7),
            max_submissions: Some(1),
            max_files,
            max_file_mib: Some(100),
            text_allowed,
            password: None,
            send_domain_id: None,
            disabled: false,
        };
        assert!(checked_input(&input(10, true), None).is_ok());
        assert!(checked_input(&input(0, false), None).is_err());
        assert!(checked_input(&input(21, true), None).is_err());
        let mut far = input(1, true);
        far.expires_in_days = Some(91);
        assert!(checked_input(&far, None).is_err());
        let mut elsewhere = input(1, true);
        elsewhere.send_domain_id = Some("d1".into());
        assert!(checked_input(&elsewhere, None).is_err());
    }

    #[test]
    fn a_file_request_always_names_a_file_limit() {
        let info: Info = serde_json::from_value(serde_json::json!({
            "name": "UwULock Server", "limits": { "maxFileBytes": 1000 }
        }))
        .unwrap();
        assert_eq!(file_limit(Some(2), Some(7), Some(&info)), 2 * 1024 * 1024);
        assert_eq!(file_limit(None, Some(7), Some(&info)), 7);
        assert_eq!(file_limit(None, None, Some(&info)), 1000);
        assert_eq!(file_limit(None, None, None), 500 * 1024 * 1024);
        assert_eq!(file_limit(Some(u64::MAX), None, None), u64::MAX);
    }
}
