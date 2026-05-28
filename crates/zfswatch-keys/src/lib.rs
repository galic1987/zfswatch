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
        Ok(())
    }

    async fn retrieve(&self, _pool: &str) -> Result<Option<SecureString>> {
        Ok(None)
    }

    async fn remove(&self, _pool: &str) -> Result<()> {
        Ok(())
    }

    async fn list_pools(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_key_vault_store() {
        let vault = MemoryKeyVault::new();
        let pass = SecureString::from_str("testpass");
        vault.store("mypool", &pass).await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_key_vault_retrieve() {
        let vault = MemoryKeyVault::new();
        let result = vault.retrieve("mypool").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_memory_key_vault_remove() {
        let vault = MemoryKeyVault::new();
        vault.remove("mypool").await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_key_vault_list_pools() {
        let vault = MemoryKeyVault::new();
        let pools = vault.list_pools().await.unwrap();
        assert!(pools.is_empty());
    }
}
