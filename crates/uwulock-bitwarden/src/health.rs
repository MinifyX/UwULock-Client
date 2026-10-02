//! The password check's calls to UwULock Server (its `docs/uwu-api.md` §15):
//! breached passwords through the server (Have I Been Pwned, XposedOrNot),
//! the lists of breached sites and of sites with two-step login, the check of
//! addresses (with the account's consent), change-password pages, the report
//! and the ignore list kept encrypted under the extras key.
//!
//! The server talks to the sources; the app only talks to its server. What
//! the lists are matched against stays on the device (`uwulock_core::health`).

use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;

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

/// `https://{host}/.well-known/change-password` for a plain host name (as
/// `uwulock_core::health::host_of` gives it); `None` for anything else, so
/// nothing but a host ends up in front of the path.
pub fn change_password_url(host: &str) -> Option<String> {
    let label_ok = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    (host.len() <= 253 && host.split('.').all(label_ok))
        .then(|| format!("https://{host}/.well-known/change-password"))
}

/// How many prefixes are asked for at the same time.
const AT_ONCE: usize = 4;

/// The most a range answer of HIBP may weigh (they are about 30 KiB padded).
const MAX_RANGE: u64 = 1024 * 1024;

/// How often a question the server answers with 429 is asked.
const BUSY_TRIES: u32 = 8;

/// The longest wait between two tries, whatever `Retry-After` says.
const LONGEST_WAIT: u64 = 60;

/// How long to wait before try `attempt + 1`: as long as the server's
/// `Retry-After` says (at most a minute), else 5 s, 10 s, … up to 30 s.
fn busy_wait(retry_after: Option<u64>, attempt: u32) -> Duration {
    Duration::from_secs(match retry_after {
        Some(seconds) => seconds.min(LONGEST_WAIT),
        None => u64::from((5 * attempt).min(30)),
    })
}

/// `job`, asked again while the server answers 429 ("too many", or `busy`
/// for XposedOrNot's queue): waiting as [`busy_wait`] says, at most `tries`
/// times. Anything else fails at once. `wait` sleeps; the tests hand in one
/// that doesn't.
pub(crate) async fn retrying_busy<T, J, JF, W, WF>(
    mut job: J,
    mut wait: W,
    tries: u32,
) -> UwuResult<T>
where
    J: FnMut() -> JF,
    JF: Future<Output = UwuResult<T>>,
    W: FnMut(Duration) -> WF,
    WF: Future<Output = ()>,
{
    let mut attempt = 1;
    loop {
        match job().await {
            Err(error) if error.status() == Some(429) && attempt < tries => {
                wait(busy_wait(error.retry_after(), attempt)).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// How far one breach source is: questions answered of all, and how many
/// wait because the server is busy (XposedOrNot's queue).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProgress {
    pub done: usize,
    pub total: usize,
    pub waiting: usize,
}

/// How far the breach check is, per source that is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreachProgress {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hibp: Option<SourceProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xon: Option<SourceProgress>,
}

impl BreachProgress {
    /// Answered of all, both sources together.
    pub fn done(&self) -> usize {
        self.hibp.map_or(0, |s| s.done) + self.xon.map_or(0, |s| s.done)
    }

    pub fn total(&self) -> usize {
        self.hibp.map_or(0, |s| s.total) + self.xon.map_or(0, |s| s.total)
    }

    fn source(&mut self, source: &str) -> Option<&mut SourceProgress> {
        match source {
            "hibp" => self.hibp.as_mut(),
            _ => self.xon.as_mut(),
        }
    }
}

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
    /// prefix, through the server; 0 when it doesn't know it. The server
    /// queues these questions for the whole instance (about one a second) and
    /// answers 429 `busy` when the queue is long: that is asked again
    /// ([`retrying_busy`]), so a long queue never makes the check incomplete.
    pub async fn xon_count(&self, access_token: &str, prefix: &str) -> UwuResult<u64> {
        self.xon_count_waiting(access_token, prefix, |_| ()).await
    }

    /// [`Client::xon_count`], saying when it starts (`true`) and stops
    /// (`false`) waiting for a busy server.
    async fn xon_count_waiting(
        &self,
        access_token: &str,
        prefix: &str,
        waiting: impl Fn(bool),
    ) -> UwuResult<u64> {
        let path = format!("/xon/{}", uwu_path(prefix));
        let waiting = &waiting;
        let answer = retrying_busy(
            || self.uwu_get(access_token, &path),
            |pause| async move {
                waiting(true);
                tokio::time::sleep(pause).await;
                waiting(false);
            },
            BUSY_TRIES,
        )
        .await?;
        Ok(answer.get("count").and_then(Value::as_u64).unwrap_or(0))
    }

    /// Asks the sources that are on for every prefix of `prepared`, a few at a
    /// time; `progress` per source after each answer, and when a question
    /// starts or stops waiting for a busy server. Only a question that fails
    /// for good (not a busy server waited out) makes the answers incomplete.
    pub async fn breach_counts(
        &self,
        access_token: &str,
        prepared: &Prepared,
        hibp: bool,
        xon: bool,
        progress: impl Fn(&BreachProgress) + Sync,
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
        let count = |hibp_ask: bool| {
            asks.iter()
                .filter(|ask| matches!(ask, Ask::Hibp(_)) == hibp_ask)
                .count()
        };
        let state = Mutex::new(BreachProgress {
            hibp: hibp.then(|| SourceProgress {
                total: count(true),
                ..SourceProgress::default()
            }),
            xon: xon.then(|| SourceProgress {
                total: count(false),
                ..SourceProgress::default()
            }),
        });
        let mut answers = BreachAnswers::default();
        if asks.is_empty() {
            return answers;
        }
        // Changes the progress and says it; never held across an await.
        let tell = |change: &dyn Fn(&mut BreachProgress)| {
            let now = {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                change(&mut state);
                *state
            };
            progress(&now);
        };
        tell(&|_| ());
        let tell_ref = &tell;
        let mut results = stream::iter(asks)
            .map(|ask| async move {
                let (source, result) = match ask {
                    Ask::Hibp(prefix) => (
                        "hibp",
                        self.hibp_range(access_token, &prefix)
                            .await
                            .map(|range| prepared.hibp_hits(&prefix, &range)),
                    ),
                    Ask::Xon(prefix) => (
                        "xon",
                        self.xon_count_waiting(access_token, &prefix, |on| {
                            tell_ref(&|p| {
                                if let Some(xon) = p.xon.as_mut() {
                                    xon.waiting = if on {
                                        xon.waiting + 1
                                    } else {
                                        xon.waiting.saturating_sub(1)
                                    };
                                }
                            })
                        })
                        .await
                        .map(|count| prepared.xon_hits(&prefix, count)),
                    ),
                };
                (source, result)
            })
            .buffer_unordered(AT_ONCE);
        while let Some((source, result)) = results.next().await {
            match result {
                Ok(hits) => {
                    for (id, count) in hits {
                        answers.counts.add(&id, count, source);
                    }
                }
                Err(error) => {
                    // Not the prefix: it is part of a password's hash.
                    tracing::warn!(source, status = ?error.status(), "a breach source didn't answer");
                    answers.incomplete = true;
                }
            }
            tell(&|p| {
                if let Some(s) = p.source(source) {
                    s.done += 1;
                }
            });
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
    ///
    /// The server only says *whether* the page exists; the address is made
    /// here from the login's own host. Whatever `url` the server sends is
    /// never opened, so a hostile server can't send the person to a page of
    /// its choosing to "change" (type) their password.
    pub async fn change_password_page(
        &self,
        access_token: &str,
        host: &str,
    ) -> UwuResult<Option<String>> {
        let host = host.to_ascii_lowercase();
        let Some(page) = change_password_url(&host) else {
            return Ok(None);
        };
        let answer = self
            .uwu_get(
                access_token,
                &format!("/change-password/{}", uwu_path(&host)),
            )
            .await?;
        let exists = answer
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| !url.is_empty());
        Ok(exists.then_some(page))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn busy() -> UwuError {
        UwuError::Refused {
            status: 429,
            code: "busy".into(),
            message: "XposedOrNot is busy.".into(),
            retry_after: None,
        }
    }

    #[test]
    fn retry_after_is_waited_as_said_but_not_forever() {
        assert_eq!(busy_wait(Some(7), 1), Duration::from_secs(7));
        assert_eq!(busy_wait(Some(3600), 1), Duration::from_secs(LONGEST_WAIT));
        assert_eq!(busy_wait(None, 2), Duration::from_secs(10));
        assert_eq!(busy_wait(None, 7), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn busy_is_asked_again_with_growing_waits() {
        let calls = Cell::new(0);
        let waits = RefCell::new(Vec::new());
        let answer = retrying_busy(
            || {
                calls.set(calls.get() + 1);
                let n = calls.get();
                async move {
                    if n < 4 {
                        Err(busy())
                    } else {
                        Ok(7)
                    }
                }
            },
            |d| {
                waits.borrow_mut().push(d.as_secs());
                async {}
            },
            BUSY_TRIES,
        )
        .await
        .unwrap();
        assert_eq!(answer, 7);
        assert_eq!(calls.get(), 4);
        assert_eq!(*waits.borrow(), vec![5, 10, 15]);
    }

    #[tokio::test]
    async fn gives_up_after_the_last_try_and_on_other_errors() {
        let calls = Cell::new(0);
        let waits = RefCell::new(Vec::new());
        let failed = retrying_busy::<u64, _, _, _, _>(
            || {
                calls.set(calls.get() + 1);
                async { Err(busy()) }
            },
            |d| {
                waits.borrow_mut().push(d.as_secs());
                async {}
            },
            BUSY_TRIES,
        )
        .await;
        assert_eq!(failed.unwrap_err().status(), Some(429));
        assert_eq!(calls.get(), BUSY_TRIES);
        assert_eq!(waits.borrow().last(), Some(&30));

        calls.set(0);
        let refused = retrying_busy::<u64, _, _, _, _>(
            || {
                calls.set(calls.get() + 1);
                async {
                    Err(UwuError::Refused {
                        status: 403,
                        code: "feature_off".into(),
                        message: "off".into(),
                        retry_after: None,
                    })
                }
            },
            |_| async {},
            BUSY_TRIES,
        )
        .await;
        assert_eq!(refused.unwrap_err().status(), Some(403));
        assert_eq!(calls.get(), 1, "only 429 is asked again");
    }
}
