//! WebAuthn's JSON, as Android's Credential Manager hands a provider the
//! request (`PublicKeyCredentialCreationOptionsJSON`,
//! `PublicKeyCredentialRequestOptionsJSON`) and takes the answer
//! (`RegistrationResponseJSON`, `AuthenticationResponseJSON`). Binary values
//! are URL-safe base64 without padding.
//!
//! A browser on the system's list of privileged apps hands over its own
//! origin and the hash of its client data; for any other app the provider
//! writes the client data itself, with the app's origin
//! (`android:apk-key-hash:…`), and the site decides whether it trusts that
//! app.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::ctap2::{GetAssertion, MakeCredential, Rp, User, ES256};

pub fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn from_b64(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = text
        .trim()
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect();
    B64.decode(cleaned)
        .map_err(|_| "a value isn't base64".to_string())
}

#[derive(Deserialize)]
struct RpJson {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserJson {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct ParamJson {
    #[serde(rename = "type")]
    kind: String,
    alg: i64,
}

#[derive(Deserialize)]
struct DescriptorJson {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    id: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SelectionJson {
    #[serde(default)]
    user_verification: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreationJson {
    rp: RpJson,
    user: UserJson,
    challenge: String,
    #[serde(default)]
    pub_key_cred_params: Vec<ParamJson>,
    #[serde(default)]
    exclude_credentials: Vec<DescriptorJson>,
    #[serde(default)]
    authenticator_selection: Option<SelectionJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestJson {
    challenge: String,
    #[serde(default)]
    rp_id: Option<String>,
    #[serde(default)]
    allow_credentials: Vec<DescriptorJson>,
    #[serde(default)]
    user_verification: Option<String>,
}

/// A creation request: the CTAP2 view of it, and the challenge for the
/// client data.
#[derive(Debug, Clone)]
pub struct Creation {
    pub request: MakeCredential,
    /// As the site wrote it, URL-safe base64.
    pub challenge: String,
    /// `required` or `preferred`: the person proves it's them (fingerprint,
    /// face, screen lock) before the passkey is made.
    pub wants_verification: bool,
}

#[derive(Debug, Clone)]
pub struct Assertion {
    pub request: GetAssertion,
    pub challenge: String,
    pub wants_verification: bool,
}

fn descriptors(list: Vec<DescriptorJson>) -> Result<Vec<Vec<u8>>, String> {
    list.into_iter()
        .filter(|d| d.kind.as_deref().unwrap_or("public-key") == "public-key")
        .map(|d| from_b64(&d.id))
        .collect()
}

fn wants(verification: Option<&str>) -> bool {
    verification.unwrap_or("preferred") != "discouraged"
}

/// `PublicKeyCredentialCreationOptionsJSON`. The RP id defaults to
/// `default_rp` (the origin's host) when the site left it out.
pub fn parse_creation(
    json: &str,
    client_data_hash: Vec<u8>,
    default_rp: Option<&str>,
) -> Result<Creation, String> {
    let options: CreationJson =
        serde_json::from_str(json).map_err(|e| format!("not a creation request: {e}"))?;
    let rp_id = options
        .rp
        .id
        .or_else(|| default_rp.map(str::to_string))
        .filter(|id| !id.is_empty())
        .ok_or("the request names no site")?;
    let user_id = from_b64(&options.user.id)?;
    if user_id.is_empty() || user_id.len() > 64 {
        return Err("the user id is empty or longer than 64 bytes".into());
    }
    let selection = options.authenticator_selection.unwrap_or_default();
    let algorithms = if options.pub_key_cred_params.is_empty() {
        // WebAuthn: none given means ES256 and RS256.
        vec![ES256, -257]
    } else {
        options
            .pub_key_cred_params
            .iter()
            .filter(|p| p.kind == "public-key")
            .map(|p| p.alg)
            .collect()
    };
    let wants_verification = wants(selection.user_verification.as_deref());
    Ok(Creation {
        request: MakeCredential {
            client_data_hash,
            rp: Rp {
                id: rp_id,
                name: options.rp.name.filter(|n| !n.is_empty()),
            },
            user: User {
                id: user_id,
                name: options.user.name.filter(|n| !n.is_empty()),
                display_name: options.user.display_name.filter(|n| !n.is_empty()),
            },
            algorithms,
            exclude_list: descriptors(options.exclude_credentials)?,
            // Every passkey in a vault can be found by the site, whatever
            // `residentKey` says: Bitwarden's are discoverable too.
            resident_key: true,
            user_verification: wants_verification,
        },
        challenge: options.challenge,
        wants_verification,
    })
}

/// `PublicKeyCredentialRequestOptionsJSON`.
pub fn parse_request(
    json: &str,
    client_data_hash: Vec<u8>,
    default_rp: Option<&str>,
) -> Result<Assertion, String> {
    let options: RequestJson =
        serde_json::from_str(json).map_err(|e| format!("not a sign-in request: {e}"))?;
    let rp_id = options
        .rp_id
        .or_else(|| default_rp.map(str::to_string))
        .filter(|id| !id.is_empty())
        .ok_or("the request names no site")?;
    let wants_verification = wants(options.user_verification.as_deref());
    Ok(Assertion {
        request: GetAssertion {
            rp_id,
            client_data_hash,
            allow_list: descriptors(options.allow_credentials)?,
            user_presence: true,
            user_verification: wants_verification,
        },
        challenge: options.challenge,
        wants_verification,
    })
}

/// Only the RP id of a request, for listing passkeys before anything is
/// decided. `None` when the JSON doesn't say.
pub fn rp_id_of(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json).ok()?;
    value
        .get("rpId")
        .or_else(|| value.get("rp").and_then(|rp| rp.get("id")))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `CollectedClientData` as WebAuthn serialises it: `type`, `challenge`,
/// `origin`, `crossOrigin` in that order — and on Android the calling app's
/// package, as Google's own provider adds it.
pub fn client_data_json(
    kind: &str,
    challenge: &str,
    origin: &str,
    android_package: Option<&str>,
) -> String {
    let mut out = format!(
        "{{\"type\":{},\"challenge\":{},\"origin\":{},\"crossOrigin\":false",
        Value::from(kind),
        Value::from(challenge),
        Value::from(origin)
    );
    if let Some(package) = android_package {
        out.push_str(&format!(",\"androidPackageName\":{}", Value::from(package)));
    }
    out.push('}');
    out
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// An Android app's origin: the SHA-256 of its signing certificate.
pub fn apk_origin(cert_sha256: &[u8]) -> String {
    format!("android:apk-key-hash:{}", b64(cert_sha256))
}

/// Whether a web origin may use passkeys of `rp_id`: HTTPS (or `localhost`
/// over HTTP), and the RP id is its host or a parent domain of it.
pub fn origin_allows(origin: &str, rp_id: &str) -> bool {
    let rp_id = rp_id.trim().to_ascii_lowercase();
    if rp_id.is_empty()
        || rp_id.starts_with('.')
        || !rp_id.contains(|c: char| c.is_ascii_alphanumeric())
    {
        return false;
    }
    let (scheme, rest) = match origin.split_once("://") {
        Some(parts) => parts,
        None => return false,
    };
    let host_port = rest.split('/').next().unwrap_or_default();
    let host = host_port
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(host_port, |(host, _)| host)
        .to_ascii_lowercase();
    let secure = scheme == "https" || (scheme == "http" && host == "localhost");
    secure && (host == rp_id || host.ends_with(&format!(".{rp_id}")))
}

/// `RegistrationResponseJSON`. `client_data_json` is `None` when the
/// browser made the client data (it puts in its own).
pub fn registration_response(
    credential_id: &[u8],
    client_data_json: Option<&str>,
    attestation_object: &[u8],
    auth_data: &[u8],
    public_key_spki: &[u8],
) -> Value {
    json!({
        "id": b64(credential_id),
        "rawId": b64(credential_id),
        "type": "public-key",
        "authenticatorAttachment": "platform",
        "response": {
            "clientDataJSON": b64(client_data_json.unwrap_or("{}").as_bytes()),
            "attestationObject": b64(attestation_object),
            "authenticatorData": b64(auth_data),
            "publicKey": b64(public_key_spki),
            "publicKeyAlgorithm": ES256,
            "transports": ["internal", "hybrid"],
        },
        "clientExtensionResults": {"credProps": {"rk": true}},
    })
}

/// `AuthenticationResponseJSON`.
pub fn authentication_response(
    credential_id: &[u8],
    client_data_json: Option<&str>,
    auth_data: &[u8],
    signature: &[u8],
    user_handle: Option<&[u8]>,
) -> Value {
    json!({
        "id": b64(credential_id),
        "rawId": b64(credential_id),
        "type": "public-key",
        "authenticatorAttachment": "platform",
        "response": {
            "clientDataJSON": b64(client_data_json.unwrap_or("{}").as_bytes()),
            "authenticatorData": b64(auth_data),
            "signature": b64(signature),
            "userHandle": user_handle.map(b64),
        },
        "clientExtensionResults": {},
    })
}

/// Whether a site's Digital Asset Links (`/.well-known/assetlinks.json`)
/// let the Android app `package`, signed with one of `cert_sha256`, use its
/// passkeys: a statement with `delegate_permission/common.get_login_creds`
/// (or `handle_all_urls`) naming the package and a fingerprint.
pub fn asset_links_allow(json: &str, package: &str, cert_sha256: &[Vec<u8>]) -> bool {
    let Ok(Value::Array(statements)) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let fingerprints: Vec<String> = cert_sha256
        .iter()
        .map(|hash| {
            hash.iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(":")
        })
        .collect();
    statements.iter().any(|statement| {
        let relations_ok = statement
            .get("relation")
            .and_then(Value::as_array)
            .is_some_and(|relations| {
                relations.iter().any(|r| {
                    matches!(
                        r.as_str(),
                        Some("delegate_permission/common.get_login_creds")
                            | Some("delegate_permission/common.handle_all_urls")
                    )
                })
            });
        let target = statement.get("target");
        let app_ok = target
            .and_then(|t| t.get("namespace"))
            .and_then(Value::as_str)
            == Some("android_app")
            && target
                .and_then(|t| t.get("package_name"))
                .and_then(Value::as_str)
                == Some(package);
        let cert_ok = target
            .and_then(|t| t.get("sha256_cert_fingerprints"))
            .and_then(Value::as_array)
            .is_some_and(|listed| {
                listed.iter().filter_map(Value::as_str).any(|listed| {
                    fingerprints
                        .iter()
                        .any(|ours| ours.eq_ignore_ascii_case(listed))
                })
            });
        relations_ok && app_ok && cert_ok
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREATE: &str = r#"{
        "rp": {"id": "example.com", "name": "Example"},
        "user": {"id": "dXNlci0xMjM0", "name": "nyu@example.com", "displayName": "Nyu"},
        "challenge": "Y2hhbGxlbmdl",
        "pubKeyCredParams": [{"type": "public-key", "alg": -8}, {"type": "public-key", "alg": -7}],
        "excludeCredentials": [{"type": "public-key", "id": "AQID"}],
        "authenticatorSelection": {"residentKey": "required", "userVerification": "discouraged"}
    }"#;

    #[test]
    fn a_creation_request() {
        let creation = parse_creation(CREATE, vec![0; 32], None).unwrap();
        let request = &creation.request;
        assert_eq!(request.rp.id, "example.com");
        assert_eq!(request.rp.name.as_deref(), Some("Example"));
        assert_eq!(request.user.id, b"user-1234");
        assert_eq!(request.user.display_name.as_deref(), Some("Nyu"));
        assert_eq!(request.algorithms, [-8, -7]);
        assert_eq!(request.exclude_list, [vec![1, 2, 3]]);
        assert!(request.resident_key);
        assert!(!creation.wants_verification);
        assert_eq!(creation.challenge, "Y2hhbGxlbmdl");

        // No RP id: the origin's host.
        let no_rp = CREATE.replace(r#""id": "example.com", "#, "");
        assert_eq!(
            parse_creation(&no_rp, vec![], Some("login.example.com"))
                .unwrap()
                .request
                .rp
                .id,
            "login.example.com"
        );
        assert!(parse_creation(&no_rp, vec![], None).is_err());
        assert!(parse_creation("{}", vec![], None).is_err());
    }

    #[test]
    fn a_sign_in_request() {
        let json = r#"{"challenge": "abc", "rpId": "example.com",
            "allowCredentials": [{"type": "public-key", "id": "AQID"}, {"type": "other", "id": "BAU"}]}"#;
        let assertion = parse_request(json, vec![7; 32], None).unwrap();
        assert_eq!(assertion.request.rp_id, "example.com");
        assert_eq!(assertion.request.allow_list, [vec![1, 2, 3]]);
        assert!(assertion.wants_verification, "preferred by default");
        assert_eq!(rp_id_of(json).as_deref(), Some("example.com"));
        assert_eq!(rp_id_of(CREATE).as_deref(), Some("example.com"));
    }

    #[test]
    fn client_data_in_webauthns_order() {
        let data = client_data_json(
            "webauthn.get",
            "abc",
            "android:apk-key-hash:xyz",
            Some("com.example.app"),
        );
        assert_eq!(
            data,
            r#"{"type":"webauthn.get","challenge":"abc","origin":"android:apk-key-hash:xyz","crossOrigin":false,"androidPackageName":"com.example.app"}"#
        );
        // Quotes in values stay JSON.
        assert!(
            client_data_json("webauthn.create", "a\"b", "https://example.com", None)
                .contains(r#""a\"b""#)
        );
    }

    #[test]
    fn origins_and_rp_ids() {
        assert!(origin_allows("https://example.com", "example.com"));
        assert!(origin_allows(
            "https://login.example.com:8443",
            "example.com"
        ));
        assert!(origin_allows("http://localhost:3000", "localhost"));
        assert!(!origin_allows("http://example.com", "example.com"));
        assert!(!origin_allows("https://evilexample.com", "example.com"));
        assert!(!origin_allows("https://example.com", "login.example.com"));
        assert!(!origin_allows("https://example.com", ""));
        assert!(!origin_allows("example.com", "example.com"));
    }

    #[test]
    fn responses() {
        let registered = registration_response(&[1, 2, 3], Some("{}"), &[4], &[5], &[6]);
        assert_eq!(registered["id"], "AQID");
        assert_eq!(registered["response"]["publicKeyAlgorithm"], -7);
        assert_eq!(registered["response"]["clientDataJSON"], "e30");
        let signed = authentication_response(&[1, 2, 3], None, &[4], &[5], None);
        assert_eq!(signed["response"]["signature"], "BQ");
        assert!(signed["response"]["userHandle"].is_null());
    }

    #[test]
    fn digital_asset_links() {
        let cert = vec![0xab; 32];
        let fingerprint = vec!["AB"; 32].join(":");
        let links = format!(
            r#"[{{"relation": ["delegate_permission/common.get_login_creds"],
                "target": {{"namespace": "android_app", "package_name": "com.example.app",
                "sha256_cert_fingerprints": ["{fingerprint}"]}}}}]"#
        );
        assert!(asset_links_allow(
            &links,
            "com.example.app",
            std::slice::from_ref(&cert)
        ));
        assert!(!asset_links_allow(
            &links,
            "com.example.other",
            std::slice::from_ref(&cert)
        ));
        assert!(!asset_links_allow(
            &links,
            "com.example.app",
            &[vec![0xcd; 32]]
        ));
        let web_only = links.replace("get_login_creds", "something_else");
        assert!(!asset_links_allow(&web_only, "com.example.app", &[cert]));
        assert!(!asset_links_allow("not json", "com.example.app", &[]));
    }
}
