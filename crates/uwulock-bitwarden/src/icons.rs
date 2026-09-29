//! Item icons that don't come from the server.
//!
//! UwULock Server fetches icons of public sites itself (contract §7.1) and
//! never asks the local network. A NAS at `nas.local`, a router at
//! `192.168.1.1`, a printer called `printer`: their icons come from the
//! person's own device instead ([`device_icon`]) and are stored as an own
//! icon. [`to_png`] makes whatever a device serves — ICO, PNG, JPEG, WebP,
//! GIF — into the small PNG own icons are.
//!
//! [`for_server`] says which hosts are worth asking the server about at all:
//! it answers 404 for IP addresses, dotless names and the local and reserved
//! top-level domains without looking, so asking is only a wasted request.

use std::io::Cursor;
use std::net::IpAddr;
use std::time::Duration;

use crate::Error;

/// Own icons are at most this many pixels wide and high.
pub const MAX_PIXELS: u32 = uwulock_core::extras::ICON_MAX_PIXELS;
/// The most a device's page or icon may weigh.
const MAX_FETCH: usize = 512 * 1024;
/// The most requests one [`device_icon`] makes.
const MAX_REQUESTS: usize = 6;

/// Last labels the server refuses for automatic icons (§7.1).
const RESERVED_TLDS: &[&str] = &[
    "local",
    "lan",
    "home",
    "internal",
    "intranet",
    "localhost",
    "localdomain",
    "test",
    "invalid",
    "example",
    "onion",
    "arpa",
    "corp",
    "private",
];

/// Last labels of names on the local network.
const LOCAL_TLDS: &[&str] = &[
    "local",
    "lan",
    "home",
    "internal",
    "intranet",
    "localhost",
    "localdomain",
    "corp",
    "private",
    "arpa",
];

fn bare(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                // CGNAT, which some home networks (and Tailscale) use.
                || (a == 100 && (64..128).contains(&b))
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| local_ip(IpAddr::V4(v4)))
        }
    }
}

/// Whether `host` is on the local network: a private or loopback address, a
/// dotless name, or a name under `.local`, `.lan`, `.home`, `.internal` and
/// the like. Those icons come from the device, never from the server.
pub fn is_local_host(host: &str) -> bool {
    let host = bare(host);
    if host.is_empty() {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return local_ip(ip);
    }
    match host.rsplit_once('.') {
        None => true,
        Some((_, last)) => LOCAL_TLDS.contains(&last),
    }
}

/// Whether the server may have an automatic icon for `host` (§7.1): a
/// dotted name, not an IP address, not under a reserved top-level domain.
pub fn for_server(host: &str) -> bool {
    let host = bare(host);
    if host.is_empty() || host.len() > 253 || host.parse::<IpAddr>().is_ok() {
        return false;
    }
    let Some((_, last)) = host.rsplit_once('.') else {
        return false;
    };
    let labels_ok = host.split('.').all(|label| {
        !label.is_empty()
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b >= 0x80)
    });
    labels_ok && !RESERVED_TLDS.contains(&last)
}

/// An image as an own icon: decoded (ICO, PNG, JPEG, WebP, GIF), made at most
/// [`MAX_PIXELS`] on each side — never larger than it was — and written as
/// PNG. Refuses images that would take too much memory to decode.
pub fn to_png(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let bad = |what: &str| Error::Crypto(format!("the icon {what}"));
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| bad("isn't an image"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| bad(&format!("didn't decode: {e}")))?;
    if image.width() == 0 || image.height() == 0 {
        return Err(bad("is empty"));
    }
    let image = if image.width() > MAX_PIXELS || image.height() > MAX_PIXELS {
        image.resize(
            MAX_PIXELS,
            MAX_PIXELS,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        image
    };
    let mut out = Cursor::new(Vec::new());
    image
        .into_rgba8()
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| bad(&format!("didn't encode: {e}")))?;
    Ok(out.into_inner())
}

/// The icons a page names in its `<link rel=…>` tags, best first: touch icons
/// (they are large), then icons by their declared size, largest first. SVGs
/// are left out (they would need rendering) and so is anything that isn't
/// http or https.
pub fn icon_links(html: &str, base: &url::Url) -> Vec<url::Url> {
    let lower = html.to_ascii_lowercase();
    let mut found: Vec<(u32, url::Url)> = Vec::new();
    let mut at = 0;
    while let Some(start) = lower[at..].find("<link") {
        let start = at + start;
        let Some(end) = lower[start..].find('>') else {
            break;
        };
        let end = start + end;
        // The original text keeps the address's case; the lowered one finds tags.
        let tag = &html[start + 5..end];
        at = end;
        let attrs = attributes(tag);
        let get = |name: &str| {
            attrs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        let rel = get("rel").unwrap_or_default().to_ascii_lowercase();
        let rels: Vec<&str> = rel.split_whitespace().collect();
        let touch = rels
            .iter()
            .any(|r| *r == "apple-touch-icon" || *r == "apple-touch-icon-precomposed");
        if !touch && !rels.contains(&"icon") {
            continue;
        }
        let Some(href) = get("href").filter(|h| !h.trim().is_empty()) else {
            continue;
        };
        let kind = get("type").unwrap_or_default().to_ascii_lowercase();
        if kind.contains("svg")
            || href
                .to_ascii_lowercase()
                .split('?')
                .next()
                .is_some_and(|p| p.ends_with(".svg"))
        {
            continue;
        }
        let Ok(url) = base.join(href.trim()) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        let size = get("sizes")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| {
                s.to_ascii_lowercase()
                    .split('x')
                    .next()?
                    .parse::<u32>()
                    .ok()
            })
            .unwrap_or(if touch { 180 } else { 16 });
        let rank = if touch { 10_000 + size } else { size };
        if !found.iter().any(|(_, known)| *known == url) {
            found.push((rank, url));
        }
    }
    found.sort_by_key(|(rank, _)| std::cmp::Reverse(*rank));
    found.into_iter().map(|(_, url)| url).collect()
}

/// The attributes of a tag, names lowered, values unquoted.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = tag.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = tag[name_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                value = tag[start..i].to_string();
                i += 1;
            } else {
                let start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = tag[start..i].to_string();
            }
        }
        if !name.is_empty() {
            out.push((name, value));
        }
    }
    out
}

/// Resolves names for [`device_icon`] and keeps only addresses on the local
/// network: a name that is local by its looks (`nas.local`) but points
/// elsewhere, or a start page naming an icon on the internet, gets nothing.
/// Checked at every connection, redirects and every icon link included, so
/// a name can't change its answer between the check and the fetch.
struct LocalOnly;

impl reqwest::dns::Resolve for LocalOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let found = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let local: Vec<std::net::SocketAddr> =
                found.filter(|addr| local_ip(addr.ip())).collect();
            if local.is_empty() {
                return Err(format!("{host} isn't on the local network").into());
            }
            let addrs: reqwest::dns::Addrs = Box::new(local.into_iter());
            Ok(addrs)
        })
    }
}

/// Fetches a device's icon from the device itself: the icons its start page
/// names, then `/favicon.ico`, over the scheme the address has first (https
/// without one) and then the other. Only for [`is_local_host`] addresses that
/// also resolve to local addresses ([`LocalOnly`]); only icon links and
/// redirects within the local network; short timeouts.
///
/// Devices on the local network rarely have a certificate anyone signed, so
/// this one fetch doesn't check it: what comes back is only ever decoded as an
/// image, nothing is sent but the request for it, and it never leaves the
/// local network.
pub async fn device_icon(address: &str) -> Result<Vec<u8>, Error> {
    let with_scheme = if address.contains("://") {
        address.trim().to_string()
    } else {
        format!("https://{}", address.trim())
    };
    let url = url::Url::parse(&with_scheme)
        .map_err(|_| Error::Refused("that isn't an address".into()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Refused("that address has no host".into()))?;
    if !is_local_host(host) {
        return Err(Error::Refused(
            "only devices on the local network give their icon this way".into(),
        ));
    }
    let http = reqwest::Client::builder()
        .user_agent(format!("UwULock/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .danger_accept_invalid_certs(true)
        .dns_resolver(std::sync::Arc::new(LocalOnly))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let local = attempt.url().host_str().is_some_and(is_local_host);
            if attempt.previous().len() >= 3 || !local {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;

    let schemes: [&str; 2] = if url.scheme() == "http" {
        ["http", "https"]
    } else {
        ["https", "http"]
    };
    let mut requests = 0;
    let mut last_error = Error::Refused("the device has no icon".into());
    for scheme in schemes {
        let mut base = url.clone();
        if base.set_scheme(scheme).is_err() {
            continue;
        }
        base.set_path("/");
        base.set_query(None);
        base.set_fragment(None);
        // The start page names the icons; a device that doesn't answer at all
        // on this scheme is skipped.
        requests += 1;
        let page = match fetch(&http, base.clone()).await {
            Ok(page) => page,
            Err(error @ Error::Network(_)) => {
                last_error = error;
                continue;
            }
            Err(_) => Vec::new(),
        };
        // A start page may name icons anywhere; only local ones are asked.
        let mut candidates: Vec<url::Url> = icon_links(&String::from_utf8_lossy(&page), &base)
            .into_iter()
            .filter(|link| link.host_str().is_some_and(is_local_host))
            .collect();
        if let Ok(ico) = base.join("/favicon.ico") {
            if !candidates.contains(&ico) {
                candidates.push(ico);
            }
        }
        for candidate in candidates {
            if requests >= MAX_REQUESTS {
                return Err(last_error);
            }
            requests += 1;
            match fetch(&http, candidate)
                .await
                .and_then(|bytes| to_png(&bytes))
            {
                Ok(png) => return Ok(png),
                Err(error) => last_error = error,
            }
        }
    }
    Err(last_error)
}

/// A GET, at most [`MAX_FETCH`] bytes of it.
async fn fetch(http: &reqwest::Client, url: url::Url) -> Result<Vec<u8>, Error> {
    let mut response = http
        .get(url)
        .send()
        .await
        .map_err(crate::api::network_error)?;
    if !response.status().is_success() {
        return Err(Error::Server {
            status: response.status().as_u16(),
            message: "the device didn't have it".into(),
        });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(crate::api::network_error)? {
        if body.len() + chunk.len() > MAX_FETCH {
            return Err(Error::Refused("the device sent too much".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn large_images_shrink_small_ones_stay() {
        let out = to_png(&png(300, 200)).unwrap();
        assert_eq!(uwulock_core::extras::png_size(&out), Some((128, 85)));
        let out = to_png(&png(16, 16)).unwrap();
        assert_eq!(uwulock_core::extras::png_size(&out), Some((16, 16)));
        // Fits an own icon, sealed.
        let key = uwulock_core::crypto::SymmetricKey::generate();
        uwulock_core::extras::seal_icon(&to_png(&png(128, 128)).unwrap(), &key).unwrap();
    }

    #[test]
    fn an_ico_becomes_a_png() {
        let mut ico = Cursor::new(Vec::new());
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([200, 30, 90, 255]));
        image.write_to(&mut ico, image::ImageFormat::Ico).unwrap();
        let out = to_png(ico.get_ref()).unwrap();
        assert_eq!(uwulock_core::extras::png_size(&out), Some((32, 32)));
        assert!(to_png(b"<html>not an image</html>").is_err());
    }

    #[tokio::test]
    async fn names_resolve_only_to_local_addresses() {
        use reqwest::dns::Resolve;
        let resolve = |name: &str| LocalOnly.resolve(name.parse().unwrap());
        let found: Vec<_> = resolve("localhost").await.unwrap().collect();
        assert!(!found.is_empty() && found.iter().all(|a| a.ip().is_loopback()));
        // A public address, as a name that looks local might answer.
        assert!(resolve("192.0.2.1").await.is_err());
        assert!(resolve("192.168.1.1").await.is_ok());
    }

    #[test]
    fn local_hosts() {
        for local in [
            "nas.local",
            "router.lan",
            "printer",
            "192.168.1.1",
            "10.0.0.2",
            "172.16.5.4",
            "127.0.0.1",
            "[fd00::1]",
            "fe80::1",
            "box.home.arpa",
            "100.64.0.1",
        ] {
            assert!(is_local_host(local), "{local}");
            assert!(!for_server(local), "{local}");
        }
        for public in [
            "github.com",
            "shop.example.com",
            "203.0.113.5",
            "8.8.8.8",
            "2001:db8::1",
        ] {
            assert!(!is_local_host(public), "{public}");
        }
        assert!(for_server("github.com"));
        assert!(for_server("Shop.Example.org."));
        assert!(!for_server("site.example"));
        assert!(!for_server("203.0.113.5"));
    }

    #[test]
    fn links_best_first() {
        let base = url::Url::parse("https://nas.local/app/").unwrap();
        let html = r#"<html><head>
            <LINK rel="icon" href="/small.png" sizes="16x16">
            <link rel='shortcut icon' href='favicon.ico'>
            <link rel="icon" type="image/svg+xml" href="/logo.svg">
            <link rel="apple-touch-icon" href="/touch.png">
            <link rel="icon" href="/big.png" sizes="96x96">
            <link rel="stylesheet" href="/style.css">
            <link rel="icon" href="javascript:alert(1)">
        </head></html>"#;
        let links: Vec<String> = icon_links(html, &base)
            .into_iter()
            .map(|u| u.to_string())
            .collect();
        assert_eq!(
            links,
            [
                "https://nas.local/touch.png",
                "https://nas.local/big.png",
                "https://nas.local/small.png",
                "https://nas.local/app/favicon.ico",
            ]
        );
    }
}
