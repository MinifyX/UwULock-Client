//! The account's own Sends: a text or a file behind a link, for somebody
//! without an account — listed, made, changed, opened to anybody again, and
//! deleted, as the web vault does it. Bitwarden and Vaultwarden have them
//! too; a UwULock Server offers them unless its `features` leave `sends` out.
//!
//! The Sends come opened with the vault (`Vault::sends`): their texts stay
//! here until the page shows one. A change is sealed under the Send's own
//! seed, so the link that was shared keeps working; an entry Send keeps its
//! text (uwulock-core's `send::seal_draft`).
//!
//! A new file Send's file reaches Rust as the raw body of
//! [`stage_send_file`], not as JSON, and waits in the unlock's cache until
//! [`save_send`] encrypts and uploads it. Locking drops it.

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, State};
use uwulock_bitwarden::send::{self as send_core, OpenSend, SendAuth, SendDraft};
use uwulock_bitwarden::uwu::SendDomain;
use zeroize::Zeroizing;

use crate::extras::{ctx, link_bases, uwu_failure, with};
use crate::vault::{sync_account, Failure, Result, VaultState};

/// Bitwarden's limit for a Send's file; a UwULock Server may set a lower one.
const MAX_FILE: u64 = 500 * 1024 * 1024;

/// A Send as the page shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendView {
    id: String,
    /// 0 a text, 1 a file.
    kind: u8,
    name: String,
    notes: Option<String>,
    /// For an entry Send, without its marker line.
    text: Option<String>,
    hidden: bool,
    file_name: Option<String>,
    size: Option<u64>,
    max_access_count: Option<u32>,
    access_count: u32,
    has_password: bool,
    /// 0 only `emails`, 1 with the password, 2 anybody with the link.
    auth_type: u8,
    emails: Vec<String>,
    disabled: bool,
    hide_email: bool,
    revision_date: Option<String>,
    expiration_date: Option<String>,
    deletion_date: Option<String>,
    /// Shared from an item as an entry: its text isn't edited.
    entry: bool,
    link: String,
    /// The send domain its link uses; `None` is the server's own address.
    send_domain_id: Option<String>,
}

fn view(send: &OpenSend, main: &str, domains: &[SendDomain], chosen: Option<&str>) -> SendView {
    let domain = chosen.and_then(|id| domains.iter().find(|d| d.id == id));
    let link = match domain {
        Some(domain) => send_core::link(&domain.url, &send.access_id, &send.seed, true),
        None => send_core::link(main, &send.access_id, &send.seed, false),
    };
    SendView {
        id: send.id.clone(),
        kind: send.kind,
        name: send.name.clone(),
        notes: send.notes.clone(),
        text: send.readable().map(str::to_string),
        hidden: send.hidden,
        file_name: send.file_name.clone(),
        size: send.size,
        max_access_count: send.max_access_count,
        access_count: send.access_count,
        has_password: send.has_password,
        auth_type: send.auth.to_wire(),
        emails: send.emails.clone(),
        disabled: send.disabled,
        hide_email: send.hide_email,
        revision_date: send.revision_date.clone(),
        expiration_date: send.expiration_date.clone(),
        deletion_date: send.deletion_date.clone(),
        entry: send.entry,
        link,
        send_domain_id: domain.map(|d| d.id.clone()),
    }
}

/// Refuses on a UwULock Server that doesn't offer Sends.
fn need_sends(state: &VaultState, account_id: &str) -> Result<()> {
    with(state, account_id, |u| match &u.info {
        Some(info) if !info.has("sends") => Err(Failure::new(
            "feature-off",
            "The server doesn't offer Sends.",
        )),
        _ => Ok(()),
    })
}

/// The account's Sends, newest change first.
#[tauri::command]
pub(crate) fn sends(state: State<'_, VaultState>) -> Result<Vec<SendView>> {
    let (account_id, _) = state.active_account()?;
    need_sends(&state, &account_id)?;
    let (main, domains) = link_bases(&state, &account_id)?;
    with(&state, &account_id, |u| {
        let domains = if u.has("send-domains") {
            domains
        } else {
            Vec::new()
        };
        let mut list: Vec<SendView> = u
            .vault
            .sends
            .iter()
            .map(|send| {
                let chosen = u.uwu.send_domains.get(&send.id).cloned().flatten();
                view(send, &main, &domains, chosen.as_deref())
            })
            .collect();
        list.sort_by(|a, b| b.revision_date.cmp(&a.revision_date));
        Ok(list)
    })
}

/// The file of a new file Send, as the request's raw body: kept until
/// [`save_send`] takes it (or the vault locks). Its name comes with the draft.
#[tauri::command]
pub(crate) fn stage_send_file(state: State<'_, VaultState>, request: Request<'_>) -> Result<()> {
    state.touch();
    let (account_id, _) = state.active_account()?;
    need_sends(&state, &account_id)?;
    // A file refused below must not leave an earlier one waiting: the next
    // save would send that one under the new file's name.
    with(&state, &account_id, |u| {
        u.extras_cache.send_file = None;
        Ok(())
    })?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(Failure::new("invalid", "The file didn't arrive as bytes."));
    };
    with(&state, &account_id, |u| {
        let limit = u
            .info
            .as_ref()
            .and_then(|i| i.limits.as_ref())
            .and_then(|l| l.max_file_bytes)
            .map_or(MAX_FILE, |max| max.min(MAX_FILE));
        // The encrypted file is a little larger; the server checks that one.
        if bytes.len() as u64 > limit {
            return Err(Failure::new(
                "too-large",
                format!("A Send's file may have {} MB at most.", limit / 1024 / 1024),
            ));
        }
        if bytes.is_empty() {
            return Err(Failure::new("invalid", "The file is empty."));
        }
        u.extras_cache.send_file = Some(Zeroizing::new(bytes.clone()));
        Ok(())
    })
}

/// Saves a Send: a new one (`id` none; a file Send takes the file
/// [`stage_send_file`] kept) or a change to one. With `choose_domain`, its
/// link moves to `domain` (a send domain's id; `None` the main address) when
/// that isn't where it is. Answers the Send's id; the list is fresh after.
#[tauri::command]
pub(crate) async fn save_send(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: Option<String>,
    draft: SendDraft,
    domain: Option<String>,
    choose_domain: bool,
) -> Result<String> {
    state.touch();
    let ctx = ctx(&state).await?;
    need_sends(&state, &ctx.account_id)?;
    let (existing, file, emails_on, domains_on) = with(&state, &ctx.account_id, |u| {
        let existing = match &id {
            Some(id) => Some(
                u.vault
                    .sends
                    .iter()
                    .find(|s| s.id == *id)
                    .cloned()
                    .ok_or_else(|| Failure::new("not-found", "This Send isn't there any more."))?,
            ),
            None => None,
        };
        let file = if existing.is_none() && draft.kind == 1 {
            u.extras_cache.send_file.take()
        } else {
            None
        };
        Ok((existing, file, u.has("send-emails"), u.has("send-domains")))
    })?;
    let to_addresses = draft.auth_type == Some(SendAuth::Emails.to_wire());
    let had_addresses = existing
        .as_ref()
        .is_some_and(|s| s.auth == SendAuth::Emails);
    if to_addresses && !emails_on && !had_addresses {
        return Err(Failure::new(
            "feature-off",
            "This server can't limit a Send to addresses.",
        ));
    }
    let sealed = with(&state, &ctx.account_id, |u| {
        Ok(send_core::seal_draft(
            &draft,
            existing.as_ref(),
            &u.user_key,
            file.as_deref().map(Vec::as_slice),
        )?)
    })?;
    drop(file);

    let send_id = match &existing {
        Some(send) => {
            ctx.client
                .update_send(&ctx.token, &send.id, &sealed.request)
                .await?;
            // Open to anybody again: a server that doesn't read `authType`
            // (an older Vaultwarden) keeps a password the change left out.
            let opened = draft.auth_type == Some(SendAuth::None.to_wire());
            if opened && (send.has_password || send.auth != SendAuth::None) {
                ctx.client
                    .remove_send_auth(&ctx.token, &send.id, send.auth == SendAuth::Emails)
                    .await?;
            }
            send.id.clone()
        }
        None if draft.kind == 1 => {
            let upload = ctx
                .client
                .create_file_send(&ctx.token, &sealed.request)
                .await?;
            let name = sealed.request["file"]["fileName"]
                .as_str()
                .unwrap_or("file");
            if let Err(error) = ctx
                .client
                .upload_file(&ctx.token, &upload, name, &sealed.file)
                .await
            {
                // A Send without its file opens nothing: gone again.
                if let Err(cleanup) = ctx.client.delete_send(&ctx.token, &upload.id).await {
                    tracing::warn!(%cleanup, "the Send without its file stayed");
                }
                return Err(error.into());
            }
            upload.id
        }
        None => {
            let answer = ctx.client.create_send(&ctx.token, &sealed.request).await?;
            crate::vault::entry_id(&answer)
                .ok_or_else(|| Failure::new("server", "The server didn't say which Send it made."))?
                .to_string()
        }
    };

    if choose_domain && domains_on {
        // Where the link is now: the Send's choice, or for a new one the
        // account's default, which the server gave it.
        let before = match &existing {
            Some(send) => with(&state, &ctx.account_id, |u| {
                Ok(u.uwu.send_domains.get(&send.id).cloned().flatten())
            })?,
            None => ctx
                .client
                .uwu_account(&ctx.token)
                .await
                .ok()
                .and_then(|a| a.send_domain_id),
        };
        if domain != before {
            // The Send is saved either way; only its link stays where it was.
            ctx.client
                .set_send_domain(&ctx.token, &send_id, domain.as_deref())
                .await
                .map_err(uwu_failure)?;
        }
    }
    resync(&app, &ctx.account_id).await;
    tracing::info!(
        file = draft.kind == 1,
        new = existing.is_none(),
        "Send saved"
    );
    Ok(send_id)
}

/// Anybody with the link may open the Send again: no password, no addresses.
#[tauri::command]
pub(crate) async fn remove_send_auth(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    let ctx = ctx(&state).await?;
    let emails = with(&state, &ctx.account_id, |u| {
        u.vault
            .sends
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.auth == SendAuth::Emails)
            .ok_or_else(|| Failure::new("not-found", "This Send isn't there any more."))
    })?;
    ctx.client.remove_send_auth(&ctx.token, &id, emails).await?;
    resync(&app, &ctx.account_id).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_send(
    app: AppHandle,
    state: State<'_, VaultState>,
    id: String,
) -> Result<()> {
    state.touch();
    let ctx = ctx(&state).await?;
    ctx.client.delete_send(&ctx.token, &id).await?;
    resync(&app, &ctx.account_id).await;
    Ok(())
}

/// The list comes from the sync. A failed one leaves the change made; the
/// next sync shows it.
async fn resync(app: &AppHandle, account_id: &str) {
    if let Err(error) = sync_account(app, account_id).await {
        tracing::warn!(kind = error.kind(), "the sync after a Send's change failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_bitwarden::api::parse_sync;
    use uwulock_bitwarden::{SymmetricKey, Vault};

    #[test]
    fn a_send_links_to_its_own_domain_or_the_main_address() {
        let user = SymmetricKey::generate();
        let draft = SendDraft {
            kind: 0,
            name: "Note".into(),
            text: Some(Zeroizing::new("text".into())),
            deletion_date: "2026-10-13T12:00:00.000Z".into(),
            ..SendDraft::default()
        };
        let sealed = send_core::seal_draft(&draft, None, &user, None).unwrap();
        let mut send = serde_json::json!({ "id": "s1", "accessId": "abc", "type": 0 });
        for key in ["key", "name", "text", "deletionDate"] {
            send[key] = sealed.request[key].clone();
        }
        let sync =
            serde_json::json!({ "profile": { "email": "nyu@example.com" }, "sends": [send] });
        let vault = Vault::open(&parse_sync(&sync.to_string()).unwrap(), &user).unwrap();
        let domains = [SendDomain {
            id: "d1".into(),
            url: "https://send.example.com".into(),
        }];
        let main = view(&vault.sends[0], "https://lock.example.com", &domains, None);
        assert!(main
            .link
            .starts_with("https://lock.example.com/#/send/abc/"));
        assert_eq!(main.send_domain_id, None);
        let own = view(
            &vault.sends[0],
            "https://lock.example.com",
            &domains,
            Some("d1"),
        );
        assert!(own.link.starts_with("https://send.example.com/abc#"));
        assert_eq!(own.send_domain_id.as_deref(), Some("d1"));
        // A domain the admin removed: the main address.
        let gone = view(
            &vault.sends[0],
            "https://lock.example.com",
            &domains,
            Some("d2"),
        );
        assert!(gone.link.starts_with("https://lock.example.com/"));
        assert_eq!((main.text.as_deref(), main.auth_type), (Some("text"), 2));
    }
}
