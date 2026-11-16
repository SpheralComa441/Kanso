//! Who signed a Windows program or library, as Windows itself judges it.
//!
//! Before a DLSS library takes the place of a game's own, Kanso asks Windows whether the file
//! carries a valid signature and whose it is: only a file signed by NVIDIA goes in. The check
//! is Windows' own (`WinVerifyTrust`, the one behind a file's "Digital Signatures" tab), with
//! no window shown and no revocation list fetched. Outside Windows there is nothing to ask,
//! and nothing to replace either.

use std::path::Path;

/// The name NVIDIA signs its libraries with.
pub const NVIDIA_SIGNER: &str = "NVIDIA Corporation";

/// What Windows says about a file's signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signature {
    /// A valid signature, by this signer.
    Valid(String),
    /// No signature, a broken one, or one Windows does not trust.
    Invalid,
    /// Not on Windows: nobody to ask.
    Unknown,
}

impl Signature {
    /// Whether the file may stand in for an NVIDIA library: signed by NVIDIA, or, where
    /// signatures cannot be checked at all, not refused on that ground.
    pub fn is_nvidias(&self) -> bool {
        match self {
            Signature::Valid(signer) => signer == NVIDIA_SIGNER,
            Signature::Invalid => false,
            Signature::Unknown => true,
        }
    }
}

/// The signature of the file at `path`.
#[cfg(not(target_os = "windows"))]
pub fn signature_of(_path: &Path) -> Signature {
    Signature::Unknown
}

/// The signature of the file at `path`.
#[cfg(target_os = "windows")]
pub fn signature_of(path: &Path) -> Signature {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows_sys::Win32::Security::WinTrust::{
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: every structure is zeroed and then given its own size, as the API asks; the
    // pointers handed over (`wide_path`, `file`, `data`, `action`) outlive both calls; the
    // pointers handed back are checked for null before use and are only read while the state
    // they belong to is open, which the closing call at the end releases.
    unsafe {
        let mut file: WINTRUST_FILE_INFO = std::mem::zeroed();
        file.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
        file.pcwszFilePath = wide_path.as_ptr();

        let mut data: WINTRUST_DATA = std::mem::zeroed();
        data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
        data.dwUIChoice = WTD_UI_NONE;
        data.fdwRevocationChecks = WTD_REVOKE_NONE;
        data.dwUnionChoice = WTD_CHOICE_FILE;
        data.Anonymous.pFile = &mut file;
        data.dwStateAction = WTD_STATEACTION_VERIFY;

        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        // From here on `data` is reached only through this pointer: Windows writes into it
        let data_pointer: *mut WINTRUST_DATA = &mut data;
        let status = WinVerifyTrust(0, &mut action, data_pointer as *mut c_void);

        let mut signer: Option<String> = None;
        if status == 0 {
            let provider = WTHelperProvDataFromStateData((*data_pointer).hWVTStateData);
            if !provider.is_null() {
                let chain_signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
                if !chain_signer.is_null() {
                    let certificate = WTHelperGetProvCertFromChain(chain_signer, 0);
                    if !certificate.is_null() && !(*certificate).pCert.is_null() {
                        let context = (*certificate).pCert;
                        let length = CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, std::ptr::null(), std::ptr::null_mut(), 0);
                        if length > 1 && length < 1024 {
                            let mut name = vec![0u16; length as usize];
                            let written = CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, std::ptr::null(), name.as_mut_ptr(), length);
                            if written > 1 {
                                signer = Some(String::from_utf16_lossy(&name[..written as usize - 1]));
                            }
                        }
                    }
                }
            }
        }

        // Whatever the answer, the state Windows opened for it is released
        (*data_pointer).dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(0, &mut action, data_pointer as *mut c_void);

        match signer {
            Some(name) if status == 0 => Signature::Valid(name),
            _ => Signature::Invalid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_nvidias_own_signature_lets_a_library_in() {
        assert!(Signature::Valid("NVIDIA Corporation".into()).is_nvidias());
        assert!(!Signature::Valid("NVIDIA Corporation Ltd".into()).is_nvidias());
        assert!(!Signature::Valid("Somebody Else".into()).is_nvidias());
        assert!(!Signature::Invalid.is_nvidias());
        // Where there is no Windows to ask, the question is not what stops a test
        assert!(Signature::Unknown.is_nvidias());
        #[cfg(not(target_os = "windows"))]
        assert_eq!(signature_of(Path::new("anything.dll")), Signature::Unknown);
    }
}
