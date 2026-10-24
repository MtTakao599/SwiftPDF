//! Current-user DPAPI for chairman seals.
//!
//! [`protect_user`] calls `CryptProtectData` with user scope. The same Windows user on this PC
//! can decrypt it, including after a password change. Another Windows user, and a copy of the
//! file on another PC, cannot. An administrator who can act as this user often can. This is not
//! a defence against the person at the keyboard, and it does not cover the original image file
//! or an operating-system backup of the ciphertext.

/// Encrypt `data` for the current Windows user. The error is a fixed sentence and does not
/// include `data`.
pub fn protect_user(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    #[cfg(windows)]
    {
        windows_dpapi::encrypt_data(data, windows_dpapi::Scope::User, None).map_err(|_| "印影を暗号化できませんでした")
    }
    #[cfg(not(windows))]
    {
        let _ = data;
        Err("印影の暗号化は Windows でのみ使えます")
    }
}

/// Decrypt a blob from [`protect_user`]. The error is a fixed sentence and does not include the
/// blob or the plaintext.
pub fn unprotect_user(blob: &[u8]) -> Result<Vec<u8>, &'static str> {
    #[cfg(windows)]
    {
        windows_dpapi::decrypt_data(blob, windows_dpapi::Scope::User, None).map_err(|_| "印影を復号できませんでした")
    }
    #[cfg(not(windows))]
    {
        let _ = blob;
        Err("印影の暗号化は Windows でのみ使えます")
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn ciphertext_bound_to_another_key_is_refused() {
        let plain = b"synthetic seal store";
        assert_eq!(super::unprotect_user(&super::protect_user(plain).unwrap()).unwrap(), plain);
        let other_key = windows_dpapi::encrypt_data(plain, windows_dpapi::Scope::User, Some(b"another key")).unwrap();
        assert!(super::unprotect_user(&other_key).is_err());
        let machine = windows_dpapi::encrypt_data(plain, windows_dpapi::Scope::Machine, Some(b"another key")).unwrap();
        assert!(super::unprotect_user(&machine).is_err());
    }
}
