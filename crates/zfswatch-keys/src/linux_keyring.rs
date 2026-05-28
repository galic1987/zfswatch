use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

use tracing::{debug, error, info};

use crate::memory::SecureString;
use zfswatch_core::{Error, Result};

// Linux kernel keyring constants
const KEY_SPEC_USER_KEYRING: c_int = -4;
const KEYCTL_REVOKE: c_int = 3;
const KEYCTL_READ: c_int = 11;
const KEYCTL_SEARCH: c_int = 10;

extern "C" {
    fn add_key(
        type_: *const c_char,
        description: *const c_char,
        payload: *const c_void,
        plen: usize,
        ringid: c_int,
    ) -> c_int;

    fn keyctl(operation: c_int, ...) -> c_int;

    fn request_key(
        type_: *const c_char,
        description: *const c_char,
        callout_info: *const c_char,
        dest_keyring: c_int,
    ) -> c_int;
}

/// Linux kernel keyring storage backend
pub struct LinuxKeyring;

impl LinuxKeyring {
    pub fn new() -> Self {
        Self
    }

    fn type_user() -> CString {
        CString::new("user").unwrap()
    }

    fn key_description(pool: &str) -> CString {
        CString::new(format!("zfswatch:{pool}")).unwrap()
    }
}

impl Default for LinuxKeyring {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::KeyStorage for LinuxKeyring {
    async fn store(&self, pool: &str, passphrase: &SecureString) -> Result<()> {
        let type_ = Self::type_user();
        let desc = Self::key_description(pool);
        let payload = passphrase.as_str().as_bytes();

        let key_id = unsafe {
            add_key(
                type_.as_ptr(),
                desc.as_ptr(),
                payload.as_ptr() as *const c_void,
                payload.len(),
                KEY_SPEC_USER_KEYRING,
            )
        };

        if key_id < 0 {
            let err = std::io::Error::last_os_error();
            return Err(Error::KeyVault(format!(
                "Failed to store key in keyring: {err}"
            )));
        }

        info!("Stored passphrase for '{pool}' in kernel keyring (key_id={key_id})");
        Ok(())
    }

    async fn retrieve(&self, pool: &str) -> Result<Option<SecureString>> {
        let type_ = Self::type_user();
        let desc = Self::key_description(pool);

        let key_id = unsafe {
            request_key(
                type_.as_ptr(),
                desc.as_ptr(),
                std::ptr::null(),
                KEY_SPEC_USER_KEYRING,
            )
        };

        if key_id < 0 {
            // Key not found
            return Ok(None);
        }

        // Read the key payload
        let mut buf = vec![0u8; 4096];
        let len = unsafe {
            keyctl(KEYCTL_READ, key_id, buf.as_mut_ptr() as *mut c_void, buf.len())
        };

        if len < 0 {
            let err = std::io::Error::last_os_error();
            return Err(Error::KeyVault(format!(
                "Failed to read key from keyring: {err}"
            )));
        }

        let len = len as usize;
        buf.truncate(len);

        // Convert to SecureString
        let s = String::from_utf8(buf)
            .map_err(|e| Error::KeyVault(format!("Invalid UTF-8 in keyring: {e}")))?;

        Ok(Some(SecureString::from_str(&s)))
    }

    async fn remove(&self, pool: &str) -> Result<()> {
        let type_ = Self::type_user();
        let desc = Self::key_description(pool);

        let key_id = unsafe {
            request_key(
                type_.as_ptr(),
                desc.as_ptr(),
                std::ptr::null(),
                KEY_SPEC_USER_KEYRING,
            )
        };

        if key_id < 0 {
            // Key not found, nothing to remove
            return Ok(());
        }

        let ret = unsafe { keyctl(KEYCTL_REVOKE, key_id) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(Error::KeyVault(format!(
                "Failed to revoke key: {err}"
            )));
        }

        info!("Revoked passphrase for '{pool}' from kernel keyring");
        Ok(())
    }

    async fn list_pools(&self) -> Result<Vec<String>> {
        // The kernel keyring doesn't provide a clean enumeration API for user keys.
        // We could read /proc/keys and parse, but that requires CAP_SYS_ADMIN.
        // For now, return empty list — the caller should track known pools separately.
        Ok(Vec::new())
    }
}
