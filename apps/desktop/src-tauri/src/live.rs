//! Live updates for the account on screen, while its vault is open.
//!
//! A UwULock Server that offers it gets the lean realtime channel
//! (`/uwu/v1/realtime`), with the offline copy's cursor so a reconnect that
//! missed something hears about it at once. Everything else — Bitwarden,
//! Vaultwarden, a UwULock Server whose channel doesn't answer — gets
//! Bitwarden's notifications hub. Either way a change means a sync (a delta
//! one on UwULock), a quarter of a second after the last change in a row.
//!
//! While a channel is up, the five-minute check in `vault::start` rests; when
//! none can be had, it goes on as before. A session the server ends (another
//! device changed the master password, this device was removed) locks the
//! account here and asks for a new login: nothing stays decrypted.

use parking_lot::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use uwulock_bitwarden::live::{Backoff, Channel, Closed, Event, Hub, Realtime};

use crate::vault::{self, VaultState};

/// Several changes in a row, one sync.
const SETTLE: Duration = Duration::from_millis(250);
/// A fresh token this long before the old one runs out.
const REAUTH_EARLY: u64 = 60;

/// What the supervisor shares with the rest of the app.
#[derive(Default)]
pub(crate) struct Live {
    wake: Notify,
    /// `realtime` or `hub` while a channel is up.
    channel: Mutex<Option<&'static str>>,
}

impl Live {
    /// Something changed that may change which account to listen for: an
    /// unlock, a lock, a switch, a login, a logout.
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }

    pub(crate) fn channel(&self) -> Option<&'static str> {
        *self.channel.lock()
    }

    fn set(&self, channel: Option<&'static str>) -> bool {
        let mut current = self.channel.lock();
        let changed = *current != channel;
        *current = channel;
        changed
    }
}

/// Which account to listen for, and how: the one on screen, if it is open
/// and its session is good.
#[derive(Clone, PartialEq)]
struct Target {
    id: String,
    realtime: bool,
}

fn target(state: &VaultState) -> Option<Target> {
    let (id, realtime) = state.live_target()?;
    Some(Target { id, realtime })
}

pub(crate) fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut backoff = Backoff::default();
        // A realtime channel that failed before it said ready: the hub,
        // until the next unlock or switch.
        let mut hub_only: Option<String> = None;
        loop {
            let state = app.state::<VaultState>();
            let Some(target) = target(&state) else {
                set_channel(&app, None);
                state.live().wake.notified().await;
                continue;
            };
            if hub_only.as_deref().is_some_and(|id| id != target.id) {
                hub_only = None;
            }
            let realtime = target.realtime && hub_only.is_none();
            let closed = match connect(&app, &target.id, realtime).await {
                Ok(channel) => {
                    let opened = Instant::now();
                    let closed = listen(&app, &target, channel).await;
                    backoff.connected_for(opened.elapsed());
                    closed
                }
                Err(closed) => {
                    if realtime && closed.code.is_none() {
                        tracing::info!(%closed, "no realtime channel; trying the hub");
                        hub_only = Some(target.id.clone());
                        continue;
                    }
                    closed
                }
            };
            set_channel(&app, None);
            if closed.needs_token() {
                app.state::<VaultState>().forget_access_token(&target.id);
            }
            // Gone because the account locked or changed: at once.
            if target_of(&app) != Some(target.clone()) {
                continue;
            }
            let Some(wait) = backoff.after(&closed) else {
                tracing::info!(%closed, "live updates refused; checking every few minutes");
                wait_for_change(&app, &target, Duration::from_secs(3600)).await;
                continue;
            };
            tracing::debug!(%closed, ?wait, "live updates dropped");
            wait_for_change(&app, &target, wait).await;
        }
    });
}

fn target_of(app: &AppHandle) -> Option<Target> {
    target(&app.state::<VaultState>())
}

/// Waits `wait`, or less if the account to listen for changes.
async fn wait_for_change(app: &AppHandle, target: &Target, wait: Duration) {
    let deadline = tokio::time::Instant::now() + wait;
    let state = app.state::<VaultState>();
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return,
            _ = state.live().wake.notified() => {
                if target_of(app).as_ref() != Some(target) {
                    return;
                }
            }
        }
    }
}

async fn connect(app: &AppHandle, id: &str, realtime: bool) -> Result<Channel, Closed> {
    let state = app.state::<VaultState>();
    let (server, token, cursor, device) =
        state.live_credentials(id).await.map_err(|failure| Closed {
            code: (failure.kind() == "session-expired").then_some(4401),
            reason: failure.message().to_string(),
        })?;
    let channel = if realtime {
        Channel::Realtime(Realtime::connect(&server, &token, cursor.as_deref()).await?)
    } else {
        Channel::Hub(Hub::connect(&server, &token, &device).await?)
    };
    set_channel(app, Some(if realtime { "realtime" } else { "hub" }));
    // The hub doesn't know what was missed while nothing listened; the
    // realtime channel says `changed` itself when the cursor is behind.
    if !realtime {
        vault::spawn_sync_of(app.clone(), id.to_string());
    }
    Ok(channel)
}

fn set_channel(app: &AppHandle, channel: Option<&'static str>) {
    if app.state::<VaultState>().live().set(channel) {
        vault::emit_status(app);
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Listens until the connection ends or the account to listen for changes.
async fn listen(app: &AppHandle, target: &Target, mut channel: Channel) -> Closed {
    let mut sync_at: Option<tokio::time::Instant> = None;
    let state = app.state::<VaultState>();
    loop {
        let reauth_at = channel.expires().map(|expires| {
            let left = expires
                .saturating_sub(unix_now())
                .saturating_sub(REAUTH_EARLY);
            tokio::time::Instant::now() + Duration::from_secs(left)
        });
        let far = tokio::time::Instant::now() + Duration::from_secs(86_400);
        tokio::select! {
            event = channel.next() => match event {
                Ok(event) => {
                    if let Some(closed) = handle(app, target, event, &mut sync_at) {
                        channel.close().await;
                        return closed;
                    }
                }
                Err(closed) => return closed,
            },
            _ = tokio::time::sleep_until(sync_at.unwrap_or(far)), if sync_at.is_some() => {
                sync_at = None;
                vault::spawn_sync_of(app.clone(), target.id.clone());
            }
            _ = tokio::time::sleep_until(reauth_at.unwrap_or(far)), if reauth_at.is_some() => {
                match state.live_credentials(&target.id).await {
                    Ok((_, token, cursor, _)) => {
                        if let Err(closed) = channel.reauth(&token, cursor.as_deref()).await {
                            return closed;
                        }
                    }
                    Err(failure) => {
                        channel.close().await;
                        return Closed { code: Some(4401), reason: failure.message().to_string() };
                    }
                }
            }
            _ = state.live().wake.notified() => {
                if target_of(app).as_ref() != Some(target) {
                    channel.close().await;
                    return Closed { code: Some(1000), reason: "the account changed".into() };
                }
            }
        }
    }
}

/// Does what an event asks. `Some` ends the connection.
fn handle(
    app: &AppHandle,
    target: &Target,
    event: Event,
    sync_at: &mut Option<tokio::time::Instant>,
) -> Option<Closed> {
    let mut sync_soon = || *sync_at = Some(tokio::time::Instant::now() + SETTLE);
    match event {
        Event::Changed { .. } | Event::Info => sync_soon(),
        Event::Notice { kind, id } => {
            let _ = app.emit("uwu-notice", serde_json::json!({ "kind": kind, "id": id }));
            // The badges' counts come with the sync.
            sync_soon();
        }
        Event::AuthRequest { id } => {
            let _ = app.emit("auth-request", serde_json::json!({ "id": id }));
        }
        Event::LogOut { reason } => {
            tracing::info!(%reason, "the server ended this device's session");
            app.state::<VaultState>().session_ended(&target.id);
            vault::emit_status(app);
            return Some(Closed {
                code: Some(4401),
                reason,
            });
        }
    }
    None
}
