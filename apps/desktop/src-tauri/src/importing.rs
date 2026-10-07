//! Moving in from a file: the window's side of it. The page reads the file
//! (any app's export, `src/lib/import`) and hands over Bitwarden's
//! unencrypted JSON (or Bitwarden's own CSV as it is); here it becomes items,
//! sealed under the user key, and goes to the server with
//! `POST /api/ciphers/import` in parts of [`CHUNK`], with an
//! `import-progress` event after each. Nothing of it is written to disk; the
//! sync afterwards brings the new items into the cached vault.
//!
//! Folders: one with the same name as a folder the account has is that
//! folder; the others are made first, so that every part of the import puts
//! its items into the same ones.
//!
//! The slow parts of opening a file stay here too: KeePass's key derivations
//! and Bitwarden's password-protected export.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter, State};
use uwulock_bitwarden::api::import_requests;
use uwulock_bitwarden::{wire, EncString, Error, SymmetricKey};
use uwulock_core::import::{self, Prepared};
use zeroize::Zeroizing;

use crate::vault::{
    access_token, emit_status, entry_id, iso_now, sync_account, Failure, VaultState,
};

type Result<T> = std::result::Result<T, Failure>;

/// Items per request: small enough for any server's body limit and its
/// 60 seconds, large enough that thousands go in a few requests.
const CHUNK: usize = 200;

/// The longest encrypted notes Bitwarden (and UwULock Server) take.
const MAX_SEALED_NOTES: usize = 10_000;

/// Whether a sealed item has a value longer than Bitwarden's server takes
/// (its `EncryptedStringLength` limits): one such item would make the whole
/// request of [`CHUNK`] items fail, so it stays out on its own as `too-long`.
/// UwULock Server and Vaultwarden only limit the notes.
fn too_long(request: &wire::CipherRequest) -> bool {
    let over = |value: Option<&String>, max: usize| value.is_some_and(|v| v.len() > max);
    if request.name.len() > 1_000 || over(request.notes.as_ref(), MAX_SEALED_NOTES) {
        return true;
    }
    if let Some(login) = &request.login {
        if over(login.username.as_ref(), 1_000)
            || over(login.password.as_ref(), 5_000)
            || over(login.totp.as_ref(), 1_000)
            || login.uris.iter().any(|u| over(u.uri.as_ref(), 10_000))
        {
            return true;
        }
    }
    if let Some(card) = &request.card {
        let values = [
            &card.cardholder_name,
            &card.brand,
            &card.number,
            &card.exp_month,
            &card.exp_year,
            &card.code,
        ];
        if values.iter().any(|v| over(v.as_ref(), 1_000)) {
            return true;
        }
    }
    let fields = request.fields.iter().flatten();
    if fields
        .clone()
        .any(|f| over(f.name.as_ref(), 1_000) || over(f.value.as_ref(), 5_000))
    {
        return true;
    }
    request
        .password_history
        .iter()
        .flatten()
        .any(|h| h.password.len() > 5_000)
}

fn file_failure(error: Error) -> Failure {
    match error {
        Error::WrongKey => Failure::new("import-password", "The password of the file is wrong."),
        Error::Unsupported(what) => Failure::new("import-unsupported", what),
        other => Failure::new("import-file", other.to_string()),
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: usize,
    total: usize,
}

/// An item that stayed out, and why: `invalid` (an SSH key without all its
/// parts, …), `too-long` (a value, mostly the notes, longer than a server takes).
#[derive(Serialize)]
pub(crate) struct Skipped {
    name: String,
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportOutcome {
    imported: usize,
    folders_created: usize,
    skipped: Vec<Skipped>,
    /// Set when the import stopped part way: what came in before stays.
    error: Option<Failure>,
}

/// The items of `prepared`, sealed under `key`; the ones that can't go are
/// in the second list. Returns, with each sealed item, the index of the item
/// in `prepared`.
fn seal_all(
    prepared: &mut Prepared,
    key: &SymmetricKey,
) -> (Vec<(usize, wire::CipherRequest)>, Vec<Skipped>) {
    let mut sealed = Vec::with_capacity(prepared.items.len());
    let mut skipped = Vec::new();
    for (index, item) in prepared.items.iter_mut().enumerate() {
        let skip = |reason| Skipped {
            name: item.name.to_string(),
            reason,
        };
        if let Some(passkeys) = item.login.as_mut().and_then(|l| l.passkeys.as_mut()) {
            if import::seal_passkeys(passkeys, key).is_err() {
                skipped.push(skip("invalid"));
                continue;
            }
        }
        if item.can_save().is_err() {
            skipped.push(skip("invalid"));
            continue;
        }
        match item.seal(key) {
            Ok(request) if too_long(&request) => skipped.push(skip("too-long")),
            Ok(request) => sealed.push((index, request)),
            Err(_) => skipped.push(skip("invalid")),
        }
    }
    (sealed, skipped)
}

/// Imports `text` (`format`: `json` or `csv`, Bitwarden's) into the account
/// on screen.
#[tauri::command]
pub(crate) async fn import_vault(
    app: AppHandle,
    state: State<'_, VaultState>,
    format: String,
    text: String,
) -> Result<ImportOutcome> {
    state.touch();
    let text = Zeroizing::new(text);
    let (account_id, account) = state.active_account()?;
    let (key, existing) = {
        let guard = state.unlocked.read();
        let unlocked = guard.get(&account_id).ok_or_else(Failure::locked)?;
        // Two of the account's folders with one name: always the first.
        let mut existing: HashMap<String, String> = HashMap::new();
        for folder in &unlocked.vault.folders {
            existing
                .entry(folder.name.clone())
                .or_insert_with(|| folder.id.clone());
        }
        (unlocked.user_key.clone(), existing)
    };
    let now = iso_now();
    // Reading and sealing thousands of items takes a moment: not on the IPC thread.
    let (folders, in_folder, sealed, skipped) = tauri::async_runtime::spawn_blocking(move || {
        let mut prepared = import::read(&format, &text, &now).map_err(file_failure)?;
        let (sealed, skipped) = seal_all(&mut prepared, &key);
        // Each folder: the account's own of that name, or none yet; the name sealed.
        let folders: Vec<(Option<String>, String)> = prepared
            .folders
            .iter()
            .map(|name| {
                let sealed = EncString::encrypt(name.as_bytes(), &key).to_string();
                (existing.get(name).cloned(), sealed)
            })
            .collect();
        // A file with two folders of one name: one folder, the first.
        let mut first: HashMap<&str, usize> = HashMap::new();
        let canonical: Vec<usize> = prepared
            .folders
            .iter()
            .enumerate()
            .map(|(index, name)| *first.entry(name.as_str()).or_insert(index))
            .collect();
        let in_folder = prepared
            .in_folder
            .iter()
            .map(|&(item, folder)| (item, canonical.get(folder).copied().unwrap_or(folder)))
            .collect::<Vec<_>>();
        Ok::<_, Failure>((folders, in_folder, sealed, skipped))
    })
    .await
    .map_err(|e| Failure::new("crypto", e.to_string()))??;

    let client = state.client(account.server.clone())?;
    let total = sealed.len();
    let mut outcome = ImportOutcome {
        imported: 0,
        folders_created: 0,
        skipped: Vec::new(),
        error: None,
    };
    let _ = app.emit("import-progress", Progress { done: 0, total });

    // Only the folders something goes into; the new ones made first, so that
    // every part puts its items into the same ones.
    let used: HashSet<usize> = in_folder
        .iter()
        .filter(|(item, _)| sealed.iter().any(|(index, _)| index == item))
        .map(|(_, folder)| *folder)
        .collect();
    let run = async {
        let mut ready: Vec<Option<(String, String)>> = Vec::with_capacity(folders.len());
        for (index, (id, name)) in folders.iter().enumerate() {
            let id = match id {
                Some(id) => Some(id.clone()),
                None if used.contains(&index) => {
                    let access = access_token(&state, &account_id).await?;
                    let answer = client.create_folder(&access, name.clone()).await?;
                    outcome.folders_created += 1;
                    entry_id(&answer).map(str::to_string)
                }
                None => None,
            };
            ready.push(id.map(|id| (id, name.clone())));
        }
        for request in import_requests(sealed, &in_folder, &ready, CHUNK) {
            state.touch();
            let access = access_token(&state, &account_id).await?;
            client.import_ciphers(&access, &request).await?;
            outcome.imported += request.ciphers.len();
            let done = outcome.imported;
            let _ = app.emit("import-progress", Progress { done, total });
        }
        Ok::<(), Failure>(())
    };
    if let Err(error) = run.await {
        outcome.error = Some(error);
    }
    outcome.skipped = skipped;
    tracing::info!(
        items = outcome.imported,
        skipped = outcome.skipped.len(),
        stopped = outcome.error.is_some(),
        "vault imported from a file"
    );
    if outcome.imported > 0 || outcome.folders_created > 0 {
        if let Err(error) = sync_account(&app, &account_id).await {
            tracing::warn!(kind = error.kind(), "sync after the import failed");
        }
    }
    emit_status(&app);
    Ok(outcome)
}

/// Bitwarden's password-protected JSON export, opened: the unencrypted
/// export, for the page to read like any other.
#[tauri::command]
pub(crate) async fn import_open_bitwarden(text: String, password: String) -> Result<String> {
    let text = Zeroizing::new(text);
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || {
        if import::is_account_bound(&text) {
            return Err(Failure::new(
                "import-account-bound",
                "This export is encrypted for the account that made it.",
            ));
        }
        import::open_protected_export(&text, &password)
            .map(|plain| plain.to_string())
            .map_err(file_failure)
    })
    .await
    .map_err(|e| Failure::new("crypto", e.to_string()))?
}

/// KeePass: Argon2d or Argon2id over the file's composite key.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn import_kdbx_argon2(
    id: bool,
    version: u32,
    key: Vec<u8>,
    salt: Vec<u8>,
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Vec<u8>> {
    let key = Zeroizing::new(key);
    tauri::async_runtime::spawn_blocking(move || {
        import::kdbx_argon2(id, version, &key, &salt, memory_kib, iterations, lanes)
            .map(|out| out.to_vec())
            .map_err(file_failure)
    })
    .await
    .map_err(|e| Failure::new("crypto", e.to_string()))?
}

/// KeePass: AES-KDF over the file's composite key.
#[tauri::command]
pub(crate) async fn import_kdbx_aes_kdf(
    key: Vec<u8>,
    seed: Vec<u8>,
    rounds: u64,
) -> Result<Vec<u8>> {
    let key = Zeroizing::new(key);
    tauri::async_runtime::spawn_blocking(move || {
        import::kdbx_aes_kdf(&key, &seed, rounds)
            .map(|out| out.to_vec())
            .map_err(file_failure)
    })
    .await
    .map_err(|e| Failure::new("crypto", e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::too_long;
    use uwulock_bitwarden::wire::{CipherRequest, FieldRequest, LoginRequest};

    #[test]
    fn values_longer_than_bitwarden_takes_stay_out() {
        let mut request = CipherRequest {
            name: "x".repeat(1_000),
            ..CipherRequest::default()
        };
        assert!(!too_long(&request));
        request.name.push('x');
        assert!(too_long(&request));
        request.name = "x".into();
        request.login = Some(LoginRequest {
            password: Some("p".repeat(5_001)),
            ..LoginRequest::default()
        });
        assert!(too_long(&request));
        request.login = None;
        request.fields = Some(vec![FieldRequest {
            value: Some("v".repeat(5_000)),
            ..FieldRequest::default()
        }]);
        assert!(!too_long(&request));
        request.notes = Some("n".repeat(10_001));
        assert!(too_long(&request));
    }
}
