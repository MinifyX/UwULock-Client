//! Moving a vault from Bitwarden (its cloud, or self-hosted) or Vaultwarden
//! to a UwULock Server.
//!
//! Bitwarden's export leaves attachments, Sends and organisations out. This
//! logs in to both servers and carries everything over the APIs instead:
//! folders, items of every kind (passkeys, custom fields, password history,
//! favourites and the master password re-prompt included), attachments,
//! Sends, and organisations with their collections. Everything is decrypted
//! here, on this device, and encrypted again for the target: every item gets
//! a fresh item key, every attachment a fresh attachment key, every Send a
//! fresh seed (and so a new link), every organisation a fresh key.
//!
//! Organisations become families (UwULock-Server's `docs/uwu-api.md` §16)
//! when the target has the feature and lets this account make them: a
//! Bitwarden organisation made through `POST /api/organizations` with
//! `planType` 22, its key made here and wrapped for the account's public key,
//! like Bitwarden's web vault does. Otherwise an organisation's items go into
//! a personal folder named after it. Members never move: they are invited
//! again in the web vault.
//!
//! A move is a list of small steps ([`Mover::step`]), each one object. What
//! has moved is written down in a [`Journal`] (source id → target id) that
//! the caller keeps after every step, so an interrupted move continues where
//! it stopped and a second one moves only what is new. The journal knows
//! ids, never contents.
//!
//! What can't move, and why, is in the [`Preview`] before anything is written:
//! the password of a Send is only a hash (the Send moves without one), a file
//! Send with a password can't be fetched at all, items in the trash stay, and
//! items the person may only read in an organisation stay there too.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use zeroize::Zeroizing;

use crate::api::{parse_sync, text_of, AttachmentRequest, Client, Server, Session};
use crate::crypto::{
    decrypt_file, encrypt_file, generate_send_seed, send_key, wrap_for, EncString, PrivateKey,
    PublicKey, SymmetricKey,
};
use crate::vault::{Item, Vault};
use crate::wire::{self, lowercase_keys};
use crate::Error;

/// Bitwarden's `planType` for "Families (annually)": a family on UwULock Server.
pub const FAMILY_PLAN: u32 = 22;

/// What has moved already, per source account: source id → target id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Journal {
    pub version: u32,
    /// By [`source_key`].
    pub sources: BTreeMap<String, Moved>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Moved {
    pub folders: BTreeMap<String, String>,
    pub organizations: BTreeMap<String, OrgMove>,
    pub collections: BTreeMap<String, String>,
    pub items: BTreeMap<String, String>,
    /// By `<source item id>/<source attachment id>`.
    pub attachments: BTreeMap<String, String>,
    pub sends: BTreeMap<String, String>,
}

/// Where an organisation went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum OrgMove {
    /// A family of its own, by its id.
    Family { id: String },
    /// A personal folder named after it, by its id.
    Folder { id: String },
}

/// Which source account a part of the journal is about.
pub fn source_key(server: &Server, email: &str) -> String {
    format!(
        "{}|{}",
        server.label(),
        crate::crypto::normalize_email(email)
    )
}

/// The source account, logged in: kept in memory for the move only.
pub struct Source {
    client: Client,
    session: Session,
    user_key: SymmetricKey,
    email: String,
}

impl Source {
    pub fn new(client: Client, session: Session, user_key: SymmetricKey, email: &str) -> Self {
        Source {
            client,
            session,
            user_key,
            email: crate::crypto::normalize_email(email),
        }
    }

    pub fn server(&self) -> &Server {
        self.client.server()
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    /// A token for the next call, renewed when the old one runs out: a move
    /// with large files can take longer than a token lives.
    async fn token(&mut self) -> Result<Zeroizing<String>, Error> {
        if self.session.is_expiring() {
            if let Some(refresh) = self.session.refresh_token.clone() {
                self.session = self.client.refresh(&refresh).await?;
            }
        }
        Ok(self.session.access_token.clone())
    }
}

/// The account the vault moves to: the app's own UwULock account.
pub struct Target {
    pub client: Client,
    pub user_key: SymmetricKey,
    pub email: String,
}

/// How many of one kind of thing there are, and how many still have to move.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    /// Everything of this kind that can move.
    pub total: usize,
    /// Moved in an earlier run.
    pub moved: usize,
    /// To move now.
    pub todo: usize,
}

/// Something that won't move, or won't move as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// `trash`: items in the trash stay behind.
    /// `broken`: items or organisations that didn't decrypt.
    /// `read-only`: organisation items the person may only read (or whose
    /// password is hidden from them) stay behind.
    /// `org-members`: members don't move; invite them again in the web vault.
    /// `orgs-as-folders`: no families here, so organisations become folders.
    /// `send-password`: Sends that had a password move without one: only its
    /// hash is on the server.
    /// `send-emails`: Sends only for given addresses move without that limit.
    /// `send-file-locked`: file Sends with a password, switched off or used
    /// up can't be fetched and stay behind.
    /// `send-expired`: Sends past their expiry stay behind.
    /// `send-file-counted`: fetching a file Send counts as one opening at the
    /// source.
    /// `too-large`: files larger than the target allows stay behind.
    pub code: &'static str,
    pub count: usize,
}

/// What a move will do, before it does anything.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub source: String,
    pub source_email: String,
    pub target: String,
    pub target_email: String,
    /// Organisations become families (rather than folders).
    pub families: bool,
    pub folders: Count,
    pub organizations: Count,
    pub collections: Count,
    pub items: Count,
    pub attachments: Count,
    pub sends: Count,
    /// Of the attachments and Send files still to move, encrypted.
    pub file_bytes: u64,
    pub notices: Vec<Notice>,
}

/// How many objects of each kind moved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    pub folders: usize,
    pub organizations: usize,
    pub collections: usize,
    pub items: usize,
    pub attachments: usize,
    pub sends: usize,
}

/// One object that didn't move in this run; the next run tries again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failed {
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub moved: Tally,
    pub failed: Vec<Failed>,
    /// Organisations that were meant to become families but became folders
    /// (the server said no).
    pub orgs_as_folders: usize,
}

/// Where a move stands after a step.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// What the last step moved: `folder`, `organization`, `collection`,
    /// `item`, `attachment`, `send`.
    pub kind: &'static str,
}

#[derive(Debug, Clone)]
pub enum Step {
    Working(Progress),
    Finished(Summary),
}

#[derive(Debug, Clone)]
enum Task {
    Folder(String),
    Org(String),
    Collection(String),
    /// By index into the source vault's items.
    Item(usize),
    Attachment {
        item: usize,
        attachment: String,
    },
    /// By index into the source sync's Sends.
    Send(usize),
}

impl Task {
    fn kind(&self) -> &'static str {
        match self {
            Task::Folder(_) => "folder",
            Task::Org(_) => "organization",
            Task::Collection(_) => "collection",
            Task::Item(_) => "item",
            Task::Attachment { .. } => "attachment",
            Task::Send(_) => "send",
        }
    }
}

/// What an item may do in its organisation, from the raw sync.
#[derive(Debug, Clone, Copy)]
struct Rights {
    edit: bool,
    view_password: bool,
}

/// An item on the target: its organisation, its wrapped item key, the ids
/// of its attachments.
#[derive(Default)]
struct TargetCipher {
    org: Option<String>,
    key: Option<String>,
    attachments: HashSet<String>,
}

/// The target's state, as far as a move needs it: what exists, and the keys.
#[derive(Default)]
struct Snapshot {
    folders: HashSet<String>,
    ciphers: HashMap<String, TargetCipher>,
    sends: HashSet<String>,
    /// Organisation id → (its key if it opened, the membership id).
    orgs: HashMap<String, (Option<SymmetricKey>, Option<String>)>,
    /// Collection id → organisation id.
    collections: HashMap<String, String>,
}

impl Snapshot {
    fn from_sync(text: &str, private: Option<&PrivateKey>) -> Result<Snapshot, Error> {
        let sync = parse_sync(text)?;
        let raw = raw_sync(text);
        let mut orgs = HashMap::new();
        for org in &sync.profile.organizations {
            let key = match (&org.key, private) {
                (Some(key), Some(private)) => key
                    .parse::<EncString>()
                    .and_then(|k| k.decrypt_key_rsa(private))
                    .ok(),
                _ => None,
            };
            let membership = raw["profile"]["organizations"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["id"].as_str() == Some(org.id.as_str()))
                .and_then(|o| o["organizationuserid"].as_str())
                .map(str::to_string);
            orgs.insert(org.id.clone(), (key, membership));
        }
        Ok(Snapshot {
            folders: sync.folders.iter().map(|f| f.id.clone()).collect(),
            ciphers: sync
                .ciphers
                .iter()
                .map(|c| {
                    let cipher = TargetCipher {
                        org: c.organization_id.clone(),
                        key: c.key.clone(),
                        attachments: c.attachments.iter().map(|a| a.id.clone()).collect(),
                    };
                    (c.id.clone(), cipher)
                })
                .collect(),
            sends: sync.sends.iter().map(|s| s.id.clone()).collect(),
            orgs,
            collections: sync
                .collections
                .iter()
                .map(|c| (c.id.clone(), c.organization_id.clone()))
                .collect(),
        })
    }
}

fn raw_sync(text: &str) -> Value {
    serde_json::from_str(text)
        .map(lowercase_keys)
        .unwrap_or(Value::Null)
}

/// A move, prepared: both vaults read, the plan made. [`Mover::step`] runs it.
pub struct Mover {
    source: Source,
    target: Target,
    target_private: Option<PrivateKey>,
    target_public: PublicKey,
    snapshot: Snapshot,
    /// Keys of organisations made in this run, and of those found.
    org_keys: HashMap<String, SymmetricKey>,
    /// Keys of items made in this run, by target id.
    item_keys: HashMap<String, SymmetricKey>,
    vault: Vault,
    sync: wire::Sync,
    key: String,
    journal: Journal,
    tasks: Vec<Task>,
    next: usize,
    /// Families that may still be made; `None` for no limit.
    family_room: Option<usize>,
    /// The most a file may weigh as it comes from the source: the target's
    /// file limit (Bitwarden's 500 MiB when it names none), plus what
    /// encryption adds. A source can't make the app read more.
    max_download: u64,
    summary: Summary,
    preview: Preview,
}

/// Bitwarden's own limit for a file, for a server that names none.
const DEFAULT_MAX_FILE: u64 = 500 * 1024 * 1024;

impl Mover {
    /// Reads both vaults and plans the move. Nothing is written yet.
    ///
    /// `target_token` is a fresh session of the target account; `now` is the
    /// time as RFC 3339, for Sends that ran out.
    pub async fn prepare(
        mut source: Source,
        target: Target,
        target_token: &str,
        mut journal: Journal,
        now: &str,
    ) -> Result<Mover, Error> {
        if source.server() == target.client.server()
            && source.email == crate::crypto::normalize_email(&target.email)
        {
            return Err(Error::Refused(
                "that is the account the vault would move to".into(),
            ));
        }
        let info = target.client.uwu_info().await?.ok_or_else(|| {
            Error::Refused("the account to move to isn't on a UwULock Server".into())
        })?;
        let max_file = info.limits.as_ref().and_then(|l| l.max_file_bytes);
        let (families, family_room) = if info.has("families") {
            families_allowed(&target.client, target_token).await
        } else {
            (false, Some(0))
        };

        let target_text = target.client.sync(target_token).await?;
        let target_sync = parse_sync(&target_text)?;
        let target_private = match &target_sync.profile.private_key {
            Some(text) => {
                let der = text.parse::<EncString>()?.decrypt(&target.user_key)?;
                Some(PrivateKey::from_der(&der)?)
            }
            None => None,
        };
        let target_public = target_private
            .as_ref()
            .map(PrivateKey::public)
            .ok_or_else(|| Error::Refused("the account to move to has no key pair yet".into()))?;
        let snapshot = Snapshot::from_sync(&target_text, target_private.as_ref())?;

        let token = source.token().await?;
        let source_text = source.client.sync(&token).await?;
        let sync = parse_sync(&source_text)?;
        let vault = Vault::open(&sync, &source.user_key)?;
        let raw = raw_sync(&source_text);

        let key = source_key(source.server(), &source.email);
        let moved = journal.sources.entry(key.clone()).or_default();
        forget_what_is_gone(moved, &snapshot);
        journal.version = 1;

        let mut org_keys = HashMap::new();
        for (id, (key, _)) in &snapshot.orgs {
            if let Some(key) = key {
                org_keys.insert(id.clone(), key.clone());
            }
        }

        let mut mover = Mover {
            preview: Preview {
                source: source.server().label(),
                source_email: source.email.clone(),
                target: target.client.server().label(),
                target_email: target.email.clone(),
                families,
                folders: Count::default(),
                organizations: Count::default(),
                collections: Count::default(),
                items: Count::default(),
                attachments: Count::default(),
                sends: Count::default(),
                file_bytes: 0,
                notices: Vec::new(),
            },
            source,
            target,
            target_private,
            target_public,
            snapshot,
            org_keys,
            item_keys: HashMap::new(),
            vault,
            sync,
            key,
            journal,
            tasks: Vec::new(),
            next: 0,
            family_room,
            // Type 2 of an encrypted file: a byte, the IV, the MAC and at most
            // a block of padding.
            max_download: max_file.unwrap_or(DEFAULT_MAX_FILE).saturating_add(1024),
            summary: Summary::default(),
        };
        mover.plan(&raw, now, max_file);
        Ok(mover)
    }

    fn moved(&self) -> &Moved {
        self.journal
            .sources
            .get(&self.key)
            .expect("made in prepare")
    }

    fn moved_mut(&mut self) -> &mut Moved {
        self.journal.sources.entry(self.key.clone()).or_default()
    }

    /// The list of steps, and the preview that describes it.
    fn plan(&mut self, raw: &Value, now: &str, max_file: Option<u64>) {
        let moved = self.moved().clone();
        let mut tasks = Vec::new();
        let mut notices: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut note = |code: &'static str, n: usize| {
            if n > 0 {
                *notices.entry(code).or_default() += n;
            }
        };
        let mut preview = self.preview.clone();
        let mut file_bytes = 0u64;

        // Folders.
        for folder in &self.vault.folders {
            count(&mut preview.folders, moved.folders.contains_key(&folder.id));
            if !moved.folders.contains_key(&folder.id) {
                tasks.push(Task::Folder(folder.id.clone()));
            }
        }

        // Organisations and their collections.
        let mut as_folders = 0;
        let mut room = self.family_room;
        for org in &self.vault.organizations {
            let done = moved.organizations.get(&org.id);
            count(&mut preview.organizations, done.is_some());
            let family = match done {
                Some(OrgMove::Family { .. }) => true,
                Some(OrgMove::Folder { .. }) => false,
                None => {
                    tasks.push(Task::Org(org.id.clone()));
                    match &mut room {
                        None => true,
                        Some(0) => false,
                        Some(n) => {
                            *n -= 1;
                            true
                        }
                    }
                }
            };
            if !family {
                as_folders += 1;
                continue;
            }
            for collection in self
                .vault
                .collections
                .iter()
                .filter(|c| c.organization_id == org.id)
            {
                let done = moved.collections.contains_key(&collection.id);
                count(&mut preview.collections, done);
                if !done {
                    tasks.push(Task::Collection(collection.id.clone()));
                }
            }
        }
        note("orgs-as-folders", as_folders);
        let members_stay = self.vault.organizations.len() - as_folders;
        note("org-members", members_stay);
        note("broken", self.vault.skipped);

        // Items, and their attachments right after each.
        let rights = rights_of(raw);
        for (index, item) in self.vault.items.iter().enumerate() {
            if item.deleted {
                note("trash", 1);
                continue;
            }
            if item.broken {
                note("broken", 1);
                continue;
            }
            if item.organization_id.is_some()
                && rights
                    .get(&item.id)
                    .is_some_and(|r| !r.edit || !r.view_password)
            {
                note("read-only", 1);
                continue;
            }
            let done = moved.items.contains_key(&item.id);
            count(&mut preview.items, done);
            if !done {
                tasks.push(Task::Item(index));
            }
            let Some(cipher) = self.sync.ciphers.iter().find(|c| c.id == item.id) else {
                continue;
            };
            for attachment in &cipher.attachments {
                let size = attachment
                    .size
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                if max_file.is_some_and(|max| size > max) {
                    note("too-large", 1);
                    continue;
                }
                let done = moved
                    .attachments
                    .contains_key(&attachment_key(&item.id, &attachment.id));
                count(&mut preview.attachments, done);
                if !done {
                    file_bytes += size;
                    tasks.push(Task::Attachment {
                        item: index,
                        attachment: attachment.id.clone(),
                    });
                }
            }
        }

        // Sends.
        let raw_sends = raw["sends"].as_array().cloned().unwrap_or_default();
        let before_now = |date: &Option<String>| {
            date.as_deref()
                .is_some_and(|d| d.get(..19).unwrap_or(d) < now.get(..19).unwrap_or(now))
        };
        for (index, send) in self.sync.sends.iter().enumerate() {
            if before_now(&send.deletion_date) || before_now(&send.expiration_date) {
                note("send-expired", 1);
                continue;
            }
            if send.kind > 1 || send.key.is_none() {
                note("broken", 1);
                continue;
            }
            let has_password = send.password.as_deref().is_some_and(|p| !p.is_empty());
            let done = moved.sends.contains_key(&send.id);
            if send.kind == 1 && !done {
                let used_up = send
                    .max_access_count
                    .is_some_and(|max| send.access_count.unwrap_or(0) >= max);
                if has_password || send.disabled == Some(true) || used_up {
                    note("send-file-locked", 1);
                    continue;
                }
                let size = send
                    .file
                    .as_ref()
                    .and_then(|f| f.size.as_deref())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                if max_file.is_some_and(|max| size > max) {
                    note("too-large", 1);
                    continue;
                }
                file_bytes += size;
                note("send-file-counted", 1);
            }
            count(&mut preview.sends, done);
            if done {
                continue;
            }
            if has_password {
                note("send-password", 1);
            }
            let emails = raw_sends
                .iter()
                .find(|s| s["id"].as_str() == Some(send.id.as_str()))
                .and_then(|s| s["emails"].as_str())
                .is_some_and(|e| !e.trim().is_empty());
            if emails {
                note("send-emails", 1);
            }
            tasks.push(Task::Send(index));
        }

        preview.file_bytes = file_bytes;
        preview.notices = notices
            .into_iter()
            .map(|(code, count)| Notice { code, count })
            .collect();
        self.preview = preview;
        self.tasks = tasks;
    }

    pub fn preview(&self) -> &Preview {
        &self.preview
    }

    /// What has moved so far, to keep after every step.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    /// What moved so far in this run, and what didn't.
    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    /// The next step. An object that fails is noted in the summary and left
    /// for the next run; a session that ran out, a server out of reach or
    /// one that asks for a break ends the move with an error (the journal
    /// keeps what moved until then).
    pub async fn step(&mut self, target_token: &str) -> Result<Step, Error> {
        let Some(task) = self.tasks.get(self.next).cloned() else {
            return Ok(Step::Finished(self.summary.clone()));
        };
        let kind = task.kind();
        match self.run(&task, target_token).await {
            Ok(()) => {}
            Err(error) if ends_the_run(&error) => return Err(error),
            Err(error) => {
                tracing::warn!(kind, %error, "didn't move");
                self.summary.failed.push(Failed {
                    kind,
                    message: error.to_string(),
                });
            }
        }
        self.next += 1;
        Ok(Step::Working(Progress {
            done: self.next,
            total: self.tasks.len(),
            kind,
        }))
    }

    async fn run(&mut self, task: &Task, token: &str) -> Result<(), Error> {
        match task {
            Task::Folder(id) => self.move_folder(id, token).await,
            Task::Org(id) => self.move_org(id, token).await,
            Task::Collection(id) => self.move_collection(id, token).await,
            Task::Item(index) => self.move_item(*index, token).await,
            Task::Attachment { item, attachment } => {
                self.move_attachment(*item, attachment, token).await
            }
            Task::Send(index) => self.move_send(*index, token).await,
        }
    }

    async fn move_folder(&mut self, id: &str, token: &str) -> Result<(), Error> {
        if self.moved().folders.contains_key(id) {
            return Ok(());
        }
        let name = self
            .vault
            .folders
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        let made = self
            .target
            .client
            .create_folder(
                token,
                EncString::encrypt(name.as_bytes(), &self.target.user_key).to_string(),
            )
            .await?;
        let new = id_of(&made)?;
        self.snapshot.folders.insert(new.clone());
        self.moved_mut().folders.insert(id.to_string(), new);
        self.summary.moved.folders += 1;
        Ok(())
    }

    async fn move_org(&mut self, id: &str, token: &str) -> Result<(), Error> {
        if self.moved().organizations.contains_key(id) {
            return Ok(());
        }
        let name = self
            .vault
            .organizations
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.name.clone())
            .unwrap_or_default();
        let wants_family = match &mut self.family_room {
            None => true,
            Some(0) => false,
            Some(n) => {
                *n -= 1;
                true
            }
        };
        if wants_family {
            match self.make_family(id, &name, token).await {
                Ok(()) => return Ok(()),
                Err(error) if ends_the_run(&error) => return Err(error),
                Err(error) => {
                    // No family after all (a limit, a setting): a folder instead.
                    tracing::warn!(%error, "no family; the organisation becomes a folder");
                    self.family_room = Some(0);
                    self.summary.orgs_as_folders += 1;
                }
            }
        }
        let made = self
            .target
            .client
            .create_folder(
                token,
                EncString::encrypt(name.as_bytes(), &self.target.user_key).to_string(),
            )
            .await?;
        let folder = id_of(&made)?;
        self.snapshot.folders.insert(folder.clone());
        self.moved_mut()
            .organizations
            .insert(id.to_string(), OrgMove::Folder { id: folder });
        self.summary.moved.organizations += 1;
        Ok(())
    }

    /// A family for a source organisation: its key and key pair made here,
    /// the key wrapped for the account's public key (type 4), as Bitwarden's
    /// web vault makes an organisation. The server makes the first collection.
    async fn make_family(&mut self, id: &str, name: &str, token: &str) -> Result<(), Error> {
        let org_key = SymmetricKey::generate();
        let pair = PrivateKey::generate()?;
        let public = pair.public().to_der()?;
        let private = pair.to_der()?;
        let first = self
            .vault
            .collections
            .iter()
            .find(|c| c.organization_id == id)
            .cloned();
        let collection_name = first
            .as_ref()
            .map_or("Default collection", |c| c.name.as_str());
        let request = json!({
            "name": name.chars().take(50).collect::<String>(),
            "billingEmail": self.target.email,
            "planType": FAMILY_PLAN,
            "key": wrap_for(&self.target_public, &org_key)?.to_string(),
            "keys": {
                "publicKey": base64_std(&public),
                "encryptedPrivateKey": EncString::encrypt(&private, &org_key).to_string(),
            },
            "collectionName": EncString::encrypt(collection_name.as_bytes(), &org_key).to_string(),
        });
        let made = self
            .target
            .client
            .create_organization(token, &request)
            .await?;
        let new = id_of(&made)?;
        self.org_keys.insert(new.clone(), org_key);
        self.moved_mut()
            .organizations
            .insert(id.to_string(), OrgMove::Family { id: new.clone() });
        self.summary.moved.organizations += 1;

        // The membership (for the collections to come) and the first
        // collection, as the server made them.
        let text = self.target.client.sync(token).await?;
        self.snapshot = Snapshot::from_sync(&text, self.target_private.as_ref())?;
        if let Some(first) = first {
            let made_first = self
                .snapshot
                .collections
                .iter()
                .find(|(_, org)| **org == new)
                .map(|(collection, _)| collection.clone());
            if let Some(made_first) = made_first {
                self.moved_mut().collections.insert(first.id, made_first);
                self.summary.moved.collections += 1;
            }
        }
        Ok(())
    }

    async fn move_collection(&mut self, id: &str, token: &str) -> Result<(), Error> {
        if self.moved().collections.contains_key(id) {
            return Ok(());
        }
        let Some(collection) = self.vault.collections.iter().find(|c| c.id == id).cloned() else {
            return Ok(());
        };
        let Some(OrgMove::Family { id: org }) = self
            .moved()
            .organizations
            .get(&collection.organization_id)
            .cloned()
        else {
            // Its organisation became a folder, or didn't move.
            return Ok(());
        };
        let org_key = self.target_org_key(&org)?;
        let users = match self.snapshot.orgs.get(&org).and_then(|(_, m)| m.clone()) {
            Some(membership) => json!([{ "id": membership, "readOnly": false,
                                         "hidePasswords": false, "manage": true }]),
            None => json!([]),
        };
        let request = json!({
            "name": EncString::encrypt(collection.name.as_bytes(), &org_key).to_string(),
            "externalId": null,
            "groups": [],
            "users": users,
        });
        let made = self
            .target
            .client
            .create_collection(token, &org, &request)
            .await?;
        let new = id_of(&made)?;
        self.snapshot.collections.insert(new.clone(), org);
        self.moved_mut().collections.insert(id.to_string(), new);
        self.summary.moved.collections += 1;
        Ok(())
    }

    fn target_org_key(&self, org: &str) -> Result<SymmetricKey, Error> {
        self.org_keys
            .get(org)
            .cloned()
            .ok_or_else(|| Error::Crypto("the new organisation's key isn't at hand".into()))
    }

    async fn move_item(&mut self, index: usize, token: &str) -> Result<(), Error> {
        let item = self.vault.items[index].clone();
        if self.moved().items.contains_key(&item.id) {
            return Ok(());
        }
        let source_outer = self
            .vault
            .outer_key(item.organization_id.as_deref(), &self.source.user_key)?
            .clone();
        let old_key = item.key.clone().unwrap_or(source_outer);

        let moved = self.moved().clone();
        let mut folder = item
            .folder_id
            .as_ref()
            .and_then(|f| moved.folders.get(f))
            .cloned();
        let (organization, collections, outer) = match &item.organization_id {
            None => (None, Vec::new(), self.target.user_key.clone()),
            Some(org) => match moved.organizations.get(org) {
                Some(OrgMove::Family { id }) => {
                    let mut collections: Vec<String> = item
                        .collection_ids
                        .iter()
                        .filter_map(|c| moved.collections.get(c).cloned())
                        .collect();
                    if collections.is_empty() {
                        // Nowhere it was is here: the family's first collection.
                        collections.extend(
                            self.snapshot
                                .collections
                                .iter()
                                .filter(|(_, o)| *o == id)
                                .map(|(c, _)| c.clone())
                                .min(),
                        );
                    }
                    (Some(id.clone()), collections, self.target_org_key(id)?)
                }
                Some(OrgMove::Folder { id }) => {
                    folder = Some(id.clone());
                    (None, Vec::new(), self.target.user_key.clone())
                }
                None => {
                    return Err(Error::Refused(
                        "its organisation didn't move, so the item waits for the next run".into(),
                    ))
                }
            },
        };

        let new_key = SymmetricKey::generate();
        let fresh = reseal_item(&item, &old_key, &new_key, &outer)?;
        let mut fresh = fresh;
        fresh.folder_id = folder;
        fresh.organization_id = organization.clone();
        fresh.collection_ids = collections.clone();
        let request = fresh.seal(&outer)?;
        let made = self
            .target
            .client
            .create_cipher(token, request, &collections)
            .await?;
        let new = id_of(&made)?;
        self.snapshot.ciphers.insert(
            new.clone(),
            TargetCipher {
                org: organization,
                key: fresh.wrapped_key.clone(),
                attachments: HashSet::new(),
            },
        );
        self.item_keys.insert(new.clone(), new_key);
        self.moved_mut().items.insert(item.id.clone(), new);
        self.summary.moved.items += 1;
        Ok(())
    }

    /// The key a moved item keeps its values (and its attachments' keys) under.
    fn target_item_key(&self, id: &str) -> Result<SymmetricKey, Error> {
        if let Some(key) = self.item_keys.get(id) {
            return Ok(key.clone());
        }
        let TargetCipher {
            org, key: wrapped, ..
        } = self
            .snapshot
            .ciphers
            .get(id)
            .ok_or_else(|| Error::Refused("the moved item isn't there any more".into()))?;
        let outer = match org {
            Some(org) => self.target_org_key(org)?,
            None => self.target.user_key.clone(),
        };
        match wrapped {
            Some(wrapped) => wrapped.parse::<EncString>()?.decrypt_key(&outer),
            None => Ok(outer),
        }
    }

    async fn move_attachment(
        &mut self,
        index: usize,
        attachment_id: &str,
        token: &str,
    ) -> Result<(), Error> {
        let item = &self.vault.items[index];
        let journal_key = attachment_key(&item.id, attachment_id);
        if self.moved().attachments.contains_key(&journal_key) {
            return Ok(());
        }
        let Some(target_item) = self.moved().items.get(&item.id).cloned() else {
            return Err(Error::Refused(
                "its item didn't move, so the attachment waits for the next run".into(),
            ));
        };
        let source_outer = self
            .vault
            .outer_key(item.organization_id.as_deref(), &self.source.user_key)?
            .clone();
        let old_item_key = item.key.clone().unwrap_or(source_outer);
        let source_item = item.id.clone();
        let attachment = self
            .sync
            .ciphers
            .iter()
            .find(|c| c.id == source_item)
            .and_then(|c| c.attachments.iter().find(|a| a.id == attachment_id))
            .ok_or_else(|| Error::Refused("the attachment isn't in the source any more".into()))?;
        let old_file_key = match &attachment.key {
            Some(key) => key.parse::<EncString>()?.decrypt_key(&old_item_key)?,
            None => old_item_key.clone(),
        };
        let name = match &attachment.file_name {
            Some(name) => name.parse::<EncString>()?.decrypt(&old_item_key)?,
            None => Zeroizing::new(b"attachment".to_vec()),
        };
        let listed_url = attachment.url.clone();

        // Fetched with a fresh link; the one from the sync may have run out.
        let source_token = self.source.token().await?;
        let url = match self
            .source
            .client
            .attachment_url(&source_token, &source_item, attachment_id)
            .await
        {
            Ok(url) => url,
            Err(error) if ends_the_run(&error) => return Err(error),
            Err(error) => listed_url.ok_or(error)?,
        };
        let encrypted = self.source.client.download(&url, self.max_download).await?;
        let plain = decrypt_file(&encrypted, &old_file_key)?;
        drop(encrypted);

        let item_key = self.target_item_key(&target_item)?;
        let file_key = SymmetricKey::generate();
        let sealed = encrypt_file(&plain, &file_key);
        drop(plain);
        let file_name = EncString::encrypt(&name, &item_key).to_string();
        let upload = self
            .target
            .client
            .announce_attachment(
                token,
                &target_item,
                &AttachmentRequest {
                    key: EncString::encrypt(&file_key.to_bytes(), &item_key).to_string(),
                    file_name: file_name.clone(),
                    file_size: sealed.len() as u64,
                    admin_request: false,
                },
            )
            .await?;
        if let Err(error) = self
            .target
            .client
            .upload_file(token, &upload, &file_name, &sealed)
            .await
        {
            // Not half an attachment: the next run tries again from the start.
            if let Err(cleanup) = self
                .target
                .client
                .delete_attachment(token, &target_item, &upload.id)
                .await
            {
                tracing::warn!(%cleanup, "couldn't remove a half-made attachment");
            }
            return Err(error);
        }
        self.moved_mut().attachments.insert(journal_key, upload.id);
        self.summary.moved.attachments += 1;
        Ok(())
    }

    async fn move_send(&mut self, index: usize, token: &str) -> Result<(), Error> {
        let send = &self.sync.sends[index];
        if self.moved().sends.contains_key(&send.id) {
            return Ok(());
        }
        let seed = uwulock_core::send::open_seed(
            send.key.as_deref().unwrap_or_default(),
            &self.source.user_key,
        )?;
        let old_key = send_key(&seed)?;
        let open = |value: &Option<String>| -> Result<Option<Zeroizing<Vec<u8>>>, Error> {
            match value.as_deref().filter(|v| !v.is_empty()) {
                None => Ok(None),
                Some(text) => Ok(Some(text.parse::<EncString>()?.decrypt(&old_key)?)),
            }
        };
        let name = open(&send.name)?.unwrap_or_default();
        let notes = open(&send.notes)?;

        let new_seed = generate_send_seed();
        let new_key = send_key(new_seed.as_ref())?;
        let seal = |plain: &[u8]| EncString::encrypt(plain, &new_key).to_string();
        let mut request = json!({
            "type": send.kind,
            "name": seal(&name),
            "notes": notes.as_ref().map(|n| seal(n)),
            "key": EncString::encrypt(new_seed.as_ref(), &self.target.user_key).to_string(),
            "maxAccessCount": send.max_access_count,
            "expirationDate": send.expiration_date,
            "deletionDate": send.deletion_date,
            "text": null,
            "file": null,
            "password": null,
            "emails": null,
            "authType": 2,
            "disabled": send.disabled.unwrap_or(false),
            "hideEmail": send.hide_email.unwrap_or(false),
        });
        let source_id = send.id.clone();

        let new = if send.kind == 0 {
            let text = send.text.as_ref();
            let plain = open(&text.and_then(|t| t.text.clone()))?.unwrap_or_default();
            request["text"] = json!({
                "text": seal(&plain),
                "hidden": text.and_then(|t| t.hidden).unwrap_or(false),
            });
            let made = self.target.client.create_send(token, &request).await?;
            id_of(&made)?
        } else {
            let file = send
                .file
                .as_ref()
                .ok_or_else(|| Error::Refused("the file Send has no file".into()))?;
            let file_id = file
                .id
                .clone()
                .ok_or_else(|| Error::Refused("the file Send has no file".into()))?;
            let file_name = open(&file.file_name)?.unwrap_or_default();
            let access_id = send.access_id.clone();
            let url = self
                .source
                .client
                .send_file_url(&source_id, access_id.as_deref(), &file_id)
                .await?;
            let encrypted = self.source.client.download(&url, self.max_download).await?;
            let plain = decrypt_file(&encrypted, &old_key)?;
            drop(encrypted);
            let sealed = encrypt_file(&plain, &new_key);
            drop(plain);
            let sealed_name = seal(&file_name);
            request["file"] = json!({ "fileName": sealed_name });
            request["fileLength"] = json!(sealed.len());
            let upload = self.target.client.create_file_send(token, &request).await?;
            if let Err(error) = self
                .target
                .client
                .upload_file(token, &upload, &sealed_name, &sealed)
                .await
            {
                if let Err(cleanup) = self.target.client.delete_send(token, &upload.id).await {
                    tracing::warn!(%cleanup, "couldn't remove a half-made Send");
                }
                return Err(error);
            }
            upload.id
        };
        self.snapshot.sends.insert(new.clone());
        self.moved_mut().sends.insert(source_id, new);
        self.summary.moved.sends += 1;
        Ok(())
    }
}

/// Families this account may still make on the target: `(false, Some(0))`
/// when none. `GET /uwu/v1/account` says `families: { mayCreate, owned,
/// perUser }` (§16.4).
async fn families_allowed(client: &Client, token: &str) -> (bool, Option<usize>) {
    let account = match client.uwu_get(token, "/account").await {
        Ok(account) => account,
        Err(error) => {
            tracing::warn!(%error, "no account details; organisations become folders");
            return (false, Some(0));
        }
    };
    let families = &account["families"];
    if families["mayCreate"].as_bool() != Some(true) {
        return (false, Some(0));
    }
    let room = match (families["perUser"].as_u64(), families["owned"].as_u64()) {
        (Some(per_user), owned) => Some(per_user.saturating_sub(owned.unwrap_or(0)) as usize),
        (None, _) => None,
    };
    (room != Some(0), room)
}

/// Drops journal entries whose target is gone (deleted on the target since):
/// those move again.
fn forget_what_is_gone(moved: &mut Moved, snapshot: &Snapshot) {
    moved
        .folders
        .retain(|_, target| snapshot.folders.contains(target));
    moved.organizations.retain(|_, target| match target {
        OrgMove::Family { id } => snapshot.orgs.get(id).is_some_and(|(key, _)| key.is_some()),
        OrgMove::Folder { id } => snapshot.folders.contains(id),
    });
    moved
        .collections
        .retain(|_, target| snapshot.collections.contains_key(target));
    moved
        .items
        .retain(|_, target| snapshot.ciphers.contains_key(target));
    let items = moved.items.clone();
    moved.attachments.retain(|key, target| {
        key.split_once('/')
            .and_then(|(item, _)| items.get(item))
            .and_then(|item| snapshot.ciphers.get(item))
            .is_some_and(|cipher| cipher.attachments.contains(target))
    });
    moved
        .sends
        .retain(|_, target| snapshot.sends.contains(target));
}

/// Errors that aren't about one object: the session ran out, the server is
/// out of reach or wants a break. The move stops; the next run continues.
fn ends_the_run(error: &Error) -> bool {
    matches!(
        error,
        Error::SessionExpired | Error::Network(_) | Error::Server { status: 429, .. }
    )
}

fn attachment_key(item: &str, attachment: &str) -> String {
    format!("{item}/{attachment}")
}

fn count(count: &mut Count, done: bool) {
    count.total += 1;
    if done {
        count.moved += 1;
    } else {
        count.todo += 1;
    }
}

/// `edit` and `viewPassword` of every organisation item, from the raw sync
/// (lowered keys). An item without them may do everything.
fn rights_of(raw: &Value) -> HashMap<String, Rights> {
    raw["ciphers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|cipher| {
            let id = cipher["id"].as_str()?.to_string();
            Some((
                id,
                Rights {
                    edit: cipher["edit"].as_bool().unwrap_or(true),
                    view_password: cipher["viewpassword"].as_bool().unwrap_or(true),
                },
            ))
        })
        .collect()
}

fn id_of(answer: &Value) -> Result<String, Error> {
    text_of(answer, "id").ok_or_else(|| Error::Server {
        status: 200,
        message: "the server didn't say which id it gave".into(),
    })
}

fn base64_std(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// An encrypted value, opened with `from` and sealed again with `to`.
fn reseal_text(text: &str, from: &SymmetricKey, to: &SymmetricKey) -> Result<String, Error> {
    let plain = text.parse::<EncString>()?.decrypt(from)?;
    Ok(EncString::encrypt(&plain, to).to_string())
}

/// A passkey as it is kept with a login, every encrypted value sealed again
/// for the new item key. Whatever isn't encrypted (the creation date, keys a
/// newer client added) stays as it is.
fn reseal_passkey(passkey: &Value, from: &SymmetricKey, to: &SymmetricKey) -> Result<Value, Error> {
    let Value::Object(map) = passkey else {
        return Err(Error::Crypto("a passkey isn't an object".into()));
    };
    let mut out = serde_json::Map::new();
    for (name, value) in map {
        let value = match value {
            Value::String(text) if text.parse::<EncString>().is_ok() => {
                Value::String(reseal_text(text, from, to)?)
            }
            other => other.clone(),
        };
        out.insert(name.clone(), value);
    }
    Ok(Value::Object(out))
}

/// The item under a new item key, ready for [`Item::seal`] with `outer`
/// (the target's user or organisation key). The values are plain in `item`
/// already; what travels encrypted (passkeys, address checksums) is sealed
/// again from `old_key` to `new_key`.
fn reseal_item(
    item: &Item,
    old_key: &SymmetricKey,
    new_key: &SymmetricKey,
    outer: &SymmetricKey,
) -> Result<Item, Error> {
    let mut fresh = item.clone();
    fresh.id = String::new();
    fresh.revision_date = None;
    fresh.key = Some(new_key.clone());
    fresh.wrapped_key = Some(EncString::encrypt(&new_key.to_bytes(), outer).to_string());
    if let Some(login) = fresh.login.as_mut() {
        if let Some(passkeys) = login.passkeys.as_mut() {
            for passkey in passkeys.iter_mut() {
                *passkey = reseal_passkey(passkey, old_key, new_key)?;
            }
        }
        for uri in &mut login.uris {
            // A checksum that doesn't open is only a check: it's left out.
            uri.checksum = uri
                .checksum
                .as_deref()
                .and_then(|c| reseal_text(c, old_key, new_key).ok());
        }
    }
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passkey_keeps_what_isnt_encrypted() {
        let old = SymmetricKey::generate();
        let new = SymmetricKey::generate();
        let enc = |t: &str| Value::String(EncString::encrypt(t.as_bytes(), &old).to_string());
        let passkey = json!({ "credentialId": enc("id-1"), "rpId": enc("example.com"),
            "creationDate": "2026-09-20T10:15:00.000Z", "futureField": { "Nested": [1] } });
        let moved = reseal_passkey(&passkey, &old, &new).unwrap();
        let open = |v: &Value| {
            v.as_str()
                .unwrap()
                .parse::<EncString>()
                .unwrap()
                .decrypt_string(&new)
                .unwrap()
                .to_string()
        };
        assert_eq!(open(&moved["rpId"]), "example.com");
        assert_eq!(open(&moved["credentialId"]), "id-1");
        assert_eq!(moved["creationDate"], passkey["creationDate"]);
        assert_eq!(moved["futureField"], passkey["futureField"]);
    }

    #[test]
    fn the_journal_forgets_what_the_target_lost() {
        let mut moved = Moved::default();
        moved.folders.insert("f1".into(), "t-f1".into());
        moved.folders.insert("f2".into(), "t-gone".into());
        moved.items.insert("i1".into(), "t-i1".into());
        moved.items.insert("i2".into(), "t-i2-gone".into());
        moved.attachments.insert("i1/a1".into(), "t-a1".into());
        moved.attachments.insert("i1/a2".into(), "t-a2-gone".into());
        moved.attachments.insert("i2/a3".into(), "t-a3".into());
        moved
            .organizations
            .insert("o1".into(), OrgMove::Folder { id: "t-f1".into() });
        moved
            .organizations
            .insert("o2".into(), OrgMove::Family { id: "t-o2".into() });
        let mut snapshot = Snapshot::default();
        snapshot.folders.insert("t-f1".into());
        snapshot.ciphers.insert(
            "t-i1".into(),
            TargetCipher {
                attachments: HashSet::from(["t-a1".to_string()]),
                ..TargetCipher::default()
            },
        );
        forget_what_is_gone(&mut moved, &snapshot);
        assert_eq!(moved.folders.len(), 1);
        assert_eq!(moved.items.keys().collect::<Vec<_>>(), ["i1"]);
        assert_eq!(moved.attachments.keys().collect::<Vec<_>>(), ["i1/a1"]);
        assert_eq!(moved.organizations.keys().collect::<Vec<_>>(), ["o1"]);
    }

    #[test]
    fn the_journal_round_trips() {
        let mut journal = Journal::default();
        let moved = journal
            .sources
            .entry(source_key(&Server::BitwardenEu, " Nyu@Example.com "))
            .or_default();
        moved
            .organizations
            .insert("o".into(), OrgMove::Family { id: "t".into() });
        let text = serde_json::to_string(&journal).unwrap();
        assert!(text.contains("bitwarden.eu|nyu@example.com"));
        assert!(text.contains(r#""kind":"family""#));
        assert_eq!(serde_json::from_str::<Journal>(&text).unwrap(), journal);
    }
}
