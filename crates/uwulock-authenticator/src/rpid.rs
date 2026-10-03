//! Which relying party ids UwULock takes from a caller.
//!
//! Browsers check that a page may speak for its rpId; local callers (a
//! program talking to the security key, a COM client on Windows) don't
//! have to. So every rpId that reaches the vault is checked here first: a
//! host name in lower-case letters, digits and hyphens (IDNs as `xn--`),
//! at most 253 characters, not an IP address and not a public suffix
//! (`com`, `co.uk`, `github.io` — nobody's to claim, as in the extension's
//! `shared/rpid.ts`). `localhost` and names below it are fine, as browsers
//! allow them for development.

/// Whether `rp_id` is a host name UwULock keeps passkeys for.
pub fn valid(rp_id: &str) -> bool {
    if rp_id.is_empty() || rp_id.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = rp_id.split('.').collect();
    let ldh = labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    if !ldh {
        return false;
    }
    // An IPv4 address: the last label of a host name is never all digits.
    if labels
        .last()
        .is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    if rp_id == "localhost" || rp_id.ends_with(".localhost") {
        return true;
    }
    !is_public_suffix(rp_id)
}

/// Whether `host` is a public suffix (or a single label): a name under
/// which anybody can register their own.
pub fn is_public_suffix(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if !host.contains('.') {
        return true;
    }
    psl::domain_str(&host).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names_only() {
        for good in [
            "example.com",
            "login.example.com",
            "a-b.example.org",
            "xn--nyu-9oa.example",
            "localhost",
            "app.localhost",
            "x.test",
        ] {
            assert!(valid(good), "{good}");
        }
        for bad in [
            "",
            "com",
            "co.uk",
            "github.io",
            "Example.com",
            "bank.example@evil.example",
            "evil.example/path",
            "evil.example:443",
            "evil.example?",
            "a..example.com",
            ".example.com",
            "example.com.",
            "-a.example.com",
            "192.0.2.1",
            "[2001:db8::1]",
            "exämple.com",
            "a b.example.com",
        ] {
            assert!(!valid(bad), "{bad}");
        }
        assert!(!valid(&format!("{}.example.com", "a".repeat(64))));
        assert!(!valid(&format!("{}example.com", "a.".repeat(130))));
    }

    #[test]
    fn public_suffixes() {
        assert!(is_public_suffix("com"));
        assert!(is_public_suffix("co.uk"));
        assert!(is_public_suffix("github.io"));
        assert!(!is_public_suffix("example.co.uk"));
        assert!(!is_public_suffix("nyu.github.io"));
        assert!(!is_public_suffix("example.com"));
    }
}
