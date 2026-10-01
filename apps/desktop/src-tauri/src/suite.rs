//! UwUSSH's and UwURDP's records in the app (UwULock-Server
//! `docs/uwu-api.md` §6): the sections "SSH (UwUSSH)" and "Remote Desktop
//! (UwURDP)".
//!
//! The records are sealed under each space's key, which is under the
//! account's extras key; they are opened here, kept for the rest of the
//! unlock ([`Cache`], part of [`Unlocked`](crate::vault::Unlocked)) and pulled
//! again from the last cursor when the server says the area `suite` changed.
//! The page gets the records' JSON, never a secret unless it asks to see
//! one. What an edit becomes is [`plan`]'s business.
//!
//! Every edit carries this device's clock id: random, kept with the account
//! on this device (`Account::suite_device`).
//!
//! Everything is offered only while the server lists the feature `suite`
//! (switch `suite.enabled`).

mod plan;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;
use uwulock_core::suite::openssh;
use uwulock_core::suite::{new_device_id, Space, SpaceVault, PAGE};

use crate::extras::{ctx, extras_key, safe_file_name, save_download, uwu_failure, with, Ctx};
use crate::vault::{Failure, Result, VaultState};
pub(crate) use plan::Op;
use plan::{identity_of, rdp_file, Refusal, SpaceState};

/// The spaces opened in this unlock. Gone on lock.
#[derive(Default)]
pub(crate) struct Cache {
    spaces: HashMap<Space, SpaceState>,
}

impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        Failure::new(refusal.kind, refusal.message)
    }
}

/// A space as the page shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    space: &'static str,
    /// No app has made the space yet: the page offers to.
    exists: bool,
    records: Vec<plan::RecordView>,
}

/// What a save answers: the space now, and the records the server had
/// newer copies of (taken over; the page says so).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Saved {
    view: View,
    conflicts: Vec<String>,
    /// The record a key generation or import made.
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

fn space_of(name: &str) -> Result<Space> {
    match name {
        "ssh" => Ok(Space::Ssh),
        "rdp" => Ok(Space::Rdp),
        _ => Err(Failure::new(
            "invalid",
            format!("no section for space {name}"),
        )),
    }
}

/// Refuses unless the server offers the suite vault.
fn need_suite(state: &VaultState) -> Result<()> {
    let (id, _) = state.active_account()?;
    with(state, &id, |u| {
        if u.has("suite") {
            Ok(())
        } else {
            Err(Failure::new(
                "feature-off",
                "The server doesn't offer the suite vault.",
            ))
        }
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// This device's clock id for the account: made once, kept with it.
fn device(state: &VaultState, account_id: &str) -> Result<u32> {
    if let Some(device) = state.account(account_id)?.suite_device.filter(|d| *d != 0) {
        return Ok(device);
    }
    let device = new_device_id();
    state.update_account(account_id, |a| a.suite_device = Some(device));
    Ok(device)
}

fn with_space<T>(
    state: &VaultState,
    ctx: &Ctx,
    space: Space,
    f: impl FnOnce(&mut SpaceState) -> Result<T>,
) -> Result<T> {
    with(state, &ctx.account_id, |u| {
        let opened = u
            .suite
            .spaces
            .get_mut(&space)
            .ok_or_else(|| Failure::new("not-found", "This space isn't there (any more)."))?;
        f(opened)
    })
}

/// Opens the space if it isn't yet: `false` when the server has none.
async fn open(app: &AppHandle, state: &VaultState, ctx: &Ctx, space: Space) -> Result<bool> {
    let known = with(state, &ctx.account_id, |u| {
        Ok(u.suite.spaces.contains_key(&space))
    })?;
    if known {
        return Ok(true);
    }
    let extras = extras_key(app, state, ctx).await?;
    let found = ctx
        .client
        .suite_spaces(&ctx.token)
        .await
        .map_err(uwu_failure)?
        .into_iter()
        .find(|s| s.space == space.as_str());
    let Some(found) = found else {
        return Ok(false);
    };
    let vault = SpaceVault::open_space(&found, &extras)?;
    with(state, &ctx.account_id, |u| {
        u.suite
            .spaces
            .entry(space)
            .or_insert_with(|| SpaceState::new(vault));
        Ok(())
    })?;
    Ok(true)
}

/// Pulls what changed since the last cursor. A `reset` (or a rekey) starts
/// over from 0, with the space fetched again.
async fn pull(app: &AppHandle, state: &VaultState, ctx: &Ctx, space: Space) -> Result<bool> {
    let mut fresh_start = false;
    loop {
        if !open(app, state, ctx, space).await? {
            return Ok(false);
        }
        let mut since = with_space(state, ctx, space, |s| Ok(s.cursor))?;
        let mut reset = false;
        loop {
            let page = ctx
                .client
                .suite_pull(&ctx.token, space.as_str(), since)
                .await
                .map_err(uwu_failure)?;
            if page.reset {
                reset = true;
                break;
            }
            since = page.cursor;
            let more = page.has_more && !page.records.is_empty();
            with_space(state, ctx, space, |s| {
                for env in page.records {
                    s.apply(env);
                }
                s.cursor = s.cursor.max(page.cursor);
                Ok(())
            })?;
            if !more {
                break;
            }
        }
        if !reset {
            return Ok(true);
        }
        if fresh_start {
            return Err(Failure::new(
                "server",
                "The server keeps resetting the space.",
            ));
        }
        // Too old, or the space was rekeyed: everything again, its key too.
        fresh_start = true;
        forget(state, ctx, space)?;
    }
}

fn forget(state: &VaultState, ctx: &Ctx, space: Space) -> Result<()> {
    with(state, &ctx.account_id, |u| {
        u.suite.spaces.remove(&space);
        Ok(())
    })
}

fn view_of(state: &VaultState, ctx: &Ctx, space: Space) -> Result<View> {
    with(state, &ctx.account_id, |u| {
        Ok(match u.suite.spaces.get(&space) {
            Some(opened) => View {
                space: space.as_str(),
                exists: true,
                records: opened.view(),
            },
            None => View {
                space: space.as_str(),
                exists: false,
                records: Vec::new(),
            },
        })
    })
}

/// The section's records, pulled since last time.
#[tauri::command]
pub(crate) async fn suite_view(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
) -> Result<View> {
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    pull(&app, &state, &ctx, space).await?;
    view_of(&state, &ctx, space)
}

/// Makes the space when no app has yet: a fresh key under the extras key. An
/// app that was quicker wins, and its space is taken.
#[tauri::command]
pub(crate) async fn suite_create(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
) -> Result<View> {
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    if !open(&app, &state, &ctx, space).await? {
        let extras = extras_key(&app, &state, &ctx).await?;
        let (vault, body) = SpaceVault::create(space, &extras);
        let path = format!("/suite/spaces/{}", space.as_str());
        match ctx.client.uwu_put(&ctx.token, &path, &body).await {
            Ok(_) => {
                tracing::info!(space = space.as_str(), "suite space made");
                with(&state, &ctx.account_id, |u| {
                    u.suite.spaces.insert(space, SpaceState::new(vault));
                    Ok(())
                })?;
            }
            Err(error) if error.code() == Some("exists") => {}
            Err(error) => return Err(uwu_failure(error)),
        }
    }
    pull(&app, &state, &ctx, space).await?;
    view_of(&state, &ctx, space)
}

/// Seals the plan and pushes it. A rekey in between (409 `space_changed`)
/// fetches the space and everything in it again, and the page asks to do
/// the change once more.
async fn push(
    app: &AppHandle,
    state: &VaultState,
    ctx: &Ctx,
    space: Space,
    ops: Vec<Op>,
) -> Result<Vec<String>> {
    if !open(app, state, ctx, space).await? {
        return Err(Failure::new("not-found", "This space isn't there yet."));
    }
    let device = device(state, &ctx.account_id)?;
    let (space_id, planned) = with_space(state, ctx, space, |s| {
        Ok((
            s.vault.id.to_string(),
            s.plan(ops, now_ms(), device).map_err(Failure::from)?,
        ))
    })?;
    let mut conflicts = Vec::new();
    for chunk in planned.chunks(PAGE) {
        let answer = match ctx
            .client
            .suite_push(&ctx.token, space.as_str(), &space_id, chunk)
            .await
        {
            Ok(answer) => answer,
            Err(error) if error.code() == Some("space_changed") => {
                tracing::info!(
                    space = space.as_str(),
                    "the space was rekeyed: fetching it again"
                );
                forget(state, ctx, space)?;
                pull(app, state, ctx, space).await?;
                return Err(Failure::new(
                    "space-changed",
                    "The space got a new key meanwhile; it was fetched again.",
                ));
            }
            Err(error) => return Err(uwu_failure(error)),
        };
        let refused = with_space(state, ctx, space, |s| Ok(s.pushed(chunk.to_vec(), answer)))?;
        conflicts.extend(refused);
    }
    if !conflicts.is_empty() {
        tracing::info!(
            n = conflicts.len(),
            "suite records changed elsewhere: taken over"
        );
    }
    Ok(conflicts)
}

/// Saves a batch of the page's changes (`Op`), all checked before anything
/// is sent.
#[tauri::command]
pub(crate) async fn suite_save(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
    ops: Vec<Op>,
) -> Result<Saved> {
    state.touch();
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let conflicts = push(&app, &state, &ctx, space, ops).await?;
    Ok(Saved {
        view: view_of(&state, &ctx, space)?,
        conflicts,
        id: None,
    })
}

fn secret_of(
    state: &VaultState,
    ctx: &Ctx,
    space: Space,
    id: &str,
) -> Result<zeroize::Zeroizing<String>> {
    with_space(state, ctx, space, |s| {
        s.secret_text(id).map_err(Failure::from)
    })
}

/// A secret's text, for the eye.
#[tauri::command]
pub(crate) async fn suite_reveal(
    state: State<'_, VaultState>,
    space: String,
    id: String,
) -> Result<String> {
    state.touch();
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    Ok(secret_of(&state, &ctx, space, &id)?.to_string())
}

/// Copies a secret, cleared after the usual time, without it passing the page.
#[tauri::command]
pub(crate) async fn suite_copy(
    state: State<'_, VaultState>,
    space: String,
    id: String,
) -> Result<()> {
    state.touch();
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let text = secret_of(&state, &ctx, space, &id)?;
    let clear = state.security.lock().clipboard;
    state
        .clipboard
        .copy(&text, clear)
        .map_err(|e| Failure::new("clipboard", e))
}

/// A new Ed25519 key, made here: the private key (encrypted with the
/// passphrase when there is one) and the passphrase as secrets, the key
/// record pointing at them.
#[tauri::command]
pub(crate) async fn suite_generate_key(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
    label: String,
    comment: String,
    passphrase: Option<String>,
) -> Result<Saved> {
    state.touch();
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let passphrase = passphrase
        .filter(|p| !p.is_empty())
        .map(zeroize::Zeroizing::new);
    let phrase = passphrase.clone();
    // bcrypt's rounds take a moment with a passphrase.
    let key = tauri::async_runtime::spawn_blocking(move || {
        openssh::generate_ed25519(&comment, phrase.as_deref().map(String::as_str))
    })
    .await
    .map_err(|e| Failure::new("crypto", e.to_string()))??;
    let ops = key_ops(
        &label,
        &key.key_type,
        &key.public_key,
        &key.private_key,
        passphrase.as_deref().map(String::as_str),
    );
    let id = ops.1;
    let conflicts = push(&app, &state, &ctx, space, ops.0).await?;
    Ok(Saved {
        view: view_of(&state, &ctx, space)?,
        conflicts,
        id: Some(id),
    })
}

/// An existing OpenSSH private key, taken in. An encrypted one needs the
/// passphrase that opens it.
#[tauri::command]
pub(crate) async fn suite_import_key(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
    label: String,
    private_key: String,
    passphrase: Option<String>,
) -> Result<Saved> {
    state.touch();
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let private_key = zeroize::Zeroizing::new(private_key.trim().to_owned() + "\n");
    // PEM and PuTTY keys aren't read here: kept as they are, with what their
    // first line says; UwUSSH reads them when it connects.
    let (info, foreign) = match openssh::inspect_private_key(&private_key) {
        Ok(info) => (info, false),
        Err(error) => match plan::foreign_key_type(&private_key) {
            Some(key_type) => (
                openssh::KeyInfo {
                    key_type,
                    public_key: String::new(),
                    fingerprint: String::new(),
                    comment: String::new(),
                    encrypted: false,
                },
                true,
            ),
            None => return Err(Failure::new("key-format", error.to_string())),
        },
    };
    let passphrase = passphrase
        .filter(|p| !p.is_empty())
        .map(zeroize::Zeroizing::new);
    if info.encrypted {
        let Some(phrase) = passphrase.clone() else {
            return Err(Failure::new(
                "key-passphrase",
                "This key needs its passphrase.",
            ));
        };
        let text = private_key.clone();
        let opens =
            tauri::async_runtime::spawn_blocking(move || openssh::passphrase_opens(&text, &phrase))
                .await
                .map_err(|e| Failure::new("crypto", e.to_string()))??;
        if !opens {
            return Err(Failure::new(
                "key-passphrase",
                "The passphrase doesn't open this key.",
            ));
        }
    }
    let phrase = if info.encrypted || foreign {
        passphrase.as_deref().map(String::as_str)
    } else {
        None
    };
    let (ops, id) = key_ops(
        &label,
        &info.key_type,
        &info.public_key,
        &private_key,
        phrase,
    );
    let conflicts = push(&app, &state, &ctx, space, ops).await?;
    Ok(Saved {
        view: view_of(&state, &ctx, space)?,
        conflicts,
        id: Some(id),
    })
}

/// The records of a new key: its secrets, and the key pointing at them.
fn key_ops(
    label: &str,
    key_type: &str,
    public_key: &str,
    private_key: &str,
    passphrase: Option<&str>,
) -> (Vec<Op>, String) {
    let key_id = Uuid::new_v4().to_string();
    let private_id = Uuid::new_v4().to_string();
    let passphrase_id = passphrase.map(|_| Uuid::new_v4().to_string());
    let mut ops = vec![Op::Secret {
        id: private_id.clone(),
        text: private_key.to_owned(),
        seq: None,
    }];
    if let (Some(id), Some(text)) = (&passphrase_id, passphrase) {
        ops.push(Op::Secret {
            id: id.clone(),
            text: text.to_owned(),
            seq: None,
        });
    }
    ops.push(Op::Put {
        id: key_id.clone(),
        kind: "key".into(),
        seq: None,
        patch: serde_json::json!({
            "label": label.trim(),
            "key_type": key_type,
            "public_key": public_key,
            "private_secret_id": private_id,
            "passphrase_secret_id": passphrase_id,
        }),
    });
    (ops, key_id)
}

/// Saves a key's private half (`private`) or its public line (`public`) as
/// a file: Downloads on a computer and on Android, UwULock's folder on iOS.
#[tauri::command]
pub(crate) async fn suite_save_key(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
    id: String,
    half: String,
) -> Result<String> {
    state.touch();
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let (label, public, private_id) = with_space(&state, &ctx, space, |s| {
        let key = s.record(&id, "key")?;
        let text = |f: &str| {
            key.get(f)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned()
        };
        Ok((text("label"), text("public_key"), text("private_secret_id")))
    })?;
    let base = {
        let name = safe_file_name(&label);
        if name.is_empty() || name == "_" {
            "id_ed25519".to_owned()
        } else {
            name
        }
    };
    let saved = match half.as_str() {
        "public" => {
            if public.is_empty() {
                return Err(Failure::new("not-found", "This key has no public half."));
            }
            save_download(
                &app,
                &format!("{base}.pub"),
                format!("{}\n", public.trim()).as_bytes(),
            )
            .await?
        }
        "private" => {
            let text = secret_of(&state, &ctx, space, &private_id)?;
            save_download(&app, &base, text.as_bytes()).await?
        }
        _ => return Err(Failure::new("invalid", "public or private")),
    };
    tracing::info!("a suite key saved as a file");
    Ok(saved)
}

/// A `.rdp` file of a UwURDP host: no password, no drives.
#[tauri::command]
pub(crate) async fn suite_save_rdp(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<String> {
    state.touch();
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let (name, text) = with_space(&state, &ctx, Space::Rdp, |s| {
        let host = s.record(&id, "host")?;
        let name = host
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|n| !n.trim().is_empty())
            .or_else(|| host.get("address").and_then(|v| v.as_str()))
            .unwrap_or("host")
            .to_owned();
        Ok((name, rdp_file(host, identity_of(s, host))))
    })?;
    save_download(
        &app,
        &format!("{}.rdp", safe_file_name(&name)),
        text.as_bytes(),
    )
    .await
}

/// Opens UwUSSH or UwURDP at a host (`uwussh://connect/<id>`): only the
/// record's id travels. Computers only.
#[tauri::command]
pub(crate) async fn suite_open_in_app(
    app: AppHandle,
    state: State<'_, VaultState>,
    space: String,
    id: String,
) -> Result<()> {
    let space = space_of(&space)?;
    need_suite(&state)?;
    let ctx = ctx(&state).await?;
    let id = with_space(&state, &ctx, space, |s| {
        s.record(&id, "host")?;
        Uuid::parse_str(&id)
            .map(|u| u.to_string())
            .map_err(|_| Failure::new("invalid", "not a UUID"))
    })?;
    open_app(&app, space, &id)
}

#[cfg(desktop)]
fn open_app(app: &AppHandle, space: Space, id: &str) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let scheme = if space == Space::Rdp {
        "uwurdp"
    } else {
        "uwussh"
    };
    app.opener()
        .open_url(format!("{scheme}://connect/{id}"), None::<&str>)
        .map_err(|e| Failure::new("no-app", format!("Couldn't open the app: {e}")))
}

#[cfg(mobile)]
fn open_app(_app: &AppHandle, _space: Space, _id: &str) -> Result<()> {
    Err(Failure::new(
        "unsupported",
        "UwUSSH and UwURDP run on computers only",
    ))
}
