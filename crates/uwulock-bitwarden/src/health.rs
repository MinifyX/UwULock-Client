//! The password check's calls to UwULock Server (its `docs/uwu-api.md` §15):
//! breached passwords through the server (Have I Been Pwned, XposedOrNot),
//! the lists of breached sites and of sites with two-step login, the check of
//! addresses (with the account's consent), change-password pages, the report
//! and the ignore list kept encrypted under the extras key.
//!
//! The server talks to the sources; the app only talks to its server. What
//! the lists are matched against stays on the device (`uwulock_core::health`).

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::Client;
use crate::uwu::{uwu_path, Info, UwuError, UwuResult};
use crate::Error;
use uwulock_core::health::{BreachCounts, Prepared, SiteBreachList, TwofaDirectory};

/// Which breach sources the server has switched on (§15.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BreachSwitches {
    pub hibp: bool,
    pub xon_passwords: bool,
    pub site_breaches: bool,
    pub email_check: bool,
    pub change_password: bool,
}

impl BreachSwitches {
    /// The switches of a server; one before 0.7 has no `breaches` and knows
    /// only Have I Been Pwned (when it lists `hibp`).
    pub fn of(info: &Info) -> BreachSwitches {
        info.breaches.unwrap_or(BreachSwitches {
            hibp: info.has("hibp"),
            ..BreachSwitches::default()
        })
    }

    /// Whether the server keeps the ignore list (0.7 and later).
    pub fn ignore_list(info: &Info) -> bool {
        info.breaches.is_some()
    }
}

/// The account's consent to the check of its addresses (§15.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmailOptIn {
    pub opted_in: bool,
    pub since: Option<String>,
}

/// One address's answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmailResult {
    pub email: String,
    /// `found`, `clean`, `later` (the server's budget is used up for now) or
    /// `failed`.
    pub status: String,
    /// XposedOrNot's names of the breaches (`sources.xon` of the site list).
    pub breaches: Vec<String>,
}

/// `POST /uwu/v1/breaches/emails`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmailAnswer {
    pub results: Vec<EmailResult>,
    /// Seconds until asking again is worth it; `None` when nothing is left.
    pub retry_after: Option<u64>,
}

/// The most addresses one request may carry.
pub const EMAILS_PER_CALL: usize = 50;

/// Something kept encrypted on the server: the report or the ignore list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stored {
    pub data: Option<String>,
    pub revision_date: Option<String>,
}

/// How many prefixes are asked for at the same time.
const AT_ONCE: usize = 4;

/// The most a range answer of HIBP may weigh (they are about 30 KiB padded).
const MAX_RANGE: u64 = 1024 * 1024;

/// What the breach sources said about a prepared report.
#[derive(Debug, Clone, Default)]
pub struct BreachAnswers {
    pub counts: BreachCounts,
    /// A question went unanswered; the rest still counts.
    pub incomplete: bool,
}

impl Client {
    /// HIBP's range for a five-digit SHA-1 prefix, through the server.
    pub async fn hibp_range(&self, access_token: &str, prefix: &str) -> UwuResult<String> {
        let bytes = self
            .uwu_download(
                access_token,
                &format!("/hibp/{}", uwu_path(prefix)),
                MAX_RANGE,
            )
            .await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// How often XposedOrNot saw a password with this ten-digit Keccak-512
    /// prefix, through the server; 0 when it doesn't know it.
    pub async fn xon_count(&self, access_token: &str, prefix: &str) -> UwuResult<u64> {
        let answer = self
            .uwu_get(access_token, &format!("/xon/{}", uwu_path(prefix)))
            .await?;
        Ok(answer.get("count").and_then(Value::as_u64).unwrap_or(0))
    }

    /// Asks the sources that are on for every prefix of `prepared`, a few at a
    /// time; `progress(done, total)` after each answer.
    pub async fn breach_counts(
        &self,
        access_token: &str,
        prepared: &Prepared,
        hibp: bool,
        xon: bool,
        progress: impl Fn(usize, usize),
    ) -> BreachAnswers {
        enum Ask {
            Hibp(String),
            Xon(String),
        }
        let mut asks = Vec::new();
        if hibp {
            asks.extend(prepared.hibp_prefixes().into_iter().map(Ask::Hibp));
        }
        if xon {
            asks.extend(prepared.xon_prefixes().into_iter().map(Ask::Xon));
        }
        let total = asks.len();
        let mut answers = BreachAnswers::default();
        if total == 0 {
            return answers;
        }
        progress(0, total);
        let mut done = 0;
        let mut results = stream::iter(asks)
            .map(|ask| async move {
                match ask {
                    Ask::Hibp(prefix) => self
                        .hibp_range(access_token, &prefix)
                        .await
                        .map(|range| ("hibp", prepared.hibp_hits(&prefix, &range))),
                    Ask::Xon(prefix) => self
                        .xon_count(access_token, &prefix)
                        .await
                        .map(|count| ("xon", prepared.xon_hits(&prefix, count))),
                }
            })
            .buffer_unordered(AT_ONCE);
        while let Some(result) = results.next().await {
            match result {
                Ok((source, hits)) => {
                    for (id, count) in hits {
                        answers.counts.add(&id, count, source);
                    }
                }
                Err(error) => {
                    // Not the prefix: it is part of a password's hash.
                    tracing::warn!(status = ?error.status(), "a breach source didn't answer");
                    answers.incomplete = true;
                }
            }
            done += 1;
            progress(done, total);
        }
        answers
    }

    /// The merged list of breached sites (§15.3).
    pub async fn site_breaches(&self, access_token: &str) -> UwuResult<SiteBreachList> {
        read(
            self.uwu_get(access_token, "/breaches/sites").await?,
            "list of breached sites",
        )
    }

    /// The server's copy of 2FA Directory (§15).
    pub async fn twofa_directory(&self, access_token: &str) -> UwuResult<TwofaDirectory> {
        read(
            self.uwu_get(access_token, "/twofa-directory").await?,
            "list of sites with two-step login",
        )
    }

    /// The site's `/.well-known/change-password`, if the server found one.
    pub async fn change_password_page(
        &self,
        access_token: &str,
        host: &str,
    ) -> UwuResult<Option<String>> {
        let answer = self
            .uwu_get(
                access_token,
                &format!("/change-password/{}", uwu_path(&host.to_ascii_lowercase())),
            )
            .await?;
        Ok(answer
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| url.starts_with("https://"))
            .map(str::to_string))
    }

    pub async fn email_opt_in(&self, access_token: &str) -> UwuResult<EmailOptIn> {
        read(
            self.uwu_get(access_token, "/breaches/emails/opt-in")
                .await?,
            "consent",
        )
    }

    pub async fn set_email_opt_in(
        &self,
        access_token: &str,
        opted_in: bool,
    ) -> UwuResult<EmailOptIn> {
        read(
            self.uwu_put(
                access_token,
                "/breaches/emails/opt-in",
                &json!({ "optedIn": opted_in }),
            )
            .await?,
            "consent",
        )
    }

    /// One request about at most [`EMAILS_PER_CALL`] addresses.
    pub async fn check_emails(
        &self,
        access_token: &str,
        emails: &[String],
    ) -> UwuResult<EmailAnswer> {
        let emails = &emails[..emails.len().min(EMAILS_PER_CALL)];
        read(
            self.uwu_post(
                access_token,
                "/breaches/emails",
                &json!({ "emails": emails }),
            )
            .await?,
            "answer about addresses",
        )
    }

    /// The last report a client saved, still encrypted.
    pub async fn health_report(&self, access_token: &str) -> UwuResult<Stored> {
        read(
            self.uwu_get(access_token, "/reports/health").await?,
            "report",
        )
    }

    pub async fn put_health_report(&self, access_token: &str, data: &str) -> UwuResult<Stored> {
        read(
            self.uwu_put(access_token, "/reports/health", &json!({ "data": data }))
                .await?,
            "report",
        )
    }

    /// The ignore list, still encrypted (§15.6).
    pub async fn health_ignores(&self, access_token: &str) -> UwuResult<Stored> {
        read(
            self.uwu_get(access_token, "/reports/health/ignored")
                .await?,
            "ignore list",
        )
    }

    /// Replaces the ignore list if `read_revision` (`None`: there was none)
    /// is still what the server has; else a refusal with code `conflict`.
    pub async fn put_health_ignores(
        &self,
        access_token: &str,
        data: &str,
        read_revision: Option<&str>,
    ) -> UwuResult<Stored> {
        read(
            self.uwu_put(
                access_token,
                "/reports/health/ignored",
                &json!({ "data": data, "revisionDate": read_revision }),
            )
            .await?,
            "ignore list",
        )
    }
}

/// Whether the server refused a save because the list changed elsewhere.
pub fn is_conflict(error: &UwuError) -> bool {
    error.code() == Some("conflict") || error.status() == Some(409)
}

fn read<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> UwuResult<T> {
    serde_json::from_value(value).map_err(|e| {
        UwuError::Core(Error::Server {
            status: 200,
            message: format!("the server's {what} doesn't read: {e}"),
        })
    })
}
