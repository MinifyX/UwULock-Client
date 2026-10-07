//! Which logins belong to a site or an app, for the system's AutoFill
//! (Android's Credential Manager and autofill service, the Apple extension).
//!
//! Bitwarden's match detection, as the browser extension reads it
//! (`apps/extension/src/shared/uri.ts`): per address, or the default
//! (domain) when an address doesn't say.
//!
//! - **Domain**: the registrable domain is the same — `login.example.co.uk`
//!   and `www.example.co.uk` (Public Suffix List, compiled in).
//! - **Host**: the same host name and port.
//! - **Starts with** / **Exact**: only where the full address is known (a
//!   browser that hands it over); never on a host name alone.
//! - **Regular expression**: never here. The extension runs them in the page;
//!   the system's AutoFill shows such a login in UwULock's own search instead.
//! - **Never**: never.
//!
//! Apps: `androidapp://<package>` matches that Android app.

use serde::{Deserialize, Serialize};

pub const DOMAIN: u32 = 0;
pub const HOST: u32 = 1;
pub const STARTS_WITH: u32 = 2;
pub const EXACT: u32 = 3;
pub const REGEX: u32 = 4;
pub const NEVER: u32 = 5;

/// What asks to be filled: a web page (its address, or only its host when the
/// system doesn't say more) or an Android app.
#[derive(Debug, Clone, Default)]
pub struct Target {
    /// The page's full address, when known.
    pub url: Option<String>,
    /// The page's host name, lower case.
    pub host: Option<String>,
    /// The port, when the page had one.
    pub port: Option<u16>,
    /// An Android app's package name.
    pub package: Option<String>,
}

impl Target {
    /// A page by its address (`https://login.example.com/x`) or bare host
    /// (`login.example.com`).
    pub fn web(address: &str) -> Target {
        let address = address.trim();
        let parsed = parse(address);
        Target {
            url: address
                .contains("://")
                .then(|| address.to_string())
                .filter(|_| parsed.is_some()),
            host: parsed.as_ref().and_then(|u| u.host_str()).map(clean_host),
            port: parsed.as_ref().and_then(url::Url::port),
            package: None,
        }
    }

    /// An Android app.
    pub fn app(package: &str) -> Target {
        Target {
            package: Some(package.trim().to_string()),
            ..Target::default()
        }
    }
}

fn clean_host(host: &str) -> String {
    host.trim_matches(|c| c == '[' || c == ']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// An address someone typed without a scheme still has a host: Bitwarden
/// reads it as http.
fn parse(uri: &str) -> Option<url::Url> {
    let uri = uri.trim();
    if uri.is_empty() {
        return None;
    }
    let has_scheme = uri.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    }) && uri.contains("://");
    if has_scheme {
        url::Url::parse(uri).ok()
    } else {
        url::Url::parse(&format!("http://{uri}")).ok()
    }
}

/// The registrable domain of a host: `example.co.uk` for `a.b.example.co.uk`.
/// An IP address, `localhost` or a name without a known public suffix is its
/// own domain.
pub fn base_domain(host: &str) -> String {
    let host = clean_host(host);
    if host.parse::<std::net::IpAddr>().is_ok() || !host.contains('.') {
        return host;
    }
    psl::domain_str(&host).map_or(host.clone(), str::to_string)
}

/// The registrable domain of an address, for `http`, `https` and `ftp` only.
pub fn domain_of(uri: &str) -> Option<String> {
    let url = parse(uri)?;
    if !matches!(url.scheme(), "http" | "https" | "ftp") {
        return None;
    }
    url.host_str().map(base_domain).filter(|d| !d.is_empty())
}

/// Host name and port, as `Host` compares them: `example.com:8443`.
fn host_port(uri: &str) -> Option<String> {
    let url = parse(uri)?;
    let host = clean_host(url.host_str()?);
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

fn android_package(uri: &str) -> Option<&str> {
    let rest = uri.trim().strip_prefix("androidapp://")?;
    let package = rest.trim_end_matches('/');
    (!package.is_empty()).then_some(package)
}

/// Whether one saved address matches `target`.
pub fn uri_matches(uri: &str, match_kind: Option<u32>, target: &Target) -> bool {
    let uri = uri.trim();
    if uri.is_empty() {
        return false;
    }
    if let Some(package) = &target.package {
        return android_package(uri).is_some_and(|p| p.eq_ignore_ascii_case(package));
    }
    let Some(host) = target.host.as_deref() else {
        return false;
    };
    match match_kind.unwrap_or(DOMAIN) {
        DOMAIN => domain_of(uri).is_some_and(|domain| domain == base_domain(host)),
        HOST => host_port(uri).is_some_and(|saved| {
            let page = match target.port {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            };
            saved == page
        }),
        STARTS_WITH => target.url.as_deref().is_some_and(|page| {
            let same_origin = match (parse(uri), parse(page)) {
                (Some(a), Some(b)) => {
                    a.origin().is_tuple()
                        && a.origin().ascii_serialization() == b.origin().ascii_serialization()
                }
                _ => false,
            };
            same_origin && page.starts_with(uri)
        }),
        EXACT => target.url.as_deref() == Some(uri),
        _ => false,
    }
}

/// Whether any of a login's addresses match.
pub fn login_matches<'a>(
    uris: impl IntoIterator<Item = (&'a str, Option<u32>)>,
    target: &Target,
) -> bool {
    uris.into_iter()
        .any(|(uri, kind)| uri_matches(uri, kind, target))
}

/// One saved address as the Apple extension matches it without the Public
/// Suffix List: the kind of match and what to compare. Swift checks a page's
/// host against `value` (domain: equal or below it; host: equal), and the
/// full address for starts-with and exact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UriHint {
    /// `domain`, `host`, `startsWith` or `exact`.
    pub kind: String,
    /// The registrable domain, the host (with `:port`), or the address.
    pub value: String,
}

/// How the Apple extension matches a saved address; `None` for one it can't
/// (regular expressions, never, apps of another system).
pub fn uri_hint(uri: &str, match_kind: Option<u32>) -> Option<UriHint> {
    let uri = uri.trim();
    if uri.is_empty() || android_package(uri).is_some() {
        return None;
    }
    let (kind, value) = match match_kind.unwrap_or(DOMAIN) {
        DOMAIN => ("domain", domain_of(uri)?),
        HOST => ("host", host_port(uri)?),
        STARTS_WITH => ("startsWith", uri.to_string()),
        EXACT => ("exact", uri.to_string()),
        _ => return None,
    };
    Some(UriHint {
        kind: kind.into(),
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(address: &str) -> Target {
        Target::web(address)
    }

    #[test]
    fn domains() {
        assert_eq!(base_domain("a.b.example.co.uk"), "example.co.uk");
        assert_eq!(base_domain("WWW.Example.com."), "example.com");
        assert_eq!(base_domain("localhost"), "localhost");
        assert_eq!(base_domain("192.0.2.7"), "192.0.2.7");
        assert_eq!(
            domain_of("example.com/login").as_deref(),
            Some("example.com")
        );
        assert_eq!(domain_of("androidapp://com.example.app"), None);
    }

    #[test]
    fn domain_match_is_the_default() {
        let page = web("https://login.example.com/session");
        assert!(uri_matches("https://www.example.com", None, &page));
        assert!(uri_matches("example.com", Some(DOMAIN), &page));
        assert!(!uri_matches("https://example.net", None, &page));
        // Not the public suffix: another site under co.uk doesn't match.
        let uk = web("shop.example.co.uk");
        assert!(uri_matches("https://example.co.uk", None, &uk));
        assert!(!uri_matches("https://other.co.uk", None, &uk));
        // A bare host is enough for domain matches.
        assert!(uri_matches(
            "https://example.com",
            None,
            &web("accounts.example.com")
        ));
    }

    #[test]
    fn host_starts_with_exact() {
        let page = web("https://login.example.com:8443/a/b?c");
        assert!(uri_matches(
            "https://login.example.com:8443",
            Some(HOST),
            &page
        ));
        assert!(!uri_matches("https://login.example.com", Some(HOST), &page));
        assert!(!uri_matches("https://example.com:8443", Some(HOST), &page));
        assert!(uri_matches(
            "https://login.example.com:8443/a",
            Some(STARTS_WITH),
            &page
        ));
        assert!(!uri_matches(
            "http://login.example.com:8443/a",
            Some(STARTS_WITH),
            &page
        ));
        assert!(uri_matches(
            "https://login.example.com:8443/a/b?c",
            Some(EXACT),
            &page
        ));
        assert!(!uri_matches(
            "https://login.example.com:8443/a/b",
            Some(EXACT),
            &page
        ));
        // Without the full address, only domain and host can match.
        let bare = web("login.example.com");
        assert!(!uri_matches(
            "https://login.example.com/",
            Some(STARTS_WITH),
            &bare
        ));
        assert!(!uri_matches(
            "https://login.example.com",
            Some(EXACT),
            &bare
        ));
        assert!(uri_matches("https://login.example.com", Some(HOST), &bare));
    }

    #[test]
    fn never_and_regex_dont_match() {
        let page = web("https://example.com/");
        assert!(!uri_matches("https://example.com", Some(NEVER), &page));
        assert!(!uri_matches(".*", Some(REGEX), &page));
        assert!(!uri_matches("", None, &page));
    }

    #[test]
    fn android_apps() {
        let app = Target::app("com.example.app");
        assert!(uri_matches("androidapp://com.example.app", None, &app));
        assert!(!uri_matches("androidapp://com.example.other", None, &app));
        assert!(!uri_matches("https://example.com", None, &app));
        assert!(!uri_matches(
            "androidapp://com.example.app",
            None,
            &web("example.com")
        ));
    }

    #[test]
    fn hints_for_the_apple_extension() {
        assert_eq!(
            uri_hint("https://www.example.co.uk/login", None),
            Some(UriHint {
                kind: "domain".into(),
                value: "example.co.uk".into()
            })
        );
        assert_eq!(
            uri_hint("https://Login.Example.com:8443/x", Some(HOST))
                .unwrap()
                .value,
            "login.example.com:8443"
        );
        assert_eq!(
            uri_hint("https://example.com/a", Some(EXACT)).unwrap().kind,
            "exact"
        );
        assert_eq!(uri_hint("^https://", Some(REGEX)), None);
        assert_eq!(uri_hint("https://example.com", Some(NEVER)), None);
        assert_eq!(uri_hint("androidapp://com.example.app", None), None);
    }
}
