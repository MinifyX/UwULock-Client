//! The password check (UwULock-Server's `docs/uwu-api.md` §15): which logins
//! have a weak password, one used more than once, one seen in a breach, an
//! address without https, a site that had a breach after the password was
//! last changed, or a site that offers two-step login when none is stored.
//!
//! Everything is worked out on the device. For breached passwords only the
//! first five hex digits of each password's SHA-1 (Have I Been Pwned) and the
//! first ten of its Keccak-512 (XposedOrNot) go to the account's server; the
//! lists of breached sites and of sites with two-step login come from the
//! server as a whole and are matched here, so the server never learns which
//! sites are in a vault.
//!
//! The formats are the web vault's: the report the clients keep encrypted on
//! the server, the ignore list (§15.6) and the problem ids, so a report or an
//! ignored problem from one UwULock app is the same in every other.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use sha3::Keccak512;

use crate::vault::{Item, ItemKind};

/// Below this many bits a password counts as weak: a random one of ten
/// letters and digits has about 60.
pub const WEAK_BITS: u32 = 50;

// ── Problems ───────────────────────────────────────────────

/// The kinds of problem the check knows: stable ids, the same in every
/// UwULock app (the ignore list keeps them). In order of weight, the
/// heaviest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemKind {
    /// Have I Been Pwned or XposedOrNot saw the password.
    Breached,
    /// The site had a breach after the password was last changed.
    SiteBreach,
    Reused,
    /// Fewer than [`WEAK_BITS`].
    Weak,
    /// An `http://` address.
    Unsecured,
    /// The site offers codes from an authenticator app; none is stored.
    Twofa,
}

impl ProblemKind {
    pub const ALL: [ProblemKind; 6] = [
        ProblemKind::Breached,
        ProblemKind::SiteBreach,
        ProblemKind::Reused,
        ProblemKind::Weak,
        ProblemKind::Unsecured,
        ProblemKind::Twofa,
    ];

    /// Each kind outweighs all lighter ones together.
    pub fn weight(self) -> u32 {
        let index = Self::ALL.iter().position(|k| *k == self).unwrap_or(0) as u32;
        1 << (Self::ALL.len() as u32 - index)
    }

    /// Whether a new password solves it.
    pub fn about_the_password(self) -> bool {
        matches!(
            self,
            ProblemKind::Breached
                | ProblemKind::SiteBreach
                | ProblemKind::Reused
                | ProblemKind::Weak
        )
    }
}

// ── Hashes for the breach sources ──────────────────────────

/// Have I Been Pwned's k-anonymity split of the password's SHA-1: the first
/// five hex digits, which are sent, and the rest, which stays here. Upper
/// case, as HIBP answers.
pub fn hibp_prefix(password: &str) -> (String, String) {
    let hash: String = Sha1::digest(password.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    let (prefix, suffix) = hash.split_at(5);
    (prefix.to_string(), suffix.to_string())
}

/// XposedOrNot's prefix: the first ten hex digits of the password's original
/// Keccak-512 (padding 0x01, not NIST's SHA3-512), lower case.
pub fn xon_prefix(password: &str) -> String {
    Keccak512::digest(password.as_bytes())
        .iter()
        .take(5)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// How often HIBP's range answer (`SUFFIX:COUNT` per line) counts `suffix`.
pub fn hibp_count(range: &str, suffix: &str) -> u64 {
    range
        .lines()
        .filter_map(|line| {
            let (found, count) = line.trim().split_once(':')?;
            found
                .eq_ignore_ascii_case(suffix)
                .then(|| count.trim().parse().ok())?
        })
        .next()
        .unwrap_or(0)
}

// ── The report ─────────────────────────────────────────────

/// One checked login. The same JSON as the web vault's `Finding`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Finding {
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub bits: u32,
    pub weak: bool,
    /// How many other logins have the same password.
    pub reused: u32,
    pub unsecured: bool,
    /// How often it was in a breach (the most any source counted); `None`
    /// when that wasn't checked.
    pub breached: Option<u64>,
    /// Which sources saw it: `hibp`, `xon`.
    pub breach_sources: Vec<String>,
    /// The login's website without `www.`.
    pub host: Option<String>,
    /// The login's first `http(s)` address.
    pub uri: Option<String>,
    /// When the password was last changed, else when the item was made.
    pub password_changed: Option<String>,
}

/// The report as the clients keep it on the server, encrypted under the
/// extras key (`/uwu/v1/reports/health`). The web vault's `Report`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub checked: u32,
    pub breaches_checked: bool,
    /// A source didn't answer for some passwords.
    pub breaches_incomplete: bool,
}

/// What a check keeps per login between asking for the prefixes and hearing
/// the answers. Never leaves the device; the suffix stays here.
#[derive(Debug, Clone)]
pub struct Checked {
    pub id: String,
    pub hibp_prefix: String,
    pub hibp_suffix: String,
    pub xon_prefix: String,
}

/// A report without breaches yet, and what asking for them needs.
#[derive(Debug, Clone, Default)]
pub struct Prepared {
    pub report: Report,
    pub checks: Vec<Checked>,
}

impl Prepared {
    /// Every HIBP prefix once, sorted.
    pub fn hibp_prefixes(&self) -> Vec<String> {
        let set: BTreeSet<_> = self.checks.iter().map(|c| c.hibp_prefix.clone()).collect();
        set.into_iter().collect()
    }

    /// Every XposedOrNot prefix once, sorted.
    pub fn xon_prefixes(&self) -> Vec<String> {
        let set: BTreeSet<_> = self.checks.iter().map(|c| c.xon_prefix.clone()).collect();
        set.into_iter().collect()
    }

    /// The logins HIBP's answer for `prefix` counts, with their counts.
    pub fn hibp_hits(&self, prefix: &str, range: &str) -> Vec<(String, u64)> {
        self.checks
            .iter()
            .filter(|c| c.hibp_prefix.eq_ignore_ascii_case(prefix))
            .filter_map(|c| {
                let count = hibp_count(range, &c.hibp_suffix);
                (count > 0).then(|| (c.id.clone(), count))
            })
            .collect()
    }

    /// The logins whose password XposedOrNot saw `count` times, by `prefix`.
    pub fn xon_hits(&self, prefix: &str, count: u64) -> Vec<(String, u64)> {
        if count == 0 {
            return Vec::new();
        }
        self.checks
            .iter()
            .filter(|c| c.xon_prefix.eq_ignore_ascii_case(prefix))
            .map(|c| (c.id.clone(), count))
            .collect()
    }
}

/// The host of an address without `www.`: `shop.example.com` from
/// `https://www.shop.example.com/login`. Addresses without a scheme count as
/// `https://`.
pub fn host_of(uri: &str) -> Option<String> {
    let uri = uri.trim();
    let with_scheme = if uri.contains("://") {
        uri.to_string()
    } else {
        format!("https://{uri}")
    };
    let host = url::Url::parse(&with_scheme).ok()?.host_str()?.to_string();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (!host.is_empty()).then_some(host)
}

fn text(value: Option<&crate::vault::Secret>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Whether the check looks at `item`: a login with a password, not in the
/// trash, not archived.
fn checked_login(item: &Item) -> Option<&str> {
    if item.kind != ItemKind::Login || item.deleted || item.archived_date.is_some() {
        return None;
    }
    let password = item.login.as_ref()?.password.as_ref()?;
    (!password.is_empty()).then_some(password.as_str())
}

/// Checks every login's password: weak, reused, sent without https; with
/// what asking for breaches needs. Breaches aren't checked yet.
pub fn prepare<'a>(items: impl IntoIterator<Item = &'a Item>) -> Prepared {
    let logins: Vec<(&Item, &str)> = items
        .into_iter()
        .filter_map(|item| Some((item, checked_login(item)?)))
        .collect();
    let mut by_password: HashMap<&str, u32> = HashMap::new();
    for (_, password) in &logins {
        *by_password.entry(password).or_default() += 1;
    }
    let mut prepared = Prepared::default();
    for (item, password) in logins {
        let login = item.login.as_ref();
        let uris = login.map(|l| l.uris.as_slice()).unwrap_or_default();
        let bits = crate::generator::entropy_bits(password);
        let (hibp_prefix, hibp_suffix) = hibp_prefix(password);
        prepared.checks.push(Checked {
            id: item.id.clone(),
            hibp_prefix,
            hibp_suffix,
            xon_prefix: xon_prefix(password),
        });
        prepared.report.findings.push(Finding {
            id: item.id.clone(),
            name: item.name.to_string(),
            subtitle: text(login.and_then(|l| l.username.as_ref())),
            bits,
            weak: bits < WEAK_BITS,
            reused: by_password.get(password).copied().unwrap_or(1) - 1,
            unsecured: uris
                .iter()
                .any(|u| u.uri.trim().to_ascii_lowercase().starts_with("http://")),
            breached: None,
            breach_sources: Vec::new(),
            host: uris.iter().find_map(|u| host_of(&u.uri)),
            uri: uris
                .iter()
                .map(|u| u.uri.trim())
                .find(|u| {
                    let lower = u.to_ascii_lowercase();
                    lower.starts_with("https://") || lower.starts_with("http://")
                })
                .map(str::to_string),
            password_changed: login
                .and_then(|l| l.password_revision_date.clone())
                .or_else(|| item.creation_date.clone()),
        });
    }
    prepared.report.checked = prepared.report.findings.len() as u32;
    prepared
}

/// What the breach sources counted, by login: the most any source counted,
/// and which sources saw it.
#[derive(Debug, Clone, Default)]
pub struct BreachCounts(HashMap<String, (u64, BTreeSet<String>)>);

impl BreachCounts {
    pub fn add(&mut self, id: &str, count: u64, source: &str) {
        let entry = self.0.entry(id.to_string()).or_default();
        entry.0 = entry.0.max(count);
        entry.1.insert(source.to_string());
    }

    /// Fills the report's breaches. `checked`: some source was asked.
    pub fn apply(&self, report: &mut Report, checked: bool, incomplete: bool) {
        report.breaches_checked = checked;
        report.breaches_incomplete = incomplete;
        for finding in &mut report.findings {
            let found = self.0.get(&finding.id);
            finding.breached = checked.then(|| found.map_or(0, |f| f.0));
            finding.breach_sources = found
                .map(|f| f.1.iter().cloned().collect())
                .unwrap_or_default();
        }
    }
}

/// Takes the breaches of an earlier report for the logins whose password
/// hasn't changed since (same id and same change date), so a report shows
/// them without asking again. The rest stays unchecked.
pub fn carry_breaches(report: &mut Report, earlier: &Report) {
    if !earlier.breaches_checked {
        return;
    }
    let known: HashMap<&str, &Finding> = earlier
        .findings
        .iter()
        .map(|f| (f.id.as_str(), f))
        .collect();
    report.breaches_checked = true;
    report.breaches_incomplete = earlier.breaches_incomplete;
    for finding in &mut report.findings {
        match known.get(finding.id.as_str()) {
            Some(old)
                if old.password_changed.is_some()
                    && old.password_changed == finding.password_changed =>
            {
                finding.breached = old.breached;
                finding.breach_sources = old.breach_sources.clone();
            }
            _ => finding.breached = None,
        }
    }
}

/// After a new password was saved here: the password problems of `id` are
/// solved until the next check.
pub fn renewed(report: &mut Report, id: &str, now: &str) {
    let checked = report.breaches_checked;
    if let Some(finding) = report.findings.iter_mut().find(|f| f.id == id) {
        finding.breached = checked.then_some(0);
        finding.breach_sources.clear();
        finding.weak = false;
        finding.reused = 0;
        finding.password_changed = Some(now.to_string());
    }
}

// ── Domains ────────────────────────────────────────────────

/// A host and every domain above it, nearest first (`login.example.com`,
/// `example.com`), never a bare top-level domain. `www.` is ignored; IP
/// addresses and names without a dot give nothing.
pub fn domains_up(host: &str) -> Vec<String> {
    let mut name = host.trim().to_ascii_lowercase();
    if name.ends_with('.') {
        name.pop();
    }
    if name.is_empty()
        || name.chars().all(|c| c.is_ascii_digit() || c == '.')
        || name.contains(':')
        || !name.contains('.')
    {
        return Vec::new();
    }
    if let Some(rest) = name.strip_prefix("www.") {
        name = rest.to_string();
    }
    let mut out = Vec::new();
    loop {
        out.push(name.clone());
        let Some(dot) = name.find('.') else {
            return out;
        };
        let parent = name[dot + 1..].to_string();
        if !parent.contains('.') {
            return out;
        }
        name = parent;
    }
}

// ── Breached sites (§15.3) ─────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteBreach {
    pub domain: String,
    pub title: String,
    /// When it happened, `YYYY-MM-DD`.
    pub date: Option<String>,
    /// When the source listed it.
    pub added: Option<String>,
    pub records: u64,
    /// Passwords or their hashes were taken.
    pub passwords: bool,
    pub data_classes: Vec<String>,
    /// Source id → the breach's name there (`hibp`, `xon`).
    pub sources: BTreeMap<String, String>,
}

/// A source of a list, for the attribution under it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Source {
    pub id: Option<String>,
    pub name: String,
    pub url: Option<String>,
    pub license: Option<String>,
    pub updated: Option<String>,
}

/// `GET /uwu/v1/breaches/sites`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteBreachList {
    pub updated: Option<String>,
    pub sources: Vec<Source>,
    pub breaches: Vec<SiteBreach>,
}

/// The breached sites by domain.
#[derive(Debug, Clone, Default)]
pub struct SiteIndex(HashMap<String, Vec<SiteBreach>>);

impl SiteIndex {
    pub fn new(breaches: &[SiteBreach]) -> Self {
        let mut index: HashMap<String, Vec<SiteBreach>> = HashMap::new();
        for breach in breaches {
            index
                .entry(breach.domain.to_ascii_lowercase())
                .or_default()
                .push(breach.clone());
        }
        SiteIndex(index)
    }

    /// The breaches of a host and of every domain above it.
    pub fn breaches_for(&self, host: &str) -> Vec<&SiteBreach> {
        domains_up(host)
            .iter()
            .flat_map(|domain| self.0.get(domain).into_iter().flatten())
            .collect()
    }

    /// "The site had a breach after your last password change": the latest
    /// breach of the login's site in which passwords were taken and which
    /// happened on or after the day the password was last changed.
    pub fn after_change(&self, finding: &Finding) -> Option<&SiteBreach> {
        let changed: String = finding
            .password_changed
            .as_ref()?
            .chars()
            .take(10)
            .collect();
        if changed.is_empty() {
            return None;
        }
        let mut after: Vec<&SiteBreach> = self
            .breaches_for(finding.host.as_deref()?)
            .into_iter()
            .filter(|b| {
                b.passwords
                    && b.date.as_deref().or(b.added.as_deref()).unwrap_or_default()
                        >= changed.as_str()
            })
            .collect();
        after.sort_by(|a, b| {
            b.date
                .as_deref()
                .unwrap_or_default()
                .cmp(a.date.as_deref().unwrap_or_default())
        });
        after.into_iter().next()
    }

    /// The breach XposedOrNot calls `name` (for an address's breaches).
    pub fn by_xon_name(&self, name: &str) -> Option<&SiteBreach> {
        self.0
            .values()
            .flatten()
            .find(|b| b.sources.get("xon").is_some_and(|n| n == name))
    }
}

// ── 2FA Directory (§15) ────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TwofaEntry {
    pub domain: String,
    pub additional_domains: Vec<String>,
    pub name: String,
    /// `totp`, `u2f`, `sms`, `email`, …
    pub methods: Vec<String>,
    pub documentation: Option<String>,
}

/// `GET /uwu/v1/twofa-directory`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TwofaDirectory {
    pub updated: Option<String>,
    pub source: Source,
    pub entries: Vec<TwofaEntry>,
}

/// The directory by domain; the first entry for a domain wins.
#[derive(Debug, Clone, Default)]
pub struct TwofaIndex(HashMap<String, TwofaEntry>);

impl TwofaIndex {
    pub fn new(entries: &[TwofaEntry]) -> Self {
        let mut index = HashMap::new();
        for entry in entries {
            for domain in std::iter::once(&entry.domain).chain(&entry.additional_domains) {
                index
                    .entry(domain.to_ascii_lowercase())
                    .or_insert_with(|| entry.clone());
            }
        }
        TwofaIndex(index)
    }

    /// The entry of a host or of the nearest domain above it.
    pub fn entry_for(&self, host: &str) -> Option<&TwofaEntry> {
        domains_up(host).iter().find_map(|d| self.0.get(d))
    }
}

/// A login for a site with codes from an authenticator app, without one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingTwofa {
    pub item_id: String,
    pub name: String,
    pub host: String,
    pub entry: TwofaEntry,
}

/// Every login (in the vault, readable) for a site that offers `totp`
/// without one stored, by name.
pub fn missing_twofa<'a>(
    items: impl IntoIterator<Item = &'a Item>,
    index: &TwofaIndex,
) -> Vec<MissingTwofa> {
    let mut out: Vec<MissingTwofa> = items
        .into_iter()
        .filter(|item| item.kind == ItemKind::Login && !item.deleted && !item.broken)
        .filter_map(|item| {
            let login = item.login.as_ref()?;
            if login.totp.as_ref().is_some_and(|t| !t.is_empty()) {
                return None;
            }
            let host = login.uris.iter().find_map(|u| host_of(&u.uri))?;
            let entry = index.entry_for(&host)?;
            entry
                .methods
                .iter()
                .any(|m| m == "totp")
                .then(|| MissingTwofa {
                    item_id: item.id.clone(),
                    name: item.name.to_string(),
                    host,
                    entry: entry.clone(),
                })
        })
        .collect();
    out.sort_by_key(|m| m.name.to_lowercase());
    out
}

// ── What the check doesn't show again (§15.6) ──────────────

/// One problem of one login not to show again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ignored {
    pub item_id: String,
    pub kind: ProblemKind,
    pub since: String,
    /// Keys a newer app added, kept as they are.
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

/// The ignore list, as it is encrypted under the extras key: `version` 1.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IgnoreList {
    pub version: u32,
    pub ignored: Vec<Ignored>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

impl Default for IgnoreList {
    fn default() -> Self {
        IgnoreList {
            version: 1,
            ignored: Vec::new(),
            other: BTreeMap::new(),
        }
    }
}

impl IgnoreList {
    /// The list from its JSON. Entries of a kind this app doesn't know, or
    /// that aren't entries, are dropped; unknown keys are kept. Anything that
    /// isn't such an object is an empty list.
    pub fn parse(json: &str) -> IgnoreList {
        let Ok(Value::Object(mut object)) = serde_json::from_str::<Value>(json) else {
            return IgnoreList::default();
        };
        let ignored = match object.remove("ignored") {
            Some(Value::Array(entries)) => entries
                .into_iter()
                .filter_map(|entry| serde_json::from_value::<Ignored>(entry).ok())
                .collect(),
            _ => Vec::new(),
        };
        object.remove("version");
        IgnoreList {
            version: 1,
            ignored,
            other: object.into_iter().collect(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn is_ignored(&self, item_id: &str, kind: ProblemKind) -> bool {
        self.ignored
            .iter()
            .any(|e| e.item_id == item_id && e.kind == kind)
    }

    /// Hides `kind` of `item_id`; once per login and kind.
    pub fn ignore(&mut self, item_id: &str, kind: ProblemKind, now: &str) {
        if !self.is_ignored(item_id, kind) {
            self.ignored.push(Ignored {
                item_id: item_id.to_string(),
                kind,
                since: now.to_string(),
                other: BTreeMap::new(),
            });
        }
    }

    pub fn unignore(&mut self, item_id: &str, kind: ProblemKind) {
        self.ignored
            .retain(|e| !(e.item_id == item_id && e.kind == kind));
    }

    /// Drops the entries of logins that are gone.
    pub fn tidy(&mut self, item_ids: &HashSet<&str>) {
        self.ignored
            .retain(|e| item_ids.contains(e.item_id.as_str()));
    }
}

// ── One card per login (§15.7) ─────────────────────────────

/// A problem of a login, with what the card says about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Problem {
    Breached { count: u64, sources: Vec<String> },
    SiteBreach { breach: SiteBreach },
    Reused { others: u32 },
    Weak { bits: u32 },
    Unsecured,
    Twofa { documentation: Option<String> },
}

impl Problem {
    pub fn kind(&self) -> ProblemKind {
        match self {
            Problem::Breached { .. } => ProblemKind::Breached,
            Problem::SiteBreach { .. } => ProblemKind::SiteBreach,
            Problem::Reused { .. } => ProblemKind::Reused,
            Problem::Weak { .. } => ProblemKind::Weak,
            Problem::Unsecured => ProblemKind::Unsecured,
            Problem::Twofa { .. } => ProblemKind::Twofa,
        }
    }
}

/// A login with its problems.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub finding: Finding,
    pub problems: Vec<Problem>,
}

/// Every problem of `finding`, ignored ones too, heaviest first.
pub fn problems_of(
    finding: &Finding,
    sites: Option<&SiteIndex>,
    twofa: &HashMap<String, Option<String>>,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    if let Some(count) = finding.breached.filter(|count| *count > 0) {
        problems.push(Problem::Breached {
            count,
            sources: finding.breach_sources.clone(),
        });
    }
    if let Some(breach) = sites.and_then(|index| index.after_change(finding)) {
        problems.push(Problem::SiteBreach {
            breach: breach.clone(),
        });
    }
    if finding.reused > 0 {
        problems.push(Problem::Reused {
            others: finding.reused,
        });
    }
    if finding.weak {
        problems.push(Problem::Weak { bits: finding.bits });
    }
    if finding.unsecured {
        problems.push(Problem::Unsecured);
    }
    if let Some(documentation) = twofa.get(&finding.id) {
        problems.push(Problem::Twofa {
            documentation: documentation.clone(),
        });
    }
    problems
}

/// The cards of the review: every login with at least one problem that isn't
/// ignored, the worst first (each kind outweighs all lighter ones together),
/// then by name. `twofa` maps logins without a stored code to the guide of
/// their site, if it has one.
pub fn cards(
    report: &Report,
    sites: Option<&SiteIndex>,
    twofa: &HashMap<String, Option<String>>,
    ignored: &IgnoreList,
) -> Vec<Card> {
    let mut cards: Vec<Card> = report
        .findings
        .iter()
        .filter_map(|finding| {
            let problems: Vec<Problem> = problems_of(finding, sites, twofa)
                .into_iter()
                .filter(|p| !ignored.is_ignored(&finding.id, p.kind()))
                .collect();
            (!problems.is_empty()).then(|| Card {
                finding: finding.clone(),
                problems,
            })
        })
        .collect();
    let weight = |card: &Card| -> u32 { card.problems.iter().map(|p| p.kind().weight()).sum() };
    cards.sort_by(|a, b| {
        weight(b).cmp(&weight(a)).then_with(|| {
            a.finding
                .name
                .to_lowercase()
                .cmp(&b.finding.name.to_lowercase())
        })
    });
    cards
}

// ── Addresses (§15.4) ──────────────────────────────────────

/// Whether `text` looks like an address worth asking XposedOrNot about:
/// a plain `local@domain.tld`, the web vault's rule.
pub fn is_address(text: &str) -> bool {
    let text = text.trim();
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    let local_ok = !local.is_empty()
        && !local
            .chars()
            .any(|c| c.is_whitespace() || "@/?#%\\\"<>".contains(c));
    let labels: Vec<&str> = domain.split('.').collect();
    let domain_ok = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    local_ok && domain_ok
}
