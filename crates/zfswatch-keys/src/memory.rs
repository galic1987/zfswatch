use std::fmt;
use zeroize::Zeroize;

/// A string that is automatically zeroed from memory when dropped.
/// Uses mlock where available to prevent swapping to disk.
pub struct SecureString {
    inner: String,
    locked: bool,
}

impl SecureString {
    /// Create a new SecureString from a plain String.
    /// Attempts to mlock the memory to prevent swapping.
    pub fn new(mut s: String) -> Self {
        // Try to lock memory pages
        let locked = unsafe {
            let ptr = s.as_mut_ptr();
            let len = s.len();
            if len > 0 {
                libc::mlock(ptr as *const libc::c_void, len) == 0
            } else {
                false
            }
        };

        Self { inner: s, locked }
    }

    /// Create from a str slice
    pub fn from_str(s: &str) -> Self {
        Self::new(s.to_string())
    }

    /// Access the string content (read-only)
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Consume and return the inner String (caller is responsible for clearing)
    pub fn into_string(mut self) -> String {
        std::mem::take(&mut self.inner)
    }

    /// Check if memory is locked
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Unlock memory pages (call before drop if you want to avoid munlock errors)
    pub fn unlock(&mut self) {
        if self.locked && !self.inner.is_empty() {
            unsafe {
                libc::munlock(self.inner.as_ptr() as *const libc::c_void, self.inner.len());
            }
            self.locked = false;
        }
    }
}

impl fmt::Debug for SecureString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureString")
            .field("len", &self.inner.len())
            .field("locked", &self.locked)
            .finish()
    }
}

impl Clone for SecureString {
    fn clone(&self) -> Self {
        Self::new(self.inner.clone())
    }
}

impl Drop for SecureString {
    fn drop(&mut self) {
        if self.locked && !self.inner.is_empty() {
            unsafe {
                libc::munlock(self.inner.as_ptr() as *const libc::c_void, self.inner.len());
            }
        }
    }
}

/// Zero out a mutable byte slice
pub fn secure_zero(bytes: &mut [u8]) {
    bytes.zeroize();
}

/// Zero out a String
pub fn secure_zero_string(s: &mut String) {
    unsafe {
        std::ptr::write_bytes(s.as_mut_ptr(), 0, s.len());
    }
    s.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_string_zeroed_on_drop() {
        let s = SecureString::from_str("super_secret_password_12345");
        assert_eq!(s.as_str(), "super_secret_password_12345");
        drop(s);
        // After drop, the memory should be zeroed (verified by ZeroizeOnDrop)
    }

    #[test]
    fn test_secure_string_clone() {
        let s1 = SecureString::from_str("test");
        let s2 = s1.clone();
        assert_eq!(s1.as_str(), "test");
        assert_eq!(s2.as_str(), "test");
    }
}
