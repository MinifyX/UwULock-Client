//! The password check and its review one login at a time (UwULock-Server's
//! `docs/uwu-api.md` §15): the rules are uwulock-core's (`health`), the
//! calls uwulock-bitwarden's; this module puts them together for the page.
//!
//! Weak, reused and `http://` logins are found on any server. Breached
//! passwords, breached sites, 2FA Directory, change-password pages, the check
//! of addresses and the shared ignore list need a UwULock Server that offers
//! them; what it switched off (or doesn't know yet) is left out without a
//! word. Passwords never reach the page: it gets findings, and the sources
//! only get hash prefixes through the server.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use uwulock_bitwarden::health::{is_conflict, BreachSwitches, EmailOptIn, Stored, EMAILS_PER_CALL};
use uwulock_bitwarden::uwu::Info;
use uwulock_bitwarden::{EncString, SymmetricKey};
use uwulock_core::health::{
    self, Card, IgnoreList, Ignored, MissingTwofa, ProblemKind, Report, SiteBreach, SiteBreachList,
    SiteIndex, Source, TwofaDirectory, TwofaIndex,
};
use zeroize::Zeroizing;

use crate::extras::{ctx, extras_key, uwu_failure, with, Ctx};
use crate::vault::{change_item, iso_now, Failure, Result, VaultState};

/// What the check fetched in this unlock. Part of the unlocked vault, so
/// locking drops it.
#[derive(Default)]
pub(crate) struct Cache {
    /// The last report with breaches: checked here, or the one a client saved
    /// on the server, and when.
    last: Option<(Report, Option<String>)>,
    /// The saved report was asked for (there may have been none).
    asked_saved: bool,
    sites: Option<Arc<SiteBreachList>>,
    twofa: Option<Arc<TwofaDirectory>>,
    /// The ignore list and the revision it was read at.
    ignores: Option<(IgnoreList, Option<String>)>,
    /// Change-password pages by host.
    pages: HashMap<String, Option<String>>,
}

/// The check as the page shows it: the report's groups and the review's
/// cards from the same findings.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthView {
    /// A UwULock Server: breach sources and the ignore list may be there.
    uwu: bool,
    switches: BreachSwitches,
    report: Report,
    /// When the breaches shown were asked for; `None` when they weren't.
    checked_at: Option<String>,
    /// The site breach after the last password change, by login.
    site_breaches: BTreeMap<String, SiteBreach>,
    site_sources: Vec<Source>,
    /// The list of breached sites was on, but didn't come.
    sites_failed: bool,
    twofa: Vec<MissingTwofa>,
    twofa_source: Option<Source>,
    twofa_failed: bool,
    /// What isn't shown again; `None` where the server keeps no list.
    ignored: Option<Vec<Ignored>>,
    cards: Vec<Card>,
    /// The account's consent to the check of its addresses, when the admin
    /// has that check on.
    email_opt_in: Option<EmailOptIn>,
}

fn info_of(state: &VaultState, id: &str) -> Result<Option<Info>> {
    with(state, id, |u| Ok(u.info.clone()))
}

fn seal(key: &SymmetricKey, json: &str) -> String {
    EncString::encrypt(json.as_bytes(), key).to_string()
}

fn open(key: &SymmetricKey, data: &str) -> Option<String> {
    let sealed: EncString = data.parse().ok()?;
    let bytes = sealed.decrypt(key).ok()?;
    String::from_utf8(bytes.to_vec()).ok()
}

/// The ignore list on the server, opened. One that doesn't open (another
/// extras key) starts over, as in the web vault.
async fn load_ignores(
    app: &AppHandle,
    state: &VaultState,
    ctx: &Ctx,
) -> Result<(IgnoreList, Option<String>)> {
    let stored = ctx
        .client
        .health_ignores(&ctx.token)
        .await
        .map_err(uwu_failure)?;
    let list = match &stored.data {
        Some(data) => {
            let key = extras_key(app, state, ctx).await?;
            open(&key, data)
                .map(|json| IgnoreList::parse(&json))
                .unwrap_or_default()
        }
        None => IgnoreList::default(),
    };
    with(state, &ctx.account_id, |u| {
        u.health.ignores = Some((list.clone(), stored.revision_date.clone()));
        Ok(())
    })?;
    Ok((list, stored.revision_date))
}

/// The report a client saved on the server, opened; `None` when there is
/// none or it doesn't open.
async fn saved_report(
    app: &AppHandle,
    state: &VaultState,
    ctx: &Ctx,
) -> Option<(Report, Option<String>)> {
    let Stored {
        data,
        revision_date,
    } = ctx.client.health_report(&ctx.token).await.ok()?;
    let key = extras_key(app, state, ctx).await.ok()?;
    let report: Report = serde_json::from_str(&open(&key, &data?)?).ok()?;
    Some((report, revision_date))
}

/// Keeps `report` on the server for the next time and the other apps.
fn save_report(app: &AppHandle, report: Report) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = tauri::Manager::state::<VaultState>(&app);
        let Ok(ctx) = ctx(&state).await else { return };
        let Ok(key) = extras_key(&app, &state, &ctx).await else {
            return;
        };
        let Ok(json) = serde_json::to_string(&report) else {
            return;
        };
        if let Err(error) = ctx
            .client
            .put_health_report(&ctx.token, &seal(&key, &json))
            .await
        {
            tracing::warn!(%error, "the report wasn't kept on the server");
        }
    });
}

/// The password check. `fresh`: ask the breach sources again; otherwise the
/// last answers are used for the passwords that haven't changed since.
#[tauri::command]
pub(crate) async fn health_report(
    app: AppHandle,
    state: State<'_, VaultState>,
    fresh: bool,
) -> Result<HealthView> {
    state.touch();
    let (id, _) = state.active_account()?;
    let prepared = with(&state, &id, |u| Ok(health::prepare(&u.vault.items)))?;
    let mut view = HealthView {
        report: prepared.report.clone(),
        ..HealthView::default()
    };
    let info = info_of(&state, &id)?;
    let Some(info) = info else {
        // Bitwarden, Vaultwarden: only what can be found on this device.
        view.cards = health::cards(&view.report, None, &HashMap::new(), &IgnoreList::default());
        return Ok(view);
    };
    view.uwu = true;
    let switches = BreachSwitches::of(&info);
    view.switches = switches;
    let ctx = ctx(&state).await?;

    // Breached passwords.
    if fresh && (switches.hibp || switches.xon_passwords) {
        let progress_app = app.clone();
        let answers = ctx
            .client
            .breach_counts(
                &ctx.token,
                &prepared,
                switches.hibp,
                switches.xon_passwords,
                move |progress| {
                    let _ = progress_app.emit(
                        "health-progress",
                        json!({
                            "done": progress.done(),
                            "total": progress.total(),
                            "hibp": progress.hibp,
                            "xon": progress.xon,
                        }),
                    );
                },
            )
            .await;
        answers
            .counts
            .apply(&mut view.report, true, answers.incomplete);
        let now = iso_now();
        view.checked_at = Some(now.clone());
        with(&state, &id, |u| {
            u.health.last = Some((view.report.clone(), Some(now)));
            Ok(())
        })?;
        save_report(&app, view.report.clone());
    } else {
        let (known, asked) = with(&state, &id, |u| {
            Ok((u.health.last.clone(), u.health.asked_saved))
        })?;
        let last = match known {
            Some(last) => Some(last),
            None if !asked => {
                let saved = saved_report(&app, &state, &ctx).await;
                with(&state, &id, |u| {
                    u.health.asked_saved = true;
                    if u.health.last.is_none() {
                        u.health.last = saved.clone();
                    }
                    Ok(())
                })?;
                saved
            }
            None => None,
        };
        if let Some((earlier, date)) = last {
            health::carry_breaches(&mut view.report, &earlier);
            if view.report.breaches_checked {
                view.checked_at = date;
            }
        }
    }

    // The lists, once per unlock.
    let (sites, twofa) = with(&state, &id, |u| {
        Ok((u.health.sites.clone(), u.health.twofa.clone()))
    })?;
    let sites = match sites {
        Some(sites) => Some(sites),
        None if switches.site_breaches => match ctx.client.site_breaches(&ctx.token).await {
            Ok(list) => {
                let list = Arc::new(list);
                with(&state, &id, |u| {
                    u.health.sites = Some(list.clone());
                    Ok(())
                })?;
                Some(list)
            }
            Err(error) => {
                tracing::warn!(%error, "no list of breached sites");
                view.sites_failed = true;
                None
            }
        },
        None => None,
    };
    let twofa_on = info.has("twofa-directory");
    let twofa = match twofa {
        Some(list) if twofa_on => Some(list),
        _ if twofa_on => match ctx.client.twofa_directory(&ctx.token).await {
            Ok(list) => {
                let list = Arc::new(list);
                with(&state, &id, |u| {
                    u.health.twofa = Some(list.clone());
                    Ok(())
                })?;
                Some(list)
            }
            Err(error) => {
                tracing::warn!(%error, "no list of sites with two-step login");
                view.twofa_failed = true;
                None
            }
        },
        _ => None,
    };

    // What isn't shown again: read anew each time, another device may have changed it.
    let ignores = if BreachSwitches::ignore_list(&info) {
        match load_ignores(&app, &state, &ctx).await {
            Ok((list, _)) => Some(list),
            Err(error) => {
                tracing::warn!(kind = error.kind(), "the ignore list didn't load");
                None
            }
        }
    } else {
        None
    };

    if switches.email_check {
        view.email_opt_in = ctx.client.email_opt_in(&ctx.token).await.ok();
    }

    let index = sites.as_ref().map(|list| SiteIndex::new(&list.breaches));
    if let Some(list) = &sites {
        view.site_sources = list.sources.clone();
    }
    if let Some(index) = &index {
        view.site_breaches = view
            .report
            .findings
            .iter()
            .filter_map(|f| Some((f.id.clone(), index.after_change(f)?.clone())))
            .collect();
    }
    if let Some(directory) = &twofa {
        let index = TwofaIndex::new(&directory.entries);
        view.twofa = with(&state, &id, |u| {
            Ok(health::missing_twofa(&u.vault.items, &index))
        })?;
        view.twofa_source = Some(directory.source.clone());
    }
    let docs: HashMap<String, Option<String>> = view
        .twofa
        .iter()
        .map(|m| (m.item_id.clone(), m.entry.documentation.clone()))
        .collect();
    let ignore_list = ignores.clone().unwrap_or_default();
    view.cards = health::cards(&view.report, index.as_ref(), &docs, &ignore_list);
    view.ignored = ignores.map(|list| list.ignored);
    Ok(view)
}

/// Hides (or shows again) one problem of one login, on every device. Saved
/// only if no other device saved the list meanwhile; otherwise the newer
/// list is read and the change made on it.
#[tauri::command]
pub(crate) async fn health_ignore(
    app: AppHandle,
    state: State<'_, VaultState>,
    item_id: String,
    kind: ProblemKind,
    ignored: bool,
) -> Result<Vec<Ignored>> {
    state.touch();
    let (id, _) = state.active_account()?;
    let info = info_of(&state, &id)?;
    if !info.as_ref().is_some_and(BreachSwitches::ignore_list) {
        return Err(Failure::new(
            "feature-off",
            "The server keeps no list of ignored problems.",
        ));
    }
    let ctx = ctx(&state).await?;
    let key = extras_key(&app, &state, &ctx).await?;
    let mut current = match with(&state, &id, |u| Ok(u.health.ignores.clone()))? {
        Some(known) => known,
        None => load_ignores(&app, &state, &ctx).await?,
    };
    for _ in 0..3 {
        let (mut list, revision) = current;
        if ignored {
            list.ignore(&item_id, kind, &iso_now());
        } else {
            list.unignore(&item_id, kind);
        }
        let ids: Vec<String> = with(&state, &id, |u| {
            Ok(u.vault.items.iter().map(|i| i.id.clone()).collect())
        })?;
        list.tidy(&ids.iter().map(String::as_str).collect::<HashSet<_>>());
        match ctx
            .client
            .put_health_ignores(
                &ctx.token,
                &seal(&key, &list.to_json()),
                revision.as_deref(),
            )
            .await
        {
            Ok(stored) => {
                with(&state, &id, |u| {
                    u.health.ignores = Some((list.clone(), stored.revision_date));
                    Ok(())
                })?;
                return Ok(list.ignored);
            }
            Err(error) if is_conflict(&error) => {
                current = load_ignores(&app, &state, &ctx).await?;
            }
            Err(error) => return Err(uwu_failure(error)),
        }
    }
    Err(Failure::new(
        "conflict",
        "The list kept changing on another device.",
    ))
}

/// Opens the login's site to change the password, in the system browser: the
/// site's `/.well-known/change-password` if the server found one, else the
/// login's address, else the site.
#[tauri::command]
pub(crate) async fn health_open_page(
    app: AppHandle,
    state: State<'_, VaultState>,
    item_id: String,
) -> Result<()> {
    state.touch();
    let (id, _) = state.active_account()?;
    let finding = with(&state, &id, |u| {
        let item = u
            .vault
            .item(&item_id)
            .ok_or_else(|| Failure::new("not-found", "This item isn't in the vault any more."))?;
        Ok(health::prepare([item])
            .report
            .findings
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                // A login without a password still has a site.
                let host = item
                    .login
                    .as_ref()
                    .and_then(|l| l.uris.iter().find_map(|u| health::host_of(&u.uri)));
                health::Finding {
                    host,
                    ..Default::default()
                }
            }))
    })?;
    let page = match (&finding.host, info_of(&state, &id)?) {
        (Some(host), Some(info)) if BreachSwitches::of(&info).change_password => {
            let known = with(&state, &id, |u| Ok(u.health.pages.get(host).cloned()))?;
            match known {
                Some(page) => page,
                None => {
                    let ctx = ctx(&state).await?;
                    match ctx.client.change_password_page(&ctx.token, host).await {
                        Ok(page) => {
                            with(&state, &id, |u| {
                                u.health.pages.insert(host.clone(), page.clone());
                                Ok(())
                            })?;
                            page
                        }
                        // Not knowing it isn't a reason not to open the site.
                        Err(_) => None,
                    }
                }
            }
        }
        _ => None,
    };
    let target = page
        .or(finding.uri.clone())
        .or_else(|| finding.host.as_ref().map(|host| format!("https://{host}/")))
        .ok_or_else(|| Failure::new("invalid", "This login has no web address."))?;
    let parsed = url::Url::parse(&target)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or_else(|| Failure::new("invalid", "Only web addresses open in the browser."))?;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| Failure::new("io", format!("Couldn't open the browser: {e}")))
}

/// Saves a new password to a login; the old one goes into its history (five
/// at most, newest first), as Bitwarden does.
#[tauri::command]
pub(crate) async fn health_save_password(
    app: AppHandle,
    state: State<'_, VaultState>,
    item_id: String,
    password: String,
) -> Result<()> {
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err(Failure::new("invalid", "The new password is empty."));
    }
    let (id, _) = state.active_account()?;
    // An item with the re-prompt is changed only after the master password, as everywhere else.
    let is_login = crate::vault::with_item(&state, &item_id, |item| Ok(item.login.is_some()))?;
    if !is_login {
        return Err(Failure::new("invalid", "This item isn't a login."));
    }
    let now = iso_now();
    change_item(&app, &state, &item_id, |item| {
        item.set_password(password, &now)
    })
    .await?;
    // The kept report knows of it too, until the next check.
    let last = with(&state, &id, |u| {
        Ok(u.health.last.as_mut().map(|(report, _)| {
            health::renewed(report, &item_id, &now);
            report.clone()
        }))
    })?;
    if let Some(report) = last {
        if info_of(&state, &id)?.is_some() {
            save_report(&app, report);
        }
    }
    Ok(())
}

/// The account's consent to the check of its addresses; `None` when the
/// server doesn't offer the check.
#[tauri::command]
pub(crate) async fn health_email_opt_in(
    state: State<'_, VaultState>,
) -> Result<Option<EmailOptIn>> {
    let (id, _) = state.active_account()?;
    let on = info_of(&state, &id)?
        .as_ref()
        .is_some_and(|info| BreachSwitches::of(info).email_check);
    if !on {
        return Ok(None);
    }
    let ctx = ctx(&state).await?;
    ctx.client
        .email_opt_in(&ctx.token)
        .await
        .map(Some)
        .map_err(uwu_failure)
}

#[tauri::command]
pub(crate) async fn set_health_email_opt_in(
    state: State<'_, VaultState>,
    opted_in: bool,
) -> Result<EmailOptIn> {
    state.touch();
    let ctx = ctx(&state).await?;
    ctx.client
        .set_email_opt_in(&ctx.token, opted_in)
        .await
        .map_err(uwu_failure)
}

/// One address as the page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailView {
    email: String,
    status: String,
    /// The breaches' titles, with the year where the list of sites knows it.
    breaches: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailsView {
    results: Vec<EmailView>,
    retry_after: Option<u64>,
}

/// Asks the server about the account's address and the logins' usernames
/// that are addresses. Waits a few seconds for the server's budget, not
/// longer: what is left says "later".
#[tauri::command]
pub(crate) async fn health_check_emails(
    app: AppHandle,
    state: State<'_, VaultState>,
) -> Result<EmailsView> {
    state.touch();
    let (id, account) = state.active_account()?;
    // The usernames below come out of the encrypted vault: they only go to the server when it
    // offers the check and this account agreed to it — checked here, not only by the page.
    let on = info_of(&state, &id)?
        .as_ref()
        .is_some_and(|info| BreachSwitches::of(info).email_check);
    if !on {
        return Err(Failure::new(
            "feature-off",
            "The server doesn't offer the check of addresses.",
        ));
    }
    let ctx = ctx(&state).await?;
    let consent = ctx
        .client
        .email_opt_in(&ctx.token)
        .await
        .map_err(uwu_failure)?;
    if !consent.opted_in {
        return Err(Failure::new(
            "consent",
            "Switch the check of addresses on in the settings first.",
        ));
    }
    let mut addresses: Vec<String> = vec![account.email.clone()];
    addresses.extend(with(&state, &id, |u| {
        Ok(u.vault
            .items
            .iter()
            .filter(|item| !item.deleted)
            .filter_map(|item| item.login.as_ref()?.username.as_ref())
            .map(|name| name.to_string())
            .collect::<Vec<_>>())
    })?);
    let mut unique: Vec<String> = Vec::new();
    for address in addresses {
        let address = address.trim().to_lowercase();
        if health::is_address(&address) && !unique.contains(&address) {
            unique.push(address);
        }
    }
    let mut results: BTreeMap<String, uwulock_bitwarden::health::EmailResult> = BTreeMap::new();
    let mut retry_after = None;
    'batches: for chunk in unique.chunks(EMAILS_PER_CALL) {
        let mut batch: Vec<String> = chunk.to_vec();
        for _ in 0..30 {
            let answer = ctx
                .client
                .check_emails(&ctx.token, &batch)
                .await
                .map_err(uwu_failure)?;
            for result in answer.results {
                results.insert(result.email.clone(), result);
            }
            let done = results.values().filter(|r| r.status != "later").count();
            let _ = app.emit(
                "health-progress",
                json!({ "done": done, "total": unique.len() }),
            );
            retry_after = answer.retry_after;
            batch = results
                .values()
                .filter(|r| r.status == "later" && batch.contains(&r.email))
                .map(|r| r.email.clone())
                .collect();
            match retry_after {
                Some(wait) if !batch.is_empty() && wait <= 5 => {
                    tokio::time::sleep(std::time::Duration::from_secs(wait.max(1))).await;
                }
                Some(_) if !batch.is_empty() => break 'batches,
                _ => break,
            }
        }
    }
    let sites = with(&state, &id, |u| Ok(u.health.sites.clone()))?;
    let index = sites.map(|list| SiteIndex::new(&list.breaches));
    let results = unique
        .iter()
        .map(|address| {
            let found = results.get(address);
            EmailView {
                email: address.clone(),
                status: found.map_or("later", |r| r.status.as_str()).to_string(),
                breaches: found
                    .map(|r| {
                        r.breaches
                            .iter()
                            .map(
                                |name| match index.as_ref().and_then(|i| i.by_xon_name(name)) {
                                    Some(breach) => match breach.date.as_deref() {
                                        Some(date) if date.len() >= 4 => {
                                            format!("{} ({})", breach.title, &date[..4])
                                        }
                                        _ => breach.title.clone(),
                                    },
                                    None => name.clone(),
                                },
                            )
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    Ok(EmailsView {
        results,
        retry_after,
    })
}
