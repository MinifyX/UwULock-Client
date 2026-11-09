//! DPAPI: sealing a few bytes for this Windows user. What Windows Hello's
//! random challenge (`hello`) and the remember token of two-step login
//! (`account`) are kept under. Elsewhere there is none, and asking fails.

/// Seals `data` with DPAPI for this Windows user.
#[cfg(windows)]
pub(crate) fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    dpapi(data, true).map(|out| out.to_vec())
}

/// Opens what [`protect`] sealed.
#[cfg(windows)]
pub(crate) fn unprotect(data: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    dpapi(data, false)
}

#[cfg(windows)]
fn dpapi(data: &[u8], seal: bool) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(|_| "too much for DPAPI".to_string())?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `data` for its length; DPAPI only reads
    // it. `output` is filled by DPAPI and freed with LocalFree below.
    let ok = unsafe {
        if seal {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(format!(
            "DPAPI said no ({})",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: DPAPI returned `cbData` bytes at `pbData`, freed right after
    // they are copied (and wiped).
    let out = unsafe {
        let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copy = zeroize::Zeroizing::new(bytes.to_vec());
        zeroize::Zeroize::zeroize(bytes);
        LocalFree(output.pbData.cast());
        copy
    };
    Ok(out)
}

#[cfg(not(windows))]
#[cfg_attr(any(mobile, target_os = "macos"), allow(dead_code))]
pub(crate) fn protect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI is only on Windows.".into())
}

#[cfg(not(windows))]
#[cfg_attr(any(mobile, target_os = "macos"), allow(dead_code))]
pub(crate) fn unprotect(_data: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    Err("DPAPI is only on Windows.".into())
}
