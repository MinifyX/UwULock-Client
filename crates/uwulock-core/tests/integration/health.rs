//! The password check (UwULock-Server's `docs/uwu-api.md` §15): the hashes
//! the breach sources get, the report, breached sites, 2FA Directory, the
//! ignore list and the cards of the review — the same rules and formats as
//! the web vault's.

use std::collections::{HashMap, HashSet};

use serde_json::json;
use uwulock_core::health::{
    self, BreachCounts, Finding, IgnoreList, Problem, ProblemKind, Report, SiteBreach, SiteIndex,
    TwofaEntry, TwofaIndex,
};
use uwulock_core::vault::{Item, ItemKind, LoginUri};
use zeroize::Zeroizing;

fn login(id: &str, name: &str, password: &str, uri: &str) -> Item {
    let mut item = Item::new(ItemKind::Login);
    item.id = id.into();
    item.name = Zeroizing::new(name.into());
    item.creation_date = Some("2020-02-02T10:00:00.000Z".into());
    let login = item.login.as_mut().unwrap();
    login.username = Some(Zeroizing::new("nyu@example.com".into()));
    login.password = Some(Zeroizing::new(password.into()));
    login.uris = vec![LoginUri {
        uri: Zeroizing::new(uri.into()),
        match_kind: None,
        checksum: None,
    }];
    item
}

#[test]
fn the_breach_sources_get_only_a_hash_prefix() {
    // HIBP's documented example: SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8.
    let (prefix, suffix) = health::hibp_prefix("password");
    assert_eq!(prefix, "5BAA6");
    assert_eq!(suffix, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
    // XposedOrNot's own example: the original Keccak-512 of "password" starts a6818b8188,
    // NIST's SHA3-512 (padding 0x06) would start b109f3bbbc.
    assert_eq!(health::xon_prefix("password"), "a6818b8188");
    assert_eq!(health::xon_prefix(""), "0eab42de4c");
    assert_eq!(health::xon_prefix("pässwört").len(), 10);

    let range = "0018A45C4D1DEF81644B54AB7F969B88D65:1\r\n1e4c9b93f3f0682250b6cf8331b7ee68fd8:3861493\r\nXYZ";
    assert_eq!(health::hibp_count(range, &suffix), 3_861_493);
    assert_eq!(health::hibp_count(range, "00000"), 0);
}

#[test]
fn the_report_finds_weak_reused_and_unsecured_logins() {
    let mut archived = login("c4", "Archived", "password", "https://archived.example.com");
    archived.archived_date = Some("2026-01-01T00:00:00Z".into());
    let mut trashed = login("c5", "Trashed", "password", "https://trash.example.com");
    trashed.deleted = true;
    let mut changed = login("c3", "Strong", "K8#vR2!qLm9$wZ4^", "shop.example.net/login");
    changed.login.as_mut().unwrap().password_revision_date = Some("2025-05-05T00:00:00Z".into());
    let mut note = Item::new(ItemKind::Note);
    note.id = "n1".into();
    let items = vec![
        login(
            "c1",
            "Shop",
            "password",
            "https://www.shop.example.com/login",
        ),
        login("c2", "Forum", "password", "http://forum.example.org"),
        changed,
        archived,
        trashed,
        note,
    ];
    let prepared = health::prepare(&items);
    let report = &prepared.report;
    assert_eq!(report.checked, 3);
    let by_id: HashMap<&str, &Finding> =
        report.findings.iter().map(|f| (f.id.as_str(), f)).collect();
    let shop = by_id["c1"];
    assert_eq!(shop.reused, 1);
    assert!(shop.weak && !shop.unsecured);
    assert_eq!(shop.host.as_deref(), Some("shop.example.com"));
    assert_eq!(
        shop.uri.as_deref(),
        Some("https://www.shop.example.com/login")
    );
    assert_eq!(shop.subtitle.as_deref(), Some("nyu@example.com"));
    assert_eq!(
        shop.password_changed.as_deref(),
        Some("2020-02-02T10:00:00.000Z")
    );
    assert_eq!(shop.breached, None);
    assert!(by_id["c2"].unsecured);
    let strong = by_id["c3"];
    assert!(!strong.weak && strong.reused == 0 && strong.bits >= health::WEAK_BITS);
    // An address without a scheme has a host, but nothing to open.
    assert_eq!(strong.host.as_deref(), Some("shop.example.net"));
    assert_eq!(strong.uri, None);
    assert_eq!(
        strong.password_changed.as_deref(),
        Some("2025-05-05T00:00:00Z")
    );
    // One prefix per password, not per login.
    assert_eq!(prepared.hibp_prefixes().len(), 2);
    assert_eq!(prepared.xon_prefixes(), {
        let mut v = vec![
            "a6818b8188".to_string(),
            health::xon_prefix("K8#vR2!qLm9$wZ4^"),
        ];
        v.sort();
        v
    });

    // The answers fill in the breaches: the most any source counted, and who saw it.
    let mut counts = BreachCounts::default();
    let range = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:10";
    for (id, count) in prepared.hibp_hits("5baa6", range) {
        counts.add(&id, count, "hibp");
    }
    for (id, count) in prepared.xon_hits("A6818B8188", 99) {
        counts.add(&id, count, "xon");
    }
    assert!(prepared.xon_hits("a6818b8188", 0).is_empty());
    let mut report = prepared.report.clone();
    counts.apply(&mut report, true, false);
    assert_eq!(report.findings[0].breached, Some(99));
    assert_eq!(report.findings[0].breach_sources, ["hibp", "xon"]);
    assert_eq!(report.findings[2].breached, Some(0));
    assert!(report.breaches_checked);

    // The JSON is the web vault's.
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["breachesChecked"], true);
    assert_eq!(
        value["findings"][0]["breachSources"],
        json!(["hibp", "xon"])
    );
    assert_eq!(
        value["findings"][0]["passwordChanged"],
        "2020-02-02T10:00:00.000Z"
    );
    let back: Report = serde_json::from_value(value).unwrap();
    assert_eq!(back, report);
}

#[test]
fn an_earlier_report_lends_its_breaches_while_the_password_is_the_same() {
    let earlier = {
        let mut report = health::prepare(&[
            login("c1", "Shop", "password", "https://shop.example.com"),
            login("c2", "Forum", "hunter2", "https://forum.example.com"),
        ])
        .report;
        let mut counts = BreachCounts::default();
        counts.add("c1", 5, "hibp");
        counts.add("c2", 7, "hibp");
        counts.apply(&mut report, true, false);
        report
    };
    let mut forum = login(
        "c2",
        "Forum",
        "n3w-and-long-Passw0rd!",
        "https://forum.example.com",
    );
    forum.login.as_mut().unwrap().password_revision_date = Some("2026-10-01T00:00:00Z".into());
    let mut now = health::prepare(&[
        login("c1", "Shop", "password", "https://shop.example.com"),
        forum,
    ])
    .report;
    health::carry_breaches(&mut now, &earlier);
    assert_eq!(now.findings[0].breached, Some(5));
    assert_eq!(
        now.findings[1].breached, None,
        "a changed password isn't known"
    );

    health::renewed(&mut now, "c1", "2026-10-01T12:00:00Z");
    let shop = &now.findings[0];
    assert_eq!(shop.breached, Some(0));
    assert!(!shop.weak && shop.reused == 0);
    assert_eq!(
        shop.password_changed.as_deref(),
        Some("2026-10-01T12:00:00Z")
    );
}

fn breach(domain: &str, date: Option<&str>, passwords: bool) -> SiteBreach {
    SiteBreach {
        domain: domain.into(),
        title: format!("{domain} {}", date.unwrap_or("?")),
        date: date.map(str::to_string),
        added: Some("2026-06-01".into()),
        passwords,
        sources: [("hibp".to_string(), format!("Leak-{domain}"))].into(),
        ..SiteBreach::default()
    }
}

#[test]
fn a_site_breach_counts_from_the_day_of_the_last_password_change() {
    let index = SiteIndex::new(&[
        breach("example.com", Some("2024-05-01"), true),
        breach("example.com", Some("2025-03-01"), true),
        breach("example.com", Some("2026-01-01"), false),
        breach("example.org", None, true),
        breach("com", Some("2026-01-01"), true),
    ]);
    let finding = |host: &str, changed: &str| Finding {
        id: "c1".into(),
        host: Some(host.into()),
        password_changed: Some(changed.into()),
        ..Finding::default()
    };
    // A subdomain finds its parent's breaches, the latest with passwords first.
    let found = index
        .after_change(&finding("login.example.com", "2024-01-01T08:00:00Z"))
        .unwrap();
    assert_eq!(found.date.as_deref(), Some("2025-03-01"));
    // The same day counts: the list only knows the day.
    assert!(index
        .after_change(&finding("example.com", "2025-03-01T23:59:00Z"))
        .is_some());
    assert!(index
        .after_change(&finding("example.com", "2025-03-02T00:00:00Z"))
        .is_none());
    // Without a date, the day the source listed it.
    assert!(index
        .after_change(&finding("www.example.org", "2026-05-31T00:00:00Z"))
        .is_some());
    // Never a bare top-level domain, an address or a local name.
    assert!(index
        .after_change(&finding("other.com", "2000-01-01"))
        .is_none());
    assert!(index.breaches_for("192.0.2.10").is_empty());
    assert!(index.breaches_for("nas").is_empty());
    assert!(index.breaches_for("[2001:db8::1]").is_empty());
    assert_eq!(index.by_xon_name("Leak-example.org"), None);
    assert_eq!(
        health::domains_up("WWW.Login.Example.COM."),
        ["login.example.com", "example.com"]
    );
}

#[test]
fn logins_for_sites_with_authenticator_codes_but_none_stored() {
    let index = TwofaIndex::new(&[
        TwofaEntry {
            domain: "example.com".into(),
            additional_domains: vec!["example.net".into()],
            name: "Example".into(),
            methods: vec!["totp".into(), "u2f".into()],
            documentation: Some("https://example.com/help/2fa".into()),
        },
        TwofaEntry {
            domain: "example.org".into(),
            name: "SMS only".into(),
            methods: vec!["sms".into()],
            ..TwofaEntry::default()
        },
    ]);
    assert_eq!(
        index.entry_for("account.example.net").unwrap().name,
        "Example"
    );
    let mut with_code = login("c2", "Has code", "x", "https://example.com");
    with_code.login.as_mut().unwrap().totp = Some(Zeroizing::new("JBSWY3DPEHPK3PXP".into()));
    let items = [
        login("c1", "Zeta", "x", "https://login.example.com"),
        with_code,
        login("c3", "Alpha", "x", "https://example.net"),
        login("c4", "SMS", "x", "https://example.org"),
    ];
    let missing = health::missing_twofa(&items, &index);
    let names: Vec<_> = missing.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "Zeta"]);
    assert_eq!(missing[1].host, "login.example.com");
}

#[test]
fn the_ignore_list_keeps_what_it_does_not_know_and_drops_unknown_kinds() {
    let stored = json!({
        "version": 1,
        "fromTheFuture": { "x": 1 },
        "ignored": [
            { "itemId": "c1", "kind": "weak", "since": "2026-10-01T08:00:00.000Z", "note": "mine" },
            { "itemId": "c2", "kind": "somethingNew", "since": "2026-10-01T08:00:00.000Z" },
            { "itemId": "c3" },
            "junk"
        ]
    });
    let mut list = IgnoreList::parse(&stored.to_string());
    assert_eq!(list.ignored.len(), 1);
    assert!(list.is_ignored("c1", ProblemKind::Weak));
    assert!(!list.is_ignored("c1", ProblemKind::Reused));
    list.ignore("c1", ProblemKind::Weak, "later");
    list.ignore("c2", ProblemKind::SiteBreach, "2026-10-01T09:00:00.000Z");
    assert_eq!(list.ignored.len(), 2, "once per login and kind");
    let out: serde_json::Value = serde_json::from_str(&list.to_json()).unwrap();
    assert_eq!(out["version"], 1);
    assert_eq!(out["fromTheFuture"], json!({ "x": 1 }));
    assert_eq!(out["ignored"][0]["note"], "mine");
    assert_eq!(out["ignored"][1]["kind"], "siteBreach");
    assert_eq!(out["ignored"][1]["itemId"], "c2");
    list.unignore("c1", ProblemKind::Weak);
    list.tidy(&HashSet::from(["c1"]));
    assert!(list.ignored.is_empty());
    assert_eq!(IgnoreList::parse("not json"), IgnoreList::default());
}

#[test]
fn the_cards_come_worst_first_without_what_is_ignored() {
    let finding = |id: &str, name: &str| Finding {
        id: id.into(),
        name: name.into(),
        host: Some(format!("{id}.example.com")),
        password_changed: Some("2020-01-01T00:00:00Z".into()),
        breached: Some(0),
        ..Finding::default()
    };
    let report = Report {
        findings: vec![
            Finding {
                weak: true,
                bits: 20,
                unsecured: true,
                ..finding("a", "weak and http")
            },
            Finding {
                breached: Some(12),
                breach_sources: vec!["xon".into()],
                ..finding("b", "breached")
            },
            Finding {
                reused: 2,
                weak: true,
                bits: 30,
                ..finding("c", "Reused")
            },
            Finding {
                weak: true,
                bits: 30,
                ..finding("d", "also weak")
            },
            finding("e", "site"),
            finding("f", "fine"),
        ],
        checked: 6,
        breaches_checked: true,
        breaches_incomplete: false,
    };
    let sites = SiteIndex::new(&[breach("e.example.com", Some("2021-01-01"), true)]);
    let twofa = HashMap::from([("f".to_string(), None)]);
    let mut ignored = IgnoreList::default();
    let cards = health::cards(&report, Some(&sites), &twofa, &ignored);
    let order: Vec<_> = cards.iter().map(|c| c.finding.id.as_str()).collect();
    // breached > site breach > reused (+weak) > weak + http > weak > 2FA; equal ones by name.
    assert_eq!(order, ["b", "e", "c", "a", "d", "f"]);
    assert_eq!(
        serde_json::to_value(&cards[0].problems).unwrap(),
        json!([{ "kind": "breached", "count": 12, "sources": ["xon"] }])
    );
    assert!(matches!(cards[1].problems[0], Problem::SiteBreach { .. }));

    ignored.ignore("f", ProblemKind::Twofa, "now");
    ignored.ignore("a", ProblemKind::Weak, "now");
    let cards = health::cards(&report, Some(&sites), &twofa, &ignored);
    assert!(cards.iter().all(|c| c.finding.id != "f"));
    let a = cards.iter().find(|c| c.finding.id == "a").unwrap();
    assert_eq!(a.problems, vec![Problem::Unsecured]);
    // Without the list of sites, no site breach.
    assert!(health::cards(&report, None, &twofa, &ignored)
        .iter()
        .all(|c| c.finding.id != "e"));
    assert!(ProblemKind::Weak.about_the_password() && !ProblemKind::Twofa.about_the_password());
}

#[test]
fn only_plain_addresses_are_asked_about() {
    for good in ["nyu@example.com", " Nyu.Mio+tag@mail.example.org "] {
        assert!(health::is_address(good), "{good}");
    }
    for bad in [
        "nyu",
        "nyu@localhost",
        "nyu@@example.com",
        "a b@example.com",
        "nyu@exa_mple.com",
        "@example.com",
        "nyu@example..com",
    ] {
        assert!(!health::is_address(bad), "{bad}");
    }
}
