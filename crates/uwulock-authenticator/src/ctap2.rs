//! CTAP2: what a browser (or Windows) asks an authenticator, and what it
//! gets back. A request is one command byte and a CBOR map with integer keys;
//! an answer is one status byte and, when it is 0, a CBOR map.
//!
//! UwULock speaks CTAP 2.0 with built-in user verification: the vault is
//! open (the master password, a fingerprint), and the person confirms each
//! request in UwULock's own window. So no PIN protocol: `options.uv` is how a
//! browser asks for verification, and a PIN never travels.
//!
//! [`Authenticator::handle`] does the protocol; a [`Backend`] — the app —
//! finds the passkeys, asks the person, and makes or signs.

use crate::cbor::{CborError, Value};
use uwulock_core::passkey::AAGUID;

/// CTAP2's command bytes.
pub mod command {
    pub const MAKE_CREDENTIAL: u8 = 0x01;
    pub const GET_ASSERTION: u8 = 0x02;
    pub const GET_INFO: u8 = 0x04;
    pub const CLIENT_PIN: u8 = 0x06;
    pub const RESET: u8 = 0x07;
    pub const GET_NEXT_ASSERTION: u8 = 0x08;
    pub const SELECTION: u8 = 0x0b;
}

/// CTAP2's status codes, the ones UwULock answers with.
pub mod status {
    pub const OK: u8 = 0x00;
    pub const INVALID_COMMAND: u8 = 0x01;
    pub const INVALID_PARAMETER: u8 = 0x02;
    pub const INVALID_LENGTH: u8 = 0x03;
    /// CTAP1_ERR_CHANNEL_BUSY: another request is being answered.
    pub const CHANNEL_BUSY: u8 = 0x06;
    pub const CBOR_UNEXPECTED_TYPE: u8 = 0x11;
    pub const INVALID_CBOR: u8 = 0x12;
    pub const MISSING_PARAMETER: u8 = 0x14;
    pub const CREDENTIAL_EXCLUDED: u8 = 0x19;
    pub const UNSUPPORTED_ALGORITHM: u8 = 0x26;
    pub const OPERATION_DENIED: u8 = 0x27;
    pub const UNSUPPORTED_OPTION: u8 = 0x2b;
    pub const INVALID_OPTION: u8 = 0x2c;
    pub const KEEPALIVE_CANCEL: u8 = 0x2d;
    pub const NO_CREDENTIALS: u8 = 0x2e;
    pub const USER_ACTION_TIMEOUT: u8 = 0x2f;
    pub const NOT_ALLOWED: u8 = 0x30;
    pub const PIN_AUTH_INVALID: u8 = 0x33;
    pub const PIN_NOT_SET: u8 = 0x35;
    pub const OTHER: u8 = 0x7f;
}

/// ES256, the one algorithm a vault's passkeys have.
pub const ES256: i64 = -7;

/// The site, as a request names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rp {
    pub id: String,
    pub name: Option<String>,
}

/// The account at the site: `id` is the user handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Vec<u8>,
    pub name: Option<String>,
    pub display_name: Option<String>,
}

impl User {
    fn to_cbor(&self) -> Value {
        let mut entries = vec![(Value::text("id"), Value::Bytes(self.id.clone()))];
        if let Some(name) = &self.name {
            entries.push((Value::text("name"), Value::text(name)));
        }
        if let Some(name) = &self.display_name {
            entries.push((Value::text("displayName"), Value::text(name)));
        }
        Value::Map(entries)
    }
}

/// `authenticatorMakeCredential`: a new passkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeCredential {
    pub client_data_hash: Vec<u8>,
    pub rp: Rp,
    pub user: User,
    /// The algorithms the site takes, in its order.
    pub algorithms: Vec<i64>,
    /// Credential ids the site already has for this account.
    pub exclude_list: Vec<Vec<u8>>,
    /// A discoverable credential ("resident key").
    pub resident_key: bool,
    pub user_verification: bool,
}

/// `authenticatorGetAssertion`: a signature from a passkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetAssertion {
    pub rp_id: String,
    pub client_data_hash: Vec<u8>,
    /// Empty: any discoverable passkey for the site.
    pub allow_list: Vec<Vec<u8>>,
    /// `false` only for a browser's silent check whether a passkey is there.
    pub user_presence: bool,
    pub user_verification: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    MakeCredential(MakeCredential),
    GetAssertion(GetAssertion),
    GetInfo,
    GetNextAssertion,
    /// "Touch the key you want to use": `authenticatorSelection` (CTAP 2.1),
    /// or — `legacy` — a 2.0 request whose `pinAuth` is empty, which answers
    /// "no PIN set" once touched.
    Selection {
        legacy: bool,
    },
    ClientPin,
    Reset,
}

/// A signature, as the backend made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    pub credential_id: Vec<u8>,
    pub auth_data: Vec<u8>,
    pub signature: Vec<u8>,
    /// The account: only for a request without an allow list, where the
    /// browser didn't know which passkey it would get.
    pub user: Option<User>,
}

/// What a status other than OK means.
pub type Status = u8;

fn cbor_status(error: CborError) -> Status {
    match error {
        CborError::Invalid => status::INVALID_CBOR,
        CborError::Unsupported => status::INVALID_CBOR,
    }
}

fn bytes(value: Option<&Value>) -> Result<Vec<u8>, Status> {
    match value {
        None => Err(status::MISSING_PARAMETER),
        Some(Value::Bytes(bytes)) => Ok(bytes.clone()),
        Some(_) => Err(status::CBOR_UNEXPECTED_TYPE),
    }
}

/// The longest name UwULock keeps from a request, in characters (CTAP lets
/// an authenticator cut names at 64 bytes).
pub const MAX_NAME: usize = 64;

/// A name for the person's eyes: without control and bidi characters (which
/// could turn "evil.example" around on screen) and at most [`MAX_NAME`]
/// characters long.
pub fn clean_name(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    *c,
                    '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_string()
}

fn optional_text(value: Option<&Value>) -> Result<Option<String>, Status> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Text(text)) => Ok(Some(clean_name(text)).filter(|t| !t.is_empty())),
        Some(_) => Err(status::CBOR_UNEXPECTED_TYPE),
    }
}

/// The rpId of a request: a host name UwULock keeps passkeys for
/// ([`crate::rpid::valid`]). Browsers check it against the page; local
/// callers don't have to, so it is checked here for everybody.
fn rp_id(value: &Value) -> Result<String, Status> {
    let rp_id = value.as_text().ok_or(status::CBOR_UNEXPECTED_TYPE)?;
    if !crate::rpid::valid(rp_id) {
        return Err(status::INVALID_PARAMETER);
    }
    Ok(rp_id.to_string())
}

fn client_data_hash(value: Option<&Value>) -> Result<Vec<u8>, Status> {
    let hash = bytes(value)?;
    if hash.len() != 32 {
        return Err(status::INVALID_LENGTH);
    }
    Ok(hash)
}

/// A list of credential descriptors: `[{"type": "public-key", "id": …}]`.
/// Other types are skipped, as the spec says.
fn credential_list(value: Option<&Value>) -> Result<Vec<Vec<u8>>, Status> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or(status::CBOR_UNEXPECTED_TYPE)?;
    let mut out = Vec::new();
    for item in items {
        if !item.is_map() {
            return Err(status::CBOR_UNEXPECTED_TYPE);
        }
        let kind = item
            .get_text("type")
            .ok_or(status::MISSING_PARAMETER)?
            .as_text()
            .ok_or(status::CBOR_UNEXPECTED_TYPE)?;
        let id = bytes(item.get_text("id"))?;
        if kind == "public-key" {
            out.push(id);
        }
    }
    Ok(out)
}

/// The options map: `rk`, `up`, `uv`, each a boolean when it is there.
fn option(options: Option<&Value>, name: &str) -> Result<Option<bool>, Status> {
    let Some(options) = options else {
        return Ok(None);
    };
    if !options.is_map() {
        return Err(status::CBOR_UNEXPECTED_TYPE);
    }
    match options.get_text(name) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(status::CBOR_UNEXPECTED_TYPE),
    }
}

/// What `pinAuth`/`pinUvAuthParam` (key 8, protocol at key 9) says: an empty
/// one asks for a touch only; a real one needs a PIN UwULock doesn't have.
fn pin_auth(params: &Value, key: i64) -> Result<Option<Request>, Status> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Bytes(auth)) if auth.is_empty() => {
            Ok(Some(Request::Selection { legacy: true }))
        }
        Some(Value::Bytes(_)) => Err(status::PIN_AUTH_INVALID),
        Some(_) => Err(status::CBOR_UNEXPECTED_TYPE),
    }
}

impl Request {
    /// A request as it arrives: the command byte, then its CBOR.
    pub fn parse(request: &[u8]) -> Result<Request, Status> {
        // No more than one CTAPHID message carries, on every way in.
        if request.len() > crate::ctaphid::MAX_PAYLOAD {
            return Err(status::INVALID_LENGTH);
        }
        let (&command, cbor) = request.split_first().ok_or(status::INVALID_LENGTH)?;
        let params = || -> Result<Value, Status> {
            let value = Value::decode(cbor).map_err(cbor_status)?;
            if !value.is_map() {
                return Err(status::CBOR_UNEXPECTED_TYPE);
            }
            Ok(value)
        };
        match command {
            command::MAKE_CREDENTIAL => {
                let params = params()?;
                let client_data_hash = client_data_hash(params.get(1))?;
                let rp = params.get(2).ok_or(status::MISSING_PARAMETER)?;
                let rp_id = rp_id(rp.get_text("id").ok_or(status::MISSING_PARAMETER)?)?;
                let user = params.get(3).ok_or(status::MISSING_PARAMETER)?;
                if !rp.is_map() || !user.is_map() {
                    return Err(status::CBOR_UNEXPECTED_TYPE);
                }
                let user_id = bytes(user.get_text("id"))?;
                if user_id.len() > 64 {
                    return Err(status::INVALID_LENGTH);
                }
                let algorithms = params
                    .get(4)
                    .ok_or(status::MISSING_PARAMETER)?
                    .as_array()
                    .ok_or(status::CBOR_UNEXPECTED_TYPE)?
                    .iter()
                    .filter(|p| p.get_text("type").and_then(Value::as_text) == Some("public-key"))
                    .filter_map(|p| p.get_text("alg").and_then(Value::as_int))
                    .collect();
                let options = params.get(7);
                if option(options, "up")? == Some(false) {
                    return Err(status::INVALID_OPTION);
                }
                if let Some(selection) = pin_auth(&params, 8)? {
                    return Ok(selection);
                }
                Ok(Request::MakeCredential(MakeCredential {
                    client_data_hash,
                    rp: Rp {
                        id: rp_id,
                        name: optional_text(rp.get_text("name"))?,
                    },
                    user: User {
                        id: user_id,
                        name: optional_text(user.get_text("name"))?,
                        display_name: optional_text(user.get_text("displayName"))?,
                    },
                    algorithms,
                    exclude_list: credential_list(params.get(5))?,
                    resident_key: option(options, "rk")?.unwrap_or(false),
                    user_verification: option(options, "uv")?.unwrap_or(false),
                }))
            }
            command::GET_ASSERTION => {
                let params = params()?;
                let rp_id = rp_id(params.get(1).ok_or(status::MISSING_PARAMETER)?)?;
                let client_data_hash = client_data_hash(params.get(2))?;
                let options = params.get(5);
                if option(options, "rk")?.is_some() {
                    return Err(status::UNSUPPORTED_OPTION);
                }
                if let Some(selection) = pin_auth(&params, 6)? {
                    return Ok(selection);
                }
                Ok(Request::GetAssertion(GetAssertion {
                    rp_id,
                    client_data_hash,
                    allow_list: credential_list(params.get(3))?,
                    user_presence: option(options, "up")?.unwrap_or(true),
                    user_verification: option(options, "uv")?.unwrap_or(false),
                }))
            }
            command::GET_INFO => Ok(Request::GetInfo),
            command::GET_NEXT_ASSERTION => Ok(Request::GetNextAssertion),
            command::SELECTION => Ok(Request::Selection { legacy: false }),
            command::CLIENT_PIN => Ok(Request::ClientPin),
            command::RESET => Ok(Request::Reset),
            _ => Err(status::INVALID_COMMAND),
        }
    }
}

/// Status OK and a CBOR map.
fn ok(value: Value) -> Vec<u8> {
    let mut out = vec![status::OK];
    out.extend_from_slice(&value.encode());
    out
}

/// A status byte alone.
pub fn error(status: Status) -> Vec<u8> {
    vec![status]
}

/// `authenticatorGetInfo`: CTAP 2.0, UwULock's AAGUID, discoverable
/// passkeys, user presence and built-in verification; ES256 only.
pub fn get_info() -> Vec<u8> {
    let option = |name: &str, on: bool| (Value::text(name), Value::Bool(on));
    ok(Value::Map(vec![
        (Value::Int(1), Value::Array(vec![Value::text("FIDO_2_0")])),
        (Value::Int(3), Value::Bytes(AAGUID.to_vec())),
        (
            Value::Int(4),
            Value::Map(vec![
                option("rk", true),
                option("up", true),
                option("uv", true),
                option("plat", false),
            ]),
        ),
        // maxMsgSize: what one CTAPHID message carries.
        (
            Value::Int(5),
            Value::Int(crate::ctaphid::MAX_PAYLOAD as i64),
        ),
        // maxCredentialCountInList, maxCredentialIdLength: a browser sends
        // the whole allow list at once instead of asking one by one.
        (Value::Int(7), Value::Int(64)),
        (Value::Int(8), Value::Int(1023)),
        (Value::Int(9), Value::Array(vec![Value::text("usb")])),
        (
            Value::Int(10),
            Value::Array(vec![Value::Map(vec![
                (Value::text("alg"), Value::Int(ES256)),
                (Value::text("type"), Value::text("public-key")),
            ])]),
        ),
    ]))
}

/// The answer to `authenticatorMakeCredential`: attestation `none`.
pub fn make_credential_response(auth_data: &[u8]) -> Vec<u8> {
    ok(Value::Map(vec![
        (Value::Int(1), Value::text("none")),
        (Value::Int(2), Value::Bytes(auth_data.to_vec())),
        (Value::Int(3), Value::Map(Vec::new())),
    ]))
}

/// The answer to `authenticatorGetAssertion`.
pub fn get_assertion_response(assertion: &Assertion) -> Vec<u8> {
    let mut entries = vec![
        (
            Value::Int(1),
            Value::Map(vec![
                (
                    Value::text("id"),
                    Value::Bytes(assertion.credential_id.clone()),
                ),
                (Value::text("type"), Value::text("public-key")),
            ]),
        ),
        (Value::Int(2), Value::Bytes(assertion.auth_data.clone())),
        (Value::Int(3), Value::Bytes(assertion.signature.clone())),
    ];
    if let Some(user) = &assertion.user {
        entries.push((Value::Int(4), user.to_cbor()));
    }
    ok(Value::Map(entries))
}

/// What the app does for the authenticator: find, ask, make, sign. Every
/// call may wait for the person; one that should stop (the browser
/// cancelled) answers [`status::KEEPALIVE_CANCEL`].
pub trait Backend {
    /// A new passkey for the site, after the person agreed. Answers the
    /// authenticator data with the attested credential. A passkey from
    /// `exclude_list` already in the vault: [`status::CREDENTIAL_EXCLUDED`]
    /// (after asking, so a site can't find out silently).
    fn make_credential(&mut self, request: &MakeCredential) -> Result<Vec<u8>, Status>;

    /// A signature. With `user_presence` false the person isn't asked, the
    /// flags say so, and the allow list is never empty (the authenticator
    /// answers that itself). Such a silent check only says whether a passkey
    /// is there: the backend must not sign with the passkey's key for it
    /// (UwULock's sign with a throwaway key, see docs/passkeys.md).
    fn get_assertion(&mut self, request: &GetAssertion) -> Result<Assertion, Status>;

    /// The person picks this authenticator ("touch your key").
    fn select(&mut self) -> Result<(), Status>;
}

/// The signature for a silent check (`up: false`): over the same bytes as a
/// real one, but with a key made for it and thrown away. A browser only
/// looks at whether the check succeeded; nobody gets a signature from the
/// passkey without the person's yes.
pub fn probe_signature(auth_data: &[u8], client_data_hash: &[u8]) -> Vec<u8> {
    use p256::ecdsa::signature::Signer as _;
    let key = p256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
    let mut message = auth_data.to_vec();
    message.extend_from_slice(client_data_hash);
    let signature: p256::ecdsa::Signature = key.sign(&message);
    signature.to_der().as_bytes().to_vec()
}

/// At most `burst` events, refilled by one every `every`: how often silent
/// checks are answered, so a caller can't run through lists of credential
/// ids. Time comes from the caller, so it is tested without waiting.
#[derive(Debug, Clone)]
pub struct Throttle {
    burst: u32,
    every: std::time::Duration,
    tokens: u32,
    since: Option<std::time::Instant>,
}

impl Throttle {
    pub const fn new(burst: u32, every: std::time::Duration) -> Self {
        Throttle {
            burst,
            every,
            tokens: burst,
            since: None,
        }
    }

    /// Whether one more is allowed at `now`.
    pub fn allow(&mut self, now: std::time::Instant) -> bool {
        let since = *self.since.get_or_insert(now);
        if !self.every.is_zero() {
            let earned = now.saturating_duration_since(since).as_nanos() / self.every.as_nanos();
            if earned > 0 {
                self.tokens = self.burst.min(
                    self.tokens
                        .saturating_add(u32::try_from(earned).unwrap_or(u32::MAX)),
                );
                self.since = Some(now);
            }
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

/// The authenticator: requests in, answers out.
pub struct Authenticator<B> {
    pub backend: B,
}

impl<B: Backend> Authenticator<B> {
    pub fn new(backend: B) -> Self {
        Authenticator { backend }
    }

    /// One request, as bytes, to one answer, as bytes.
    pub fn handle(&mut self, request: &[u8]) -> Vec<u8> {
        match Request::parse(request) {
            Err(status) => error(status),
            Ok(request) => self.handle_request(&request),
        }
    }

    fn handle_request(&mut self, request: &Request) -> Vec<u8> {
        match request {
            Request::GetInfo => get_info(),
            Request::MakeCredential(request) => {
                if !request.algorithms.contains(&ES256) {
                    return error(status::UNSUPPORTED_ALGORITHM);
                }
                match self.backend.make_credential(request) {
                    Ok(auth_data) => make_credential_response(&auth_data),
                    Err(status) => error(status),
                }
            }
            // A silent check (`up: false`) is what browsers send before the
            // real request, with the allow list they got from the site: is
            // one of these here? Without an allow list it would list the
            // person's accounts at any site to any caller, so: no. And the
            // answer never names the account.
            Request::GetAssertion(request) if !request.user_presence => {
                if request.allow_list.is_empty() {
                    return error(status::NO_CREDENTIALS);
                }
                match self.backend.get_assertion(request) {
                    Ok(mut assertion) => {
                        assertion.user = None;
                        get_assertion_response(&assertion)
                    }
                    Err(status) => error(status),
                }
            }
            Request::GetAssertion(request) => match self.backend.get_assertion(request) {
                Ok(assertion) => get_assertion_response(&assertion),
                Err(status) => error(status),
            },
            Request::Selection { legacy } => match self.backend.select() {
                Ok(()) if *legacy => error(status::PIN_NOT_SET),
                Ok(()) => error(status::OK),
                Err(status) => error(status),
            },
            // The person picked one passkey in UwULock; there is no next.
            Request::GetNextAssertion => error(status::NOT_ALLOWED),
            // No PIN: verification is UwULock's own. Reset would wipe the
            // vault's passkeys — never from a browser.
            Request::ClientPin => error(status::INVALID_COMMAND),
            Request::Reset => error(status::OPERATION_DENIED),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend that agrees to everything and remembers what it was asked.
    #[derive(Default)]
    struct Fake {
        made: Vec<MakeCredential>,
        asked: Vec<GetAssertion>,
        selected: usize,
        refuse: Option<Status>,
        /// Answer with the account even when the browser named the passkey.
        user_always: bool,
    }

    impl Backend for Fake {
        fn make_credential(&mut self, request: &MakeCredential) -> Result<Vec<u8>, Status> {
            self.made.push(request.clone());
            self.refuse.map_or(Ok(vec![0xaa; 37]), Err)
        }
        fn get_assertion(&mut self, request: &GetAssertion) -> Result<Assertion, Status> {
            self.asked.push(request.clone());
            if let Some(status) = self.refuse {
                return Err(status);
            }
            Ok(Assertion {
                credential_id: vec![1, 2, 3],
                auth_data: vec![0xbb; 37],
                signature: vec![0xcc; 70],
                user: (self.user_always || request.allow_list.is_empty()).then(|| User {
                    id: b"user".to_vec(),
                    name: Some("nyu@example.com".into()),
                    display_name: None,
                }),
            })
        }
        fn select(&mut self) -> Result<(), Status> {
            self.selected += 1;
            self.refuse.map_or(Ok(()), Err)
        }
    }

    fn request(command: u8, params: Value) -> Vec<u8> {
        let mut out = vec![command];
        out.extend_from_slice(&params.encode());
        out
    }

    fn make_credential_params() -> Vec<(Value, Value)> {
        vec![
            (Value::Int(1), Value::Bytes(vec![9; 32])),
            (
                Value::Int(2),
                Value::Map(vec![
                    (Value::text("id"), Value::text("example.com")),
                    (Value::text("name"), Value::text("Example")),
                ]),
            ),
            (
                Value::Int(3),
                Value::Map(vec![
                    (Value::text("id"), Value::Bytes(b"user".to_vec())),
                    (Value::text("name"), Value::text("nyu@example.com")),
                    (Value::text("displayName"), Value::text("Nyu")),
                ]),
            ),
            (
                Value::Int(4),
                Value::Array(vec![
                    Value::Map(vec![
                        (Value::text("alg"), Value::Int(-8)),
                        (Value::text("type"), Value::text("public-key")),
                    ]),
                    Value::Map(vec![
                        (Value::text("alg"), Value::Int(-7)),
                        (Value::text("type"), Value::text("public-key")),
                    ]),
                ]),
            ),
            (
                Value::Int(5),
                Value::Array(vec![Value::Map(vec![
                    (Value::text("id"), Value::Bytes(vec![4, 5])),
                    (Value::text("type"), Value::text("public-key")),
                ])]),
            ),
            (
                Value::Int(7),
                Value::Map(vec![
                    (Value::text("rk"), Value::Bool(true)),
                    (Value::text("uv"), Value::Bool(true)),
                ]),
            ),
        ]
    }

    fn answer(bytes: &[u8]) -> (u8, Option<Value>) {
        let (&status, rest) = bytes.split_first().unwrap();
        (
            status,
            (!rest.is_empty()).then(|| Value::decode(rest).unwrap()),
        )
    }

    #[test]
    fn get_info_says_what_uwulock_is() {
        let mut authenticator = Authenticator::new(Fake::default());
        let (status, info) = answer(&authenticator.handle(&[command::GET_INFO]));
        assert_eq!(status, status::OK);
        let info = info.unwrap();
        assert_eq!(
            info.get(1).unwrap().as_array().unwrap(),
            [Value::text("FIDO_2_0")]
        );
        assert_eq!(info.get(3).unwrap().as_bytes().unwrap(), AAGUID);
        let options = info.get(4).unwrap();
        for (name, on) in [("rk", true), ("up", true), ("uv", true), ("plat", false)] {
            assert_eq!(options.get_text(name), Some(&Value::Bool(on)), "{name}");
        }
        // No clientPin: a browser never asks for a PIN.
        assert!(options.get_text("clientPin").is_none());
    }

    #[test]
    fn make_credential() {
        let mut authenticator = Authenticator::new(Fake::default());
        let bytes = request(
            command::MAKE_CREDENTIAL,
            Value::Map(make_credential_params()),
        );
        let (status, made) = answer(&authenticator.handle(&bytes));
        assert_eq!(status, status::OK);
        let made = made.unwrap();
        assert_eq!(made.get(1), Some(&Value::text("none")));
        assert_eq!(made.get(2).unwrap().as_bytes().unwrap(), [0xaa; 37]);
        assert_eq!(made.get(3), Some(&Value::Map(vec![])));

        let asked = &authenticator.backend.made[0];
        assert_eq!(asked.rp.id, "example.com");
        assert_eq!(asked.rp.name.as_deref(), Some("Example"));
        assert_eq!(asked.user.id, b"user");
        assert_eq!(asked.user.display_name.as_deref(), Some("Nyu"));
        assert_eq!(asked.algorithms, [-8, -7]);
        assert_eq!(asked.exclude_list, [vec![4, 5]]);
        assert!(asked.resident_key && asked.user_verification);
    }

    #[test]
    fn make_credential_refusals() {
        let mut authenticator = Authenticator::new(Fake::default());
        // Only EdDSA: not something a vault keeps.
        let mut params = make_credential_params();
        params[3].1 = Value::Array(vec![Value::Map(vec![
            (Value::text("alg"), Value::Int(-8)),
            (Value::text("type"), Value::text("public-key")),
        ])]);
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        assert_eq!(
            authenticator.handle(&bytes),
            [status::UNSUPPORTED_ALGORITHM]
        );
        assert!(authenticator.backend.made.is_empty());

        // A client data hash that isn't a SHA-256.
        let mut params = make_credential_params();
        params[0].1 = Value::Bytes(vec![1; 5]);
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::INVALID_LENGTH]);

        // No user.
        let mut params = make_credential_params();
        params.remove(2);
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::MISSING_PARAMETER]);

        // up: false makes no sense for a new passkey.
        let mut params = make_credential_params();
        params[5].1 = Value::Map(vec![(Value::text("up"), Value::Bool(false))]);
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::INVALID_OPTION]);

        // Not CBOR, not a map, an unknown command, nothing at all.
        assert_eq!(
            authenticator.handle(&[command::MAKE_CREDENTIAL, 0xff]),
            [status::INVALID_CBOR]
        );
        assert_eq!(
            authenticator.handle(&[command::MAKE_CREDENTIAL, 0x01]),
            [status::CBOR_UNEXPECTED_TYPE]
        );
        assert_eq!(authenticator.handle(&[0x42]), [status::INVALID_COMMAND]);
        assert_eq!(authenticator.handle(&[]), [status::INVALID_LENGTH]);

        // The person said no.
        authenticator.backend.refuse = Some(status::OPERATION_DENIED);
        let bytes = request(
            command::MAKE_CREDENTIAL,
            Value::Map(make_credential_params()),
        );
        assert_eq!(authenticator.handle(&bytes), [status::OPERATION_DENIED]);
    }

    #[test]
    fn touch_to_select() {
        let mut authenticator = Authenticator::new(Fake::default());
        // CTAP 2.0: an empty pinAuth. Touched, but there's no PIN.
        let mut params = make_credential_params();
        params.push((Value::Int(8), Value::Bytes(vec![])));
        params.push((Value::Int(9), Value::Int(1)));
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::PIN_NOT_SET]);
        assert_eq!(authenticator.backend.selected, 1);
        assert!(authenticator.backend.made.is_empty());
        // CTAP 2.1's own command.
        assert_eq!(authenticator.handle(&[command::SELECTION]), [status::OK]);
        // A real pinAuth: UwULock has no PIN to check it against.
        let mut params = make_credential_params();
        params.push((Value::Int(8), Value::Bytes(vec![1; 16])));
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::PIN_AUTH_INVALID]);
    }

    fn get_assertion_params(allow: bool) -> Vec<(Value, Value)> {
        let mut params = vec![
            (Value::Int(1), Value::text("example.com")),
            (Value::Int(2), Value::Bytes(vec![3; 32])),
        ];
        if allow {
            params.push((
                Value::Int(3),
                Value::Array(vec![Value::Map(vec![
                    (Value::text("id"), Value::Bytes(vec![1, 2, 3])),
                    (Value::text("type"), Value::text("public-key")),
                ])]),
            ));
        }
        params
    }

    #[test]
    fn get_assertion() {
        let mut authenticator = Authenticator::new(Fake::default());
        let bytes = request(
            command::GET_ASSERTION,
            Value::Map(get_assertion_params(true)),
        );
        let (status, signed) = answer(&authenticator.handle(&bytes));
        assert_eq!(status, status::OK);
        let signed = signed.unwrap();
        assert_eq!(
            signed
                .get(1)
                .unwrap()
                .get_text("id")
                .unwrap()
                .as_bytes()
                .unwrap(),
            [1, 2, 3]
        );
        assert_eq!(signed.get(3).unwrap().as_bytes().unwrap().len(), 70);
        // The browser knew which passkey: no user.
        assert!(signed.get(4).is_none());
        let asked = &authenticator.backend.asked[0];
        assert_eq!(asked.rp_id, "example.com");
        assert!(asked.user_presence && !asked.user_verification);

        // Discoverable: the account comes along.
        let mut params = get_assertion_params(false);
        params.push((
            Value::Int(5),
            Value::Map(vec![(Value::text("uv"), Value::Bool(true))]),
        ));
        let bytes = request(command::GET_ASSERTION, Value::Map(params));
        let (_, signed) = answer(&authenticator.handle(&bytes));
        let user = signed.unwrap().get(4).unwrap().clone();
        assert_eq!(user.get_text("id").unwrap().as_bytes().unwrap(), b"user");
        let asked = &authenticator.backend.asked[1];
        assert!(asked.allow_list.is_empty() && asked.user_presence && asked.user_verification);

        // A silent check without an allow list would enumerate accounts:
        // refused before the backend hears of it (R3-3).
        let silent = || Value::Map(vec![(Value::text("up"), Value::Bool(false))]);
        let mut params = get_assertion_params(false);
        params.push((Value::Int(5), silent()));
        let bytes = request(command::GET_ASSERTION, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::NO_CREDENTIALS]);
        assert_eq!(authenticator.backend.asked.len(), 2);
        // With one, it is answered, but never with the account.
        let mut params = get_assertion_params(true);
        params.push((Value::Int(5), silent()));
        let bytes = request(command::GET_ASSERTION, Value::Map(params));
        authenticator.backend.user_always = true;
        let (status, signed) = answer(&authenticator.handle(&bytes));
        authenticator.backend.user_always = false;
        assert_eq!(status, status::OK);
        assert!(signed.unwrap().get(4).is_none());
        assert!(!authenticator.backend.asked[2].user_presence);

        // rk in a sign-in is an error; there's no next assertion.
        let mut params = get_assertion_params(false);
        params.push((
            Value::Int(5),
            Value::Map(vec![(Value::text("rk"), Value::Bool(true))]),
        ));
        let bytes = request(command::GET_ASSERTION, Value::Map(params));
        assert_eq!(authenticator.handle(&bytes), [status::UNSUPPORTED_OPTION]);
        assert_eq!(
            authenticator.handle(&[command::GET_NEXT_ASSERTION]),
            [status::NOT_ALLOWED]
        );
        assert_eq!(
            authenticator.handle(&[command::RESET]),
            [status::OPERATION_DENIED]
        );
        assert_eq!(
            authenticator.handle(&[command::CLIENT_PIN, 0xa0]),
            [status::INVALID_COMMAND]
        );

        authenticator.backend.refuse = Some(status::NO_CREDENTIALS);
        let bytes = request(
            command::GET_ASSERTION,
            Value::Map(get_assertion_params(true)),
        );
        assert_eq!(authenticator.handle(&bytes), [status::NO_CREDENTIALS]);
    }

    #[test]
    fn a_whole_round_with_a_real_passkey() {
        use p256::ecdsa::signature::Verifier;
        use p256::pkcs8::DecodePublicKey;
        use uwulock_core::passkey::{Passkey, BE, BS, UP, UV};

        /// Signs with one real passkey, like the app does.
        struct One(Passkey);
        impl Backend for One {
            fn make_credential(&mut self, _: &MakeCredential) -> Result<Vec<u8>, Status> {
                self.0
                    .authenticator_data(UP | UV | BE | BS, true)
                    .map_err(|_| status::OTHER)
            }
            fn get_assertion(&mut self, request: &GetAssertion) -> Result<Assertion, Status> {
                let auth_data = self.0.authenticator_data(UP | UV | BE | BS, false).unwrap();
                Ok(Assertion {
                    credential_id: self.0.credential_id_bytes().unwrap(),
                    signature: self.0.sign(&auth_data, &request.client_data_hash).unwrap(),
                    auth_data,
                    user: None,
                })
            }
            fn select(&mut self) -> Result<(), Status> {
                Ok(())
            }
        }

        let passkey =
            Passkey::generate("example.com", None, Some(b"u"), None, None, true, "").unwrap();
        let spki = passkey.public_key_spki().unwrap();
        let mut authenticator = Authenticator::new(One(passkey));

        let bytes = request(
            command::MAKE_CREDENTIAL,
            Value::Map(make_credential_params()),
        );
        let (_, made) = answer(&authenticator.handle(&bytes));
        let auth_data = made.unwrap().get(2).unwrap().as_bytes().unwrap().to_vec();
        assert_eq!(auth_data[32] & 0x40, 0x40, "attested");

        let bytes = request(
            command::GET_ASSERTION,
            Value::Map(get_assertion_params(true)),
        );
        let (_, signed) = answer(&authenticator.handle(&bytes));
        let signed = signed.unwrap();
        let mut message = signed.get(2).unwrap().as_bytes().unwrap().to_vec();
        message.extend_from_slice(&[3; 32]);
        let signature =
            p256::ecdsa::Signature::from_der(signed.get(3).unwrap().as_bytes().unwrap()).unwrap();
        let key =
            p256::ecdsa::VerifyingKey::from(&p256::PublicKey::from_public_key_der(&spki).unwrap());
        assert!(key.verify(&message, &signature).is_ok());
    }

    #[test]
    fn rp_ids_and_names_are_checked() {
        // R3-7: a local caller's rpId is a host name, or the request goes.
        for bad in [
            "com",
            "bank.example@evil.example",
            "github.io",
            "192.0.2.1",
            "",
        ] {
            let mut params = get_assertion_params(true);
            params[0].1 = Value::text(bad);
            let bytes = request(command::GET_ASSERTION, Value::Map(params));
            assert_eq!(
                Request::parse(&bytes),
                Err(status::INVALID_PARAMETER),
                "{bad}"
            );
            let mut params = make_credential_params();
            params[1].1 = Value::Map(vec![(Value::text("id"), Value::text(bad))]);
            let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
            assert_eq!(
                Request::parse(&bytes),
                Err(status::INVALID_PARAMETER),
                "{bad}"
            );
        }
        // R3-6: names lose bidi tricks and length.
        let mut params = make_credential_params();
        params[1].1 = Value::Map(vec![
            (Value::text("id"), Value::text("example.com")),
            (Value::text("name"), Value::text("Pay\u{202e}lap\u{0007}")),
        ]);
        params[2].1 = Value::Map(vec![
            (Value::text("id"), Value::Bytes(b"user".to_vec())),
            (Value::text("name"), Value::text(&"n".repeat(500))),
        ]);
        let bytes = request(command::MAKE_CREDENTIAL, Value::Map(params));
        let Ok(Request::MakeCredential(made)) = Request::parse(&bytes) else {
            panic!("parsed");
        };
        assert_eq!(made.rp.name.as_deref(), Some("Paylap"));
        assert_eq!(made.user.name.unwrap().chars().count(), MAX_NAME);
        // R3-8: no request is longer than one CTAPHID message.
        let mut long = vec![command::GET_INFO];
        long.resize(crate::ctaphid::MAX_PAYLOAD + 1, 0);
        assert_eq!(Request::parse(&long), Err(status::INVALID_LENGTH));
    }

    #[test]
    fn throttle_refills() {
        use std::time::{Duration, Instant};
        let mut throttle = Throttle::new(2, Duration::from_secs(10));
        let start = Instant::now();
        assert!(throttle.allow(start));
        assert!(throttle.allow(start));
        assert!(!throttle.allow(start + Duration::from_secs(5)));
        assert!(throttle.allow(start + Duration::from_secs(11)));
        assert!(!throttle.allow(start + Duration::from_secs(12)));
        assert!(throttle.allow(start + Duration::from_secs(60)));
        assert!(throttle.allow(start + Duration::from_secs(60)));
        assert!(!throttle.allow(start + Duration::from_secs(60)));
    }

    #[test]
    fn probes_get_no_real_signature() {
        let signature = probe_signature(&[1; 37], &[2; 32]);
        assert!(p256::ecdsa::Signature::from_der(&signature).is_ok());
        assert_ne!(signature, probe_signature(&[1; 37], &[2; 32]));
    }
}
