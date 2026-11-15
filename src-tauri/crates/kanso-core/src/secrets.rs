//! Keeps small secrets (the sign-in tokens of the store accounts) readable by the current
//! Windows user only.
//!
//! On Windows the bytes go through the Data Protection API (`CryptProtectData`): they are
//! encrypted with a key Windows derives from the user's own sign-in, so the file they end up
//! in is useless on another PC or to another user of this one. Nothing has to be remembered
//! or typed, and Kanso never holds the key.
//!
//! Kanso ships for Windows only. On other systems, where it is merely built and tested, the
//! bytes are kept as they are and labelled as such ([`Scheme::Plain`]); a Windows build
//! refuses to read them.

use serde::{Deserialize, Serialize};

/// How protected bytes were produced, stored next to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// Windows Data Protection API, current user.
    Dpapi,
    /// Not protected: development builds on systems other than Windows.
    Plain,
}

/// The scheme this build protects with.
pub const SCHEME: Scheme = if cfg!(target_os = "windows") { Scheme::Dpapi } else { Scheme::Plain };

/// Protects `secret` for the current user. `Err` carries the system's error code.
pub fn protect(secret: &[u8]) -> Result<Vec<u8>, String> {
    platform::protect(secret)
}

/// Gives back what [`protect`] was given, when `scheme` is the one this build uses and the
/// bytes were protected by this same user.
pub fn unprotect(protected: &[u8], scheme: Scheme) -> Result<Vec<u8>, String> {
    if scheme != SCHEME {
        return Err("protected with a scheme this build does not use".to_string());
    }
    platform::unprotect(protected)
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ptr;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    use windows_sys::Win32::System::Memory::LocalFree;

    fn blob_of(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, String> {
        Ok(CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(bytes.len()).map_err(|_| "too large to protect".to_string())?,
            pbData: bytes.as_ptr().cast_mut(),
        })
    }

    /// Copies what Windows allocated for `blob` and frees it.
    fn take(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if blob.pbData.is_null() {
            return Vec::new();
        }
        // SAFETY: on success the API returns a buffer of `cbData` bytes at `pbData`, owned by
        // the caller and to be released with LocalFree; it is read once and freed once.
        unsafe {
            let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec();
            LocalFree(blob.pbData as isize);
            bytes
        }
    }

    pub fn protect(secret: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob_of(secret)?;
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: `input` points at `secret`, which outlives the call and is only read; the
        // optional parameters are null; `output` is a valid place for the result.
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            // SAFETY: reads the calling thread's last error code
            return Err(format!("CryptProtectData failed ({})", unsafe { GetLastError() }));
        }
        Ok(take(output))
    }

    pub fn unprotect(protected: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob_of(protected)?;
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: as in `protect`
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            // SAFETY: reads the calling thread's last error code
            return Err(format!("CryptUnprotectData failed ({})", unsafe { GetLastError() }));
        }
        Ok(take(output))
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    pub fn protect(secret: &[u8]) -> Result<Vec<u8>, String> {
        Ok(secret.to_vec())
    }

    pub fn unprotect(protected: &[u8]) -> Result<Vec<u8>, String> {
        Ok(protected.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_protected_comes_back_for_the_same_user() {
        let secret = "eg1~a-refresh-token-with-ünïcödé".as_bytes();
        let protected = protect(secret).expect("protects");
        assert_eq!(unprotect(&protected, SCHEME).expect("unprotects"), secret);
        assert_eq!(unprotect(&protect(b"").unwrap(), SCHEME).unwrap(), b"");
    }

    /// On Windows (this runs in CI there) the protected bytes are not the secret, and bytes
    /// that were tampered with or never protected are refused.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_really_encrypts_and_refuses_what_it_did_not_produce() {
        let secret = b"a refresh token that must not be readable in the file";
        let protected = protect(secret).unwrap();
        assert!(!protected.windows(secret.len()).any(|window| window == secret));
        assert!(protected.len() > secret.len());

        let mut tampered = protected.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        assert!(unprotect(&tampered, Scheme::Dpapi).is_err());
        assert!(unprotect(secret, Scheme::Dpapi).is_err());
    }

    #[test]
    fn bytes_labelled_with_another_scheme_are_refused() {
        let other = if SCHEME == Scheme::Dpapi { Scheme::Plain } else { Scheme::Dpapi };
        assert!(unprotect(b"anything", other).is_err());
    }
}
