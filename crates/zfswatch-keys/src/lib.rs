pub mod memory;

#[cfg(target_os = "linux")]
pub mod linux_keyring;

pub use memory::{SecureString, secure_zero};

#[cfg(target_os = "linux")]
pub use linux_keyring::LinuxKeyring;

use zfswatch_core::Result;

/// Abstract key storage backend
#[async_trait::async_trait]
pub trait KeyStorage: Send + Sync {
    /// Store a passphrase for a given pool/dataset
    async fn store(&self, pool: &str, passphrase: &SecureString) -> Result<()>;

    /// Retrieve a passphrase for a given pool/dataset
    async fn retrieve(&self, pool: &str) -> Result<Option<SecureString>>;

    /// Remove a stored passphrase
    async fn remove(&self, pool: &str) -> Result<()>;

    /// List pools with stored keys
    async fn list_pools(&self) -> Result<Vec<String>>;
}

/// In-memory key vault (ephemeral, cleared on daemon restart)
pub struct MemoryKeyVault;

impl MemoryKeyVault {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl KeyStorage for MemoryKeyVault {
    async fn store(&self, _pool: &str, _passphrase: &SecureString) -> Result<()> {
        // TODO: Implement secure in-memory storage with mlock
        Ok(())
    }

    async fn retrieve(&self, _pool: &str) -> Result<Option<SecureString>> {
        // TODO
        Ok(None)
    }

    async fn remove(&self, _pool: &str) -> Result<()> {
        // TODO
        Ok(())
    }

    async fn list_pools(&self) -> Result<Vec<String>> {
        // TODO
        Ok(Vec::new())
    }
}
