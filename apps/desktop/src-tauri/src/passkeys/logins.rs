//! Which of the vault's logins Android may offer a caller for password
//! filling (Credential Manager and the autofill service, both in
//! [`super::android_logins`]). Pure, so the host's tests cover it.
//!
//! What asks is either a web page whose address the system vouches for (a
//! browser on the privileged list) or an app. An app gets its own logins
//! (`androidapp://<package>`) and, besides those, website logins only for
//! sites whose Digital Asset Links share sign-ins with that app and its
//! certificate. Which sites to ask is [`app_sites`]: the site the package
//! name points at (`com.example.app` → `example.com`) and the site an app's
//! WebView shows — never every site in the vault, so filling doesn't tell
//! the network which sites the person has logins for.
//!
//! Matching itself is `uwulock_authenticator::autofill`'s, as on Apple.

// Only Android fills from here; the tests run everywhere.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use uwulock_authenticator::apple::listed_login;
use uwulock_authenticator::autofill::{base_domain, login_matches, Target};
use uwulock_authenticator::rpid;
use uwulock_core::vault::Item;

/// One login to offer: never its password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Offer {
    pub item_id: String,
    pub name: String,
    pub user_name: Option<String>,
}

/// Who asks, as far as it is known for sure.
#[derive(Debug, Clone)]
pub(crate) enum Asker {
    /// A page whose address a privileged browser handed over.
    Web(Target),
    /// An app, with the sites whose Digital Asset Links share sign-ins with
    /// it (already checked).
    App { package: String, sites: Vec<String> },
}

/// Whether a login may be offered at all: a password to fill, not in the
/// trash or archived, no master password re-prompt (Android only has the
/// screen lock or a biometric).
pub(crate) fn fillable(item: &Item) -> bool {
    listed_login(item)
}

fn uris(item: &Item) -> Vec<(&str, Option<u32>)> {
    item.login
        .as_ref()
        .map(|login| {
            login
                .uris
                .iter()
                .map(|u| (u.uri.as_str(), u.match_kind))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `item` is one to offer `asker`.
pub(crate) fn offered(item: &Item, asker: &Asker) -> bool {
    if !fillable(item) {
        return false;
    }
    let uris = uris(item);
    match asker {
        Asker::Web(target) => login_matches(uris.iter().copied(), target),
        Asker::App { package, sites } => {
            login_matches(uris.iter().copied(), &Target::app(package))
                || sites
                    .iter()
                    .any(|site| login_matches(uris.iter().copied(), &Target::web(site)))
        }
    }
}

/// The logins to offer, by name.
pub(crate) fn offers(items: &[Item], asker: &Asker) -> Vec<Offer> {
    let mut found: Vec<Offer> = items
        .iter()
        .filter(|item| offered(item, asker))
        .map(|item| Offer {
            item_id: item.id.clone(),
            name: item.name.to_string(),
            user_name: item
                .login
                .as_ref()
                .and_then(|l| l.username.as_ref())
                .map(|u| u.to_string())
                .filter(|u| !u.is_empty()),
        })
        .collect();
    found.sort_by_key(|offer| offer.name.to_lowercase());
    found
}

/// The site an Android package name points at: the shortest reversed
/// prefix that is a registrable domain (`com.example.app` → `example.com`,
/// `uk.co.example.app` → `example.co.uk`).
pub(crate) fn package_site(package: &str) -> Option<String> {
    let parts: Vec<&str> = package.trim().split('.').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    (2..=parts.len().min(4)).find_map(|n| {
        let candidate = parts[..n]
            .iter()
            .rev()
            .map(|p| p.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(".");
        (rpid::valid(&candidate) && base_domain(&candidate) == candidate).then_some(candidate)
    })
}

/// The sites whose Digital Asset Links to ask for an app: the package's own
/// and, inside a WebView, the page's (its registrable domain). At most two.
pub(crate) fn app_sites(package: &str, web_domain: Option<&str>) -> Vec<String> {
    let mut sites = Vec::new();
    if let Some(site) = package_site(package) {
        sites.push(site);
    }
    if let Some(host) = web_domain.map(str::trim).filter(|h| !h.is_empty()) {
        let site = base_domain(host);
        if rpid::valid(&site) && !sites.contains(&site) {
            sites.push(site);
        }
    }
    sites
}

/// A browser's page as a web target: `https` unless it said otherwise.
pub(crate) fn web_target(scheme: Option<&str>, host: &str) -> Option<Target> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    let scheme = scheme
        .map(str::trim)
        .filter(|s| matches!(*s, "http" | "https"))
        .unwrap_or("https");
    let target = Target::web(&format!("{scheme}://{host}"));
    target.host.is_some().then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwulock_core::vault::{ItemKind, LoginUri, Secret};

    fn login(
        id: &str,
        name: &str,
        user: &str,
        password: &str,
        uris: &[(&str, Option<u32>)],
    ) -> Item {
        let mut item = Item::new(ItemKind::Login);
        item.id = id.into();
        item.name = Secret::new(name.into());
        let login = item.login.as_mut().unwrap();
        login.username = Some(Secret::new(user.into()));
        login.password = Some(Secret::new(password.into()));
        login.uris = uris
            .iter()
            .map(|(uri, kind)| LoginUri {
                uri: Secret::new((*uri).into()),
                match_kind: *kind,
                checksum: None,
            })
            .collect();
        item
    }

    fn web(address: &str) -> Asker {
        Asker::Web(Target::web(address))
    }

    #[test]
    fn web_pages_get_their_site_only() {
        let items = vec![
            login(
                "1",
                "Example",
                "ann",
                "pw",
                &[("https://www.example.com", None)],
            ),
            login("2", "Other", "bob", "pw", &[("https://example.net", None)]),
            login(
                "3",
                "App",
                "cy",
                "pw",
                &[("androidapp://com.example.app", None)],
            ),
        ];
        let found = offers(&items, &web("https://login.example.com"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item_id, "1");
        assert_eq!(found[0].user_name.as_deref(), Some("ann"));
    }

    #[test]
    fn skipped_logins() {
        let site = &[("https://example.com", None)];
        let mut trashed = login("1", "a", "u", "pw", site);
        trashed.deleted = true;
        let mut archived = login("2", "b", "u", "pw", site);
        archived.archived_date = Some("2026-01-01T00:00:00Z".into());
        let mut reprompt = login("3", "c", "u", "pw", site);
        reprompt.reprompt = true;
        let no_password = login("4", "d", "u", "", site);
        let fine = login("5", "e", "", "pw", site);
        let items = vec![trashed, archived, reprompt, no_password, fine];
        let found = offers(&items, &web("example.com"));
        assert_eq!(
            found.iter().map(|o| o.item_id.as_str()).collect::<Vec<_>>(),
            ["5"]
        );
        // An empty user name is none.
        assert_eq!(found[0].user_name, None);
    }

    #[test]
    fn apps_get_their_own_and_vouched_sites() {
        let items = vec![
            login("1", "Web", "ann", "pw", &[("https://example.com", None)]),
            login(
                "2",
                "App",
                "bob",
                "pw",
                &[("androidapp://com.example.app", None)],
            ),
            login("3", "Other", "cy", "pw", &[("https://example.org", None)]),
        ];
        // Without Digital Asset Links: only the app's own login.
        let alone = Asker::App {
            package: "com.example.app".into(),
            sites: Vec::new(),
        };
        let ids: Vec<_> = offers(&items, &alone)
            .into_iter()
            .map(|o| o.item_id)
            .collect();
        assert_eq!(ids, ["2"]);
        // The site shares sign-ins with the app.
        let vouched = Asker::App {
            package: "com.example.app".into(),
            sites: vec!["example.com".into()],
        };
        let ids: Vec<_> = offers(&items, &vouched)
            .into_iter()
            .map(|o| o.item_id)
            .collect();
        assert_eq!(ids, ["2", "1"]);
        // Another app gets nothing.
        let other = Asker::App {
            package: "com.example.other".into(),
            sites: Vec::new(),
        };
        assert!(offers(&items, &other).is_empty());
    }

    #[test]
    fn sorted_by_name() {
        let site = &[("https://example.com", None)];
        let items = vec![
            login("1", "beta", "u", "pw", site),
            login("2", "Alpha", "u", "pw", site),
        ];
        let names: Vec<_> = offers(&items, &web("example.com"))
            .into_iter()
            .map(|o| o.name)
            .collect();
        assert_eq!(names, ["Alpha", "beta"]);
    }

    #[test]
    fn package_sites() {
        assert_eq!(
            package_site("com.example.app").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            package_site("uk.co.example.app").as_deref(),
            Some("example.co.uk")
        );
        assert_eq!(package_site("org.example").as_deref(), Some("example.org"));
        assert_eq!(package_site("example"), None);
        assert_eq!(package_site("com..x"), None);
        // Never a public suffix.
        assert_eq!(package_site("uk.co"), None);
        assert_eq!(
            app_sites("com.example.app", Some("login.example.net")),
            ["example.com", "example.net"]
        );
        assert_eq!(
            app_sites("com.example.app", Some("www.example.com")),
            ["example.com"]
        );
        assert_eq!(app_sites("app", Some("co.uk")), Vec::<String>::new());
    }

    #[test]
    fn web_targets() {
        let t = web_target(None, "login.example.com").unwrap();
        assert_eq!(t.url.as_deref(), Some("https://login.example.com"));
        assert_eq!(
            web_target(Some("http"), "example.com")
                .unwrap()
                .url
                .as_deref(),
            Some("http://example.com")
        );
        // Anything but http(s) is read as https.
        assert_eq!(
            web_target(Some("javascript"), "example.com")
                .unwrap()
                .url
                .as_deref(),
            Some("https://example.com")
        );
        assert!(web_target(None, " ").is_none());
    }
}
