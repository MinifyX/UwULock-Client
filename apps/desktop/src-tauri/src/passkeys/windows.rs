//! Windows 11: UwULock as a plugin passkey manager (experimental).
//!
//! Windows 11 24H2/25H2 (build 26100/26200, update 6725 or newer) lets a
//! passkey manager stand next to Windows Hello: webauthn.dll's plugin API.
//! The manager is a COM server (`IPluginAuthenticator`, pluginauthenticator.idl
//! in github.com/microsoft/webauthn); Windows hands it CTAP2 requests in
//! CBOR, and takes the answers encoded by its own `WebAuthNEncode…`.
//!
//! When the setting is on, UwULock
//! - registers its COM class for the user (HKCU, `LocalServer32`: Windows
//!   starts UwULock when it isn't running) and in this process,
//! - adds itself as an authenticator (`WebAuthNPluginAddAuthenticator`),
//!   which the person then switches on in Settings → Accounts → Passkeys →
//!   Advanced options,
//! - tells Windows which passkeys there are (only ids, sites and names), so
//!   they show up in Windows' own picker.
//!
//! Each request is answered like the Linux security key's: UwULock's dialog
//! asks, the master password verifies. Only requests Windows signed are
//! taken: the COM class can be called by any program of the user, so every
//! request's signature is checked with the operation signing key Windows
//! handed over when UwULock was added ([`uwulock_authenticator::opsign`]);
//! a request without a valid one is refused before anybody is asked. The
//! API is loaded at run time: on an older Windows the setting just says it
//! isn't there. Not tried on a real machine yet; docs/passkeys.md says what
//! is open.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use tauri::{AppHandle, EventId, Listener, Manager};
use uwulock_authenticator::ctap2::{self, Authenticator, Request};
use uwulock_authenticator::ctaphid::MAX_PAYLOAD;
use uwulock_authenticator::opsign::OpSignKey;
use windows::core::{
    implement, interface, IUnknown, IUnknown_Vtbl, Interface, Ref, BOOL, GUID, HRESULT,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, IClassFactory, IClassFactory_Impl,
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, REGCLS_MULTIPLEUSE,
};

use super::{Client, DesktopBackend, Provider};
use crate::vault::VaultState;

/// UwULock's plugin class: random, picked once.
const CLSID: GUID = GUID::from_u128(0x5b0c_8e7a_4f2d_4c61_9a3e_7d1b_2c6f_0e94);

const E_FAIL: HRESULT = HRESULT(0x8000_4005_u32 as i32);
const E_INVALIDARG: HRESULT = HRESULT(0x8007_0057_u32 as i32);
const E_ACCESSDENIED: HRESULT = HRESULT(0x8007_0005_u32 as i32);
/// NTE_USER_CANCELLED: the person said no.
const NTE_USER_CANCELLED: HRESULT = HRESULT(0x8009_0036_u32 as i32);
const NTE_NOT_FOUND: HRESULT = HRESULT(0x8009_0011_u32 as i32);
const CLASS_E_NOAGGREGATION: HRESULT = HRESULT(0x8004_0110_u32 as i32);

type Pcwstr = *const u16;

#[repr(C)]
pub struct OperationRequest {
    hwnd: *mut c_void,
    transaction_id: GUID,
    cb_request_signature: u32,
    pb_request_signature: *const u8,
    request_type: i32,
    cb_encoded_request: u32,
    pb_encoded_request: *const u8,
}

#[repr(C)]
pub struct OperationResponse {
    cb_encoded_response: u32,
    pb_encoded_response: *mut u8,
}

#[repr(C)]
pub struct CancelRequest {
    transaction_id: GUID,
    cb_request_signature: u32,
    pb_request_signature: *const u8,
}

#[repr(C)]
struct AddOptions {
    name: Pcwstr,
    clsid: *const GUID,
    plugin_rp_id: Pcwstr,
    light_logo_svg: Pcwstr,
    dark_logo_svg: Pcwstr,
    cb_info: u32,
    pb_info: *const u8,
    c_rp_ids: u32,
    rp_ids: *const Pcwstr,
}

#[repr(C)]
struct AddResponse {
    cb_op_sign_pub_key: u32,
    pb_op_sign_pub_key: *mut u8,
}

#[repr(C)]
struct CredentialDetails {
    cb_credential_id: u32,
    pb_credential_id: *const u8,
    rp_id: Pcwstr,
    rp_name: Pcwstr,
    cb_user_id: u32,
    pb_user_id: *const u8,
    user_name: Pcwstr,
    user_display_name: Pcwstr,
}

#[repr(C)]
struct Extensions {
    count: u32,
    extensions: *mut c_void,
}

/// WEBAUTHN_CREDENTIAL_ATTESTATION, version 8.
#[repr(C)]
struct CredentialAttestation {
    version: u32,
    format_type: Pcwstr,
    cb_authenticator_data: u32,
    pb_authenticator_data: *mut u8,
    cb_attestation: u32,
    pb_attestation: *mut u8,
    attestation_decode_type: u32,
    attestation_decode: *mut c_void,
    cb_attestation_object: u32,
    pb_attestation_object: *mut u8,
    cb_credential_id: u32,
    pb_credential_id: *mut u8,
    extensions: Extensions,
    used_transport: u32,
    ep_att: BOOL,
    large_blob_supported: BOOL,
    resident_key: BOOL,
    prf_enabled: BOOL,
    cb_unsigned_extension_outputs: u32,
    pb_unsigned_extension_outputs: *mut u8,
    hmac_secret: *mut c_void,
    third_party_payment: BOOL,
    transports: u32,
    cb_client_data_json: u32,
    pb_client_data_json: *mut u8,
    cb_registration_response_json: u32,
    pb_registration_response_json: *mut u8,
}

#[repr(C)]
struct Credential {
    version: u32,
    cb_id: u32,
    pb_id: *mut u8,
    credential_type: Pcwstr,
}

/// WEBAUTHN_ASSERTION, version 6.
#[repr(C)]
struct WebAuthnAssertion {
    version: u32,
    cb_authenticator_data: u32,
    pb_authenticator_data: *mut u8,
    cb_signature: u32,
    pb_signature: *mut u8,
    credential: Credential,
    cb_user_id: u32,
    pb_user_id: *mut u8,
    extensions: Extensions,
    cb_cred_large_blob: u32,
    pb_cred_large_blob: *mut u8,
    cred_large_blob_status: u32,
    hmac_secret: *mut c_void,
    used_transport: u32,
    cb_unsigned_extension_outputs: u32,
    pb_unsigned_extension_outputs: *mut u8,
    cb_client_data_json: u32,
    pb_client_data_json: *mut u8,
    cb_authentication_response_json: u32,
    pb_authentication_response_json: *mut u8,
}

#[repr(C)]
struct UserEntity {
    version: u32,
    cb_id: u32,
    pb_id: *mut u8,
    name: Pcwstr,
    icon: Pcwstr,
    display_name: Pcwstr,
}

#[repr(C)]
struct GetAssertionResponse {
    assertion: WebAuthnAssertion,
    user: *const UserEntity,
    number_of_credentials: u32,
    user_selected: i32,
    cb_large_blob_key: u32,
    pb_large_blob_key: *mut u8,
    cb_unsigned_extension_outputs: u32,
    pb_unsigned_extension_outputs: *mut u8,
}

type AddAuthenticator =
    unsafe extern "system" fn(*const AddOptions, *mut *mut AddResponse) -> HRESULT;
type FreeAddResponse = unsafe extern "system" fn(*mut AddResponse);
type RemoveAuthenticator = unsafe extern "system" fn(*const GUID) -> HRESULT;
type AddCredentials =
    unsafe extern "system" fn(*const GUID, u32, *const CredentialDetails) -> HRESULT;
type RemoveAllCredentials = unsafe extern "system" fn(*const GUID) -> HRESULT;
type GetOpSignPubKey = unsafe extern "system" fn(*const GUID, *mut u32, *mut *mut u8) -> HRESULT;
type FreePubKey = unsafe extern "system" fn(*mut u8);
type EncodeMakeCredential =
    unsafe extern "system" fn(*const CredentialAttestation, *mut u32, *mut *mut u8) -> HRESULT;
type EncodeGetAssertion =
    unsafe extern "system" fn(*const GetAssertionResponse, *mut u32, *mut *mut u8) -> HRESULT;

/// The plugin functions of webauthn.dll, when this Windows has them.
struct Api {
    add_authenticator: AddAuthenticator,
    free_add_response: FreeAddResponse,
    remove_authenticator: RemoveAuthenticator,
    add_credentials: AddCredentials,
    remove_all_credentials: RemoveAllCredentials,
    encode_make_credential: EncodeMakeCredential,
    encode_get_assertion: EncodeGetAssertion,
    /// The key Windows signs requests with, for a plugin added before.
    get_op_sign_pub_key: Option<GetOpSignPubKey>,
    free_pub_key: Option<FreePubKey>,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
        let name = wide("webauthn.dll");
        let module = LoadLibraryW(name.as_ptr());
        if module.is_null() {
            return None;
        }
        macro_rules! function {
            ($name:literal) => {{
                let address = GetProcAddress(module, concat!($name, "\0").as_ptr())?;
                std::mem::transmute::<unsafe extern "system" fn() -> isize, _>(address)
            }};
        }
        macro_rules! optional {
            ($name:literal) => {{
                GetProcAddress(module, concat!($name, "\0").as_ptr()).map(|address| {
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, _>(address)
                })
            }};
        }
        Some(Api {
            get_op_sign_pub_key: optional!("WebAuthNPluginGetOperationSigningPublicKey"),
            free_pub_key: optional!("WebAuthNPluginFreePublicKeyResponse"),
            add_authenticator: function!("WebAuthNPluginAddAuthenticator"),
            free_add_response: function!("WebAuthNPluginFreeAddAuthenticatorResponse"),
            remove_authenticator: function!("WebAuthNPluginRemoveAuthenticator"),
            add_credentials: function!("WebAuthNPluginAuthenticatorAddCredentials"),
            remove_all_credentials: function!("WebAuthNPluginAuthenticatorRemoveAllCredentials"),
            encode_make_credential: function!("WebAuthNEncodeMakeCredentialResponse"),
            encode_get_assertion: function!("WebAuthNEncodeGetAssertionResponse"),
        })
    })
    .as_ref()
}

pub(crate) fn problem() -> Option<String> {
    if api().is_none() {
        return Some(
            "This Windows has no plugin API for passkey managers yet (Windows 11 24H2 or 25H2 with the update from late 2025)."
                .to_string(),
        );
    }
    PROBLEM.lock().clone()
}

/// Why the plugin didn't start last time.
static PROBLEM: Mutex<Option<String>> = parking_lot::const_mutex(None);

/// The key Windows signs every request with.
static OP_SIGN_KEY: Mutex<Option<OpSignKey>> = parking_lot::const_mutex(None);

/// The request in flight, for the whole process: Windows may cancel through
/// another instance of the class than the one it asked. Its transaction id
/// and the flag that cancels it.
static IN_FLIGHT: Mutex<Option<(GUID, std::sync::Arc<AtomicBool>)>> =
    parking_lot::const_mutex(None);

/// The running plugin: the thread that holds the COM registration, and the
/// listeners that keep Windows' passkey list current.
pub(crate) struct Plugin {
    stop: std::sync::mpsc::Sender<()>,
    listeners: Vec<EventId>,
}

/// Bytes Windows handed over, copied.
unsafe fn copied(data: *const u8, length: u32) -> Option<Vec<u8>> {
    (!data.is_null() && length > 0)
        .then(|| std::slice::from_raw_parts(data, length as usize).to_vec())
}

/// The operation signing key: from the add (`AddResponse`), else asked for
/// (UwULock was added before). Never from a file: another program of the
/// user could put its own key there. Without one, the plugin is removed and
/// stays off; adding it again next time hands over a fresh key.
fn op_sign_key(api: &Api, path: &std::path::Path, from_add: Option<Vec<u8>>) -> Option<OpSignKey> {
    let asked = || unsafe {
        let get = api.get_op_sign_pub_key?;
        let (mut length, mut data) = (0u32, std::ptr::null_mut());
        let result = get(&CLSID, &mut length, &mut data);
        let bytes = if result.is_ok() {
            copied(data, length)
        } else {
            None
        };
        if let (Some(free), false) = (api.free_pub_key, data.is_null()) {
            free(data);
        }
        bytes
    };
    // Earlier builds kept a copy; it is never read.
    let _ = std::fs::remove_file(path.with_file_name("windows-opsign.key"));
    let bytes = from_add.or_else(asked)?;
    OpSignKey::parse(&bytes)
}

/// Whether Windows signed `data` (`signature`, `length`).
unsafe fn signed_by_windows(data: &[u8], signature: *const u8, length: u32) -> bool {
    let Some(signature) = copied(signature, length) else {
        return false;
    };
    OP_SIGN_KEY
        .lock()
        .as_ref()
        .is_some_and(|key| key.verify(data, &signature))
}

/// A GUID's 16 bytes as Windows keeps them in memory.
fn guid_bytes(guid: &GUID) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..4].copy_from_slice(&guid.data1.to_le_bytes());
    out[4..6].copy_from_slice(&guid.data2.to_le_bytes());
    out[6..8].copy_from_slice(&guid.data3.to_le_bytes());
    out[8..].copy_from_slice(&guid.data4);
    out
}

static SYNCING: AtomicBool = AtomicBool::new(false);

pub(crate) fn start(app: &AppHandle) -> Result<(), String> {
    let api = api().ok_or_else(|| problem().unwrap_or_default())?;
    let provider = app.state::<Provider>();
    let mut plugin = provider.plugin.lock();
    if plugin.is_some() {
        return Ok(());
    }
    register_server()?;

    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let (ready, registered) = std::sync::mpsc::channel::<Result<(), String>>();
    let handle = app.clone();
    std::thread::Builder::new()
        .name("uwulock-passkey-plugin".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let factory: IClassFactory = Factory { app: handle }.into();
            match CoRegisterClassObject(&CLSID, &factory, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE) {
                Ok(cookie) => {
                    let _ = ready.send(Ok(()));
                    let _ = stopped.recv();
                    let _ = CoRevokeClassObject(cookie);
                }
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    registered
        .recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("CoRegisterClassObject: {e}"))?;

    // Windows' list of passkey managers.
    let name = wide("UwULock");
    let rp_id = wide("uwulock.app");
    let logo = wide(&logo_svg_base64());
    let info = &ctap2::get_info()[1..];
    let options = AddOptions {
        name: name.as_ptr(),
        clsid: &CLSID,
        plugin_rp_id: rp_id.as_ptr(),
        light_logo_svg: logo.as_ptr(),
        dark_logo_svg: logo.as_ptr(),
        cb_info: info.len() as u32,
        pb_info: info.as_ptr(),
        c_rp_ids: 0,
        rp_ids: std::ptr::null(),
    };
    let mut response: *mut AddResponse = std::ptr::null_mut();
    let added = unsafe { (api.add_authenticator)(&options, &mut response) };
    let from_add = unsafe {
        response
            .as_ref()
            .and_then(|r| copied(r.pb_op_sign_pub_key, r.cb_op_sign_pub_key))
    };
    if !response.is_null() {
        unsafe { (api.free_add_response)(response) };
    }
    // Already added before: fine.
    if added.is_err() && added != HRESULT(0x800F_0001_u32 as i32) {
        tracing::info!(
            hresult = added.0,
            "WebAuthNPluginAddAuthenticator didn't take UwULock (it may be there already)"
        );
    }

    // Without Windows' key no request can be checked: none is taken.
    let Some(key) = op_sign_key(api, &provider.path, from_add) else {
        let _ = stop.send(());
        unsafe {
            let _ = (api.remove_authenticator)(&CLSID);
        }
        unregister_server();
        let why = "Windows didn't hand over the key it signs passkey requests with, so UwULock can't tell its requests from other programs' and stays off.".to_string();
        *PROBLEM.lock() = Some(why.clone());
        return Err(why);
    };
    *OP_SIGN_KEY.lock() = Some(key);
    *PROBLEM.lock() = None;

    let listeners = ["vault-changed", "vault-status"]
        .into_iter()
        .map(|event| {
            let handle = app.clone();
            app.listen(event, move |_| sync_credentials(&handle))
        })
        .collect();
    *plugin = Some(Plugin { stop, listeners });
    drop(plugin);
    sync_credentials(app);
    tracing::info!("the Windows passkey plugin is registered");
    Ok(())
}

pub(crate) fn stop(app: &AppHandle) {
    let Some(plugin) = app.state::<Provider>().plugin.lock().take() else {
        return;
    };
    let _ = plugin.stop.send(());
    for listener in plugin.listeners {
        app.unlisten(listener);
    }
    *OP_SIGN_KEY.lock() = None;
    if let Some(api) = api() {
        unsafe {
            let _ = (api.remove_all_credentials)(&CLSID);
            let _ = (api.remove_authenticator)(&CLSID);
        }
    }
    unregister_server();
}

/// HKCU\Software\Classes\CLSID\{…}\LocalServer32: Windows starts UwULock for
/// a request while it isn't running.
fn clsid_key() -> String {
    format!("Software\\Classes\\CLSID\\{{{CLSID:?}}}")
}

fn register_server() -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = wide(&format!("\"{}\" --passkey-plugin", exe.display()));
    let key = wide(&format!("{}\\LocalServer32", clsid_key()));
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            std::ptr::null(),
            REG_SZ,
            command.as_ptr().cast(),
            (command.len() * 2) as u32,
        )
    };
    if status != 0 {
        return Err(format!("couldn't register the COM server ({status})"));
    }
    Ok(())
}

fn unregister_server() {
    use windows_sys::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};
    let key = wide(&clsid_key());
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr());
    }
}

/// Tells Windows which passkeys the open vault has: ids, sites, names.
fn sync_credentials(app: &AppHandle) {
    let Some(api) = api() else { return };
    if SYNCING.swap(true, Ordering::AcqRel) {
        return;
    }
    let vault = app.state::<VaultState>();
    let mut found = Vec::new();
    if let Ok((account_id, _)) = vault.active_account() {
        let items: Vec<_> = vault
            .unlocked
            .read()
            .get(&account_id)
            .map(|u| {
                u.vault
                    .items
                    .iter()
                    .filter(|i| {
                        !i.deleted && i.login.as_ref().is_some_and(|l| l.passkey_count() > 0)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for item in &items {
            for (_, passkey) in super::passkeys_of(&vault, &account_id, item) {
                found.push(passkey);
            }
        }
        if items.is_empty() && !vault.unlocked.read().contains_key(&account_id) {
            // Locked: Windows keeps the list it had.
            SYNCING.store(false, Ordering::Release);
            return;
        }
    }
    struct Owned {
        id: Vec<u8>,
        rp_id: Vec<u16>,
        rp_name: Vec<u16>,
        user_id: Vec<u8>,
        user_name: Vec<u16>,
        display_name: Vec<u16>,
    }
    let owned: Vec<Owned> = found
        .iter()
        .filter_map(|p| {
            Some(Owned {
                id: p.credential_id_bytes().ok()?,
                rp_id: wide(&p.rp_id),
                rp_name: wide(p.rp_name.as_deref().unwrap_or(&p.rp_id)),
                user_id: p.user_handle_bytes().ok().flatten().unwrap_or_default(),
                user_name: wide(p.user_name.as_deref().unwrap_or_default()),
                display_name: wide(
                    p.user_display_name
                        .as_deref()
                        .or(p.user_name.as_deref())
                        .unwrap_or_default(),
                ),
            })
        })
        .collect();
    let details: Vec<CredentialDetails> = owned
        .iter()
        .map(|o| CredentialDetails {
            cb_credential_id: o.id.len() as u32,
            pb_credential_id: o.id.as_ptr(),
            rp_id: o.rp_id.as_ptr(),
            rp_name: o.rp_name.as_ptr(),
            cb_user_id: o.user_id.len() as u32,
            pb_user_id: o.user_id.as_ptr(),
            user_name: o.user_name.as_ptr(),
            user_display_name: o.display_name.as_ptr(),
        })
        .collect();
    unsafe {
        let _ = (api.remove_all_credentials)(&CLSID);
        if !details.is_empty() {
            let added = (api.add_credentials)(&CLSID, details.len() as u32, details.as_ptr());
            if added.is_err() {
                tracing::warn!(hresult = added.0, "Windows didn't take the passkey list");
            }
        }
    }
    SYNCING.store(false, Ordering::Release);
}

#[implement(IClassFactory)]
struct Factory {
    app: AppHandle,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let instance: IUnknown = PluginAuthenticator {
            app: self.app.clone(),
        }
        .into();
        unsafe { instance.query(iid, object).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

#[interface("d26bcf6f-b54c-43ff-9f06-d5bf148625f7")]
unsafe trait IPluginAuthenticator: IUnknown {
    fn MakeCredential(
        &self,
        request: *const OperationRequest,
        response: *mut OperationResponse,
    ) -> HRESULT;
    fn GetAssertion(
        &self,
        request: *const OperationRequest,
        response: *mut OperationResponse,
    ) -> HRESULT;
    fn CancelOperation(&self, request: *const CancelRequest) -> HRESULT;
    fn GetLockStatus(&self, status: *mut i32) -> HRESULT;
}

#[implement(IPluginAuthenticator)]
struct PluginAuthenticator {
    app: AppHandle,
}

/// Why a request isn't taken.
enum Refusal {
    Invalid,
    /// Not signed by Windows.
    Unsigned,
}

impl Refusal {
    fn hresult(&self) -> HRESULT {
        match self {
            Refusal::Invalid => E_INVALIDARG,
            Refusal::Unsigned => E_ACCESSDENIED,
        }
    }
}

/// The request's CTAP2 bytes with the command byte in front (Windows may
/// send the CBOR map alone) — only when they are no longer than a CTAP
/// message may be and Windows signed them.
fn ctap_request(command: u8, request: &OperationRequest) -> Result<Vec<u8>, Refusal> {
    if request.pb_encoded_request.is_null()
        || request.cb_encoded_request == 0
        || request.cb_encoded_request as usize > MAX_PAYLOAD
    {
        return Err(Refusal::Invalid);
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(
            request.pb_encoded_request,
            request.cb_encoded_request as usize,
        )
    };
    let signed = unsafe {
        signed_by_windows(
            bytes,
            request.pb_request_signature,
            request.cb_request_signature,
        )
    };
    if !signed {
        tracing::warn!("a passkey request without Windows' signature: refused");
        return Err(Refusal::Unsigned);
    }
    // A CBOR map starts at 0xa0; a command byte is far below.
    if bytes[0] >= 0xa0 {
        let mut out = vec![command];
        out.extend_from_slice(bytes);
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

fn status_hresult(status: u8) -> HRESULT {
    match status {
        ctap2::status::OPERATION_DENIED | ctap2::status::KEEPALIVE_CANCEL => NTE_USER_CANCELLED,
        ctap2::status::NO_CREDENTIALS => NTE_NOT_FOUND,
        ctap2::status::INVALID_CBOR | ctap2::status::MISSING_PARAMETER => E_INVALIDARG,
        _ => E_FAIL,
    }
}

impl PluginAuthenticator_Impl {
    fn run(&self, request: &[u8]) -> Result<ctap2::Request, Vec<u8>> {
        Request::parse(request).map_err(ctap2::error)
    }

    fn answer(&self, transaction: GUID, request: &[u8]) -> Vec<u8> {
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        *IN_FLIGHT.lock() = Some((transaction, std::sync::Arc::clone(&cancelled)));
        let mut tick = || {};
        let mut authenticator = Authenticator::new(DesktopBackend {
            app: self.app.clone(),
            // Signed by Windows: it is Windows asking, for a browser or app.
            client: Client {
                name: "Windows".into(),
                trusted: true,
            },
            cancelled: &cancelled,
            tick: &mut tick,
        });
        let answer = authenticator.handle(request);
        let mut flight = IN_FLIGHT.lock();
        if flight.as_ref().is_some_and(|(id, _)| *id == transaction) {
            *flight = None;
        }
        answer
    }
}

impl IPluginAuthenticator_Impl for PluginAuthenticator_Impl {
    unsafe fn MakeCredential(
        &self,
        request: *const OperationRequest,
        response: *mut OperationResponse,
    ) -> HRESULT {
        let (Some(request), Some(response)) = (request.as_ref(), response.as_mut()) else {
            return E_INVALIDARG;
        };
        *response = OperationResponse {
            cb_encoded_response: 0,
            pb_encoded_response: std::ptr::null_mut(),
        };
        let bytes = match ctap_request(ctap2::command::MAKE_CREDENTIAL, request) {
            Ok(bytes) => bytes,
            Err(refusal) => return refusal.hresult(),
        };
        if let Err(answer) = self.run(&bytes) {
            return status_hresult(answer[0]);
        }
        let answer = self.answer(request.transaction_id, &bytes);
        if answer.first() != Some(&ctap2::status::OK) {
            return status_hresult(answer.first().copied().unwrap_or(ctap2::status::OTHER));
        }
        let Ok(map) = uwulock_authenticator::cbor::Value::decode(&answer[1..]) else {
            return E_FAIL;
        };
        let Some(mut auth_data) = map.get(2).and_then(|v| v.as_bytes()).map(<[u8]>::to_vec) else {
            return E_FAIL;
        };
        // The credential id sits in the attested data: after 37 bytes and
        // the AAGUID, behind its two-byte length.
        let Some(mut credential_id) = auth_data
            .get(53..55)
            .map(|len| usize::from(u16::from_be_bytes([len[0], len[1]])))
            .and_then(|len| auth_data.get(55..55 + len))
            .map(<[u8]>::to_vec)
        else {
            return E_FAIL;
        };
        let format = wide("none");
        let attestation = CredentialAttestation {
            version: 8,
            format_type: format.as_ptr(),
            cb_authenticator_data: auth_data.len() as u32,
            pb_authenticator_data: auth_data.as_mut_ptr(),
            cb_attestation: 0,
            pb_attestation: std::ptr::null_mut(),
            attestation_decode_type: 0,
            attestation_decode: std::ptr::null_mut(),
            cb_attestation_object: 0,
            pb_attestation_object: std::ptr::null_mut(),
            cb_credential_id: credential_id.len() as u32,
            pb_credential_id: credential_id.as_mut_ptr(),
            extensions: Extensions {
                count: 0,
                extensions: std::ptr::null_mut(),
            },
            used_transport: 0x10, // WEBAUTHN_CTAP_TRANSPORT_INTERNAL
            ep_att: BOOL(0),
            large_blob_supported: BOOL(0),
            resident_key: BOOL(1),
            prf_enabled: BOOL(0),
            cb_unsigned_extension_outputs: 0,
            pb_unsigned_extension_outputs: std::ptr::null_mut(),
            hmac_secret: std::ptr::null_mut(),
            third_party_payment: BOOL(0),
            transports: 0x10,
            cb_client_data_json: 0,
            pb_client_data_json: std::ptr::null_mut(),
            cb_registration_response_json: 0,
            pb_registration_response_json: std::ptr::null_mut(),
        };
        let Some(api) = api() else { return E_FAIL };
        let mut length = 0u32;
        let mut buffer = std::ptr::null_mut();
        let encoded = (api.encode_make_credential)(&attestation, &mut length, &mut buffer);
        if encoded.is_err() {
            return encoded;
        }
        response.cb_encoded_response = length;
        response.pb_encoded_response = buffer;
        HRESULT(0)
    }

    unsafe fn GetAssertion(
        &self,
        request: *const OperationRequest,
        response: *mut OperationResponse,
    ) -> HRESULT {
        let (Some(request), Some(response)) = (request.as_ref(), response.as_mut()) else {
            return E_INVALIDARG;
        };
        *response = OperationResponse {
            cb_encoded_response: 0,
            pb_encoded_response: std::ptr::null_mut(),
        };
        let bytes = match ctap_request(ctap2::command::GET_ASSERTION, request) {
            Ok(bytes) => bytes,
            Err(refusal) => return refusal.hresult(),
        };
        if let Err(answer) = self.run(&bytes) {
            return status_hresult(answer[0]);
        }
        let answer = self.answer(request.transaction_id, &bytes);
        if answer.first() != Some(&ctap2::status::OK) {
            return status_hresult(answer.first().copied().unwrap_or(ctap2::status::OTHER));
        }
        let Ok(map) = uwulock_authenticator::cbor::Value::decode(&answer[1..]) else {
            return E_FAIL;
        };
        let bytes_of = |key: i64| map.get(key).and_then(|v| v.as_bytes()).map(<[u8]>::to_vec);
        let (Some(mut credential_id), Some(mut auth_data), Some(mut signature)) = (
            map.get(1)
                .and_then(|c| c.get_text("id"))
                .and_then(|v| v.as_bytes())
                .map(<[u8]>::to_vec),
            bytes_of(2),
            bytes_of(3),
        ) else {
            return E_FAIL;
        };
        let user = map.get(4);
        let mut user_id = user
            .and_then(|u| u.get_text("id"))
            .and_then(|v| v.as_bytes())
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        let user_name = wide(
            user.and_then(|u| u.get_text("name"))
                .and_then(|v| v.as_text())
                .unwrap_or_default(),
        );
        let display_name = wide(
            user.and_then(|u| u.get_text("displayName"))
                .and_then(|v| v.as_text())
                .unwrap_or_default(),
        );
        let public_key = wide("public-key");
        let user_entity = UserEntity {
            version: 1,
            cb_id: user_id.len() as u32,
            pb_id: user_id.as_mut_ptr(),
            name: user_name.as_ptr(),
            icon: std::ptr::null(),
            display_name: display_name.as_ptr(),
        };
        let assertion = GetAssertionResponse {
            assertion: WebAuthnAssertion {
                version: 6,
                cb_authenticator_data: auth_data.len() as u32,
                pb_authenticator_data: auth_data.as_mut_ptr(),
                cb_signature: signature.len() as u32,
                pb_signature: signature.as_mut_ptr(),
                credential: Credential {
                    version: 1,
                    cb_id: credential_id.len() as u32,
                    pb_id: credential_id.as_mut_ptr(),
                    credential_type: public_key.as_ptr(),
                },
                cb_user_id: user_id.len() as u32,
                pb_user_id: user_id.as_mut_ptr(),
                extensions: Extensions {
                    count: 0,
                    extensions: std::ptr::null_mut(),
                },
                cb_cred_large_blob: 0,
                pb_cred_large_blob: std::ptr::null_mut(),
                cred_large_blob_status: 0,
                hmac_secret: std::ptr::null_mut(),
                used_transport: 0x10,
                cb_unsigned_extension_outputs: 0,
                pb_unsigned_extension_outputs: std::ptr::null_mut(),
                cb_client_data_json: 0,
                pb_client_data_json: std::ptr::null_mut(),
                cb_authentication_response_json: 0,
                pb_authentication_response_json: std::ptr::null_mut(),
            },
            user: if user.is_some() {
                &user_entity
            } else {
                std::ptr::null()
            },
            number_of_credentials: 1,
            user_selected: 1,
            cb_large_blob_key: 0,
            pb_large_blob_key: std::ptr::null_mut(),
            cb_unsigned_extension_outputs: 0,
            pb_unsigned_extension_outputs: std::ptr::null_mut(),
        };
        let Some(api) = api() else { return E_FAIL };
        let mut length = 0u32;
        let mut buffer = std::ptr::null_mut();
        let encoded = (api.encode_get_assertion)(&assertion, &mut length, &mut buffer);
        if encoded.is_err() {
            return encoded;
        }
        response.cb_encoded_response = length;
        response.pb_encoded_response = buffer;
        HRESULT(0)
    }

    unsafe fn CancelOperation(&self, request: *const CancelRequest) -> HRESULT {
        let Some(request) = request.as_ref() else {
            return E_INVALIDARG;
        };
        // Only the request in flight, named by its transaction id, which only
        // Windows knows (a random GUID). A cancel can only end a request, so
        // that is enough; what exactly Windows signs for a cancel isn't
        // confirmed yet, so its signature is only logged.
        let flight = IN_FLIGHT.lock();
        let Some((_, cancelled)) = flight
            .as_ref()
            .filter(|(id, _)| *id == request.transaction_id)
        else {
            return E_ACCESSDENIED;
        };
        let signed = signed_by_windows(
            &guid_bytes(&request.transaction_id),
            request.pb_request_signature,
            request.cb_request_signature,
        );
        tracing::debug!(signed, "Windows cancels the passkey request");
        cancelled.store(true, Ordering::Relaxed);
        HRESULT(0)
    }

    unsafe fn GetLockStatus(&self, status: *mut i32) -> HRESULT {
        let Some(status) = status.as_mut() else {
            return E_INVALIDARG;
        };
        // PluginLocked = 0, PluginUnlocked = 1.
        *status = i32::from(super::is_open(&self.app.state::<VaultState>()));
        HRESULT(0)
    }
}

/// UwULock's symbol for Windows' list, as base64 SVG.
fn logo_svg_base64() -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .encode(include_str!("../../../../../brand/uwulock-symbol.svg"))
}
