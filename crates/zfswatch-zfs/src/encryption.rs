use tracing::info;

use zfswatch_core::{Error, Result};

use crate::command::CommandRunner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;

    #[tokio::test]
    async fn test_load_key() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["load-key", "tank"], "", "", true);

        let mgr = EncryptionManager::new(runner);
        mgr.load_key("tank", "mypass").await.unwrap();
    }

    #[tokio::test]
    async fn test_load_key_failure() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["load-key", "tank"], "", "wrong passphrase", false);

        let mgr = EncryptionManager::new(runner);
        let err = mgr.load_key("tank", "badpass").await.unwrap_err().to_string();
        assert!(err.contains("wrong passphrase"));
    }

    #[tokio::test]
    async fn test_unload_key() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["unload-key", "tank"], "", "", true);

        let mgr = EncryptionManager::new(runner);
        mgr.unload_key("tank").await.unwrap();
    }

    #[tokio::test]
    async fn test_change_key() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["load-key", "tank"], "", "", true);
        runner.add_response("zfs", &["change-key", "-o", "keyformat=passphrase", "-o", "keylocation=prompt", "tank"], "", "", true);

        let mgr = EncryptionManager::new(runner);
        mgr.change_key("tank", "oldpass", "newpass").await.unwrap();
    }

    #[tokio::test]
    async fn test_key_status_available() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["get", "-H", "-o", "value", "keystatus", "tank"], "available\n", "", true);

        let mgr = EncryptionManager::new(runner);
        assert!(matches!(mgr.key_status("tank").await.unwrap(), KeyStatus::Available));
    }

    #[tokio::test]
    async fn test_key_status_unavailable() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["get", "-H", "-o", "value", "keystatus", "tank"], "unavailable\n", "", true);

        let mgr = EncryptionManager::new(runner);
        assert!(matches!(mgr.key_status("tank").await.unwrap(), KeyStatus::Unavailable));
    }

    #[tokio::test]
    async fn test_get_encryption_props() {
        let runner = MockCommandRunner::new();
        let stdout = "encryption\taes-256-gcm\nkeyformat\tpassphrase\nkeylocation\tprompt\nkeystatus\tavailable\npbkdf2iters\t350000\n";
        runner.add_response("zfs", &["get", "-H", "-o", "property,value", "encryption,keyformat,keylocation,keystatus,pbkdf2iters", "tank"], stdout, "", true);

        let mgr = EncryptionManager::new(runner);
        let props = mgr.get_encryption_props("tank").await.unwrap();
        assert_eq!(props.algorithm, Some("aes-256-gcm".to_string()));
        assert_eq!(props.keyformat, Some("passphrase".to_string()));
        assert_eq!(props.keylocation, Some("prompt".to_string()));
        assert!(matches!(props.keystatus, KeyStatus::Available));
        assert_eq!(props.pbkdf2_iterations, Some(350000));
    }
}

/// Manages ZFS encryption key operations
pub struct EncryptionManager<R: CommandRunner> {
    runner: R,
}

impl<R: CommandRunner> EncryptionManager<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub async fn load_key(&self, dataset: &str, passphrase: &str) -> Result<()> {
        info!("Loading encryption key for: {dataset}");
        let (_, stderr, success) = self.runner.run("zfs", &["load-key", dataset], Some(passphrase)).await?;
        if !success {
            return Err(Error::Zfs(format!("Key load failed: {stderr}")));
        }
        info!("Encryption key loaded for: {dataset}");
        Ok(())
    }

    pub async fn unload_key(&self, dataset: &str) -> Result<()> {
        info!("Unloading encryption key for: {dataset}");
        let (_, stderr, success) = self.runner.run("zfs", &["unload-key", dataset], None).await?;
        if !success {
            return Err(Error::Zfs(format!("Key unload failed: {stderr}")));
        }
        info!("Encryption key unloaded for: {dataset}");
        Ok(())
    }

    pub async fn change_key(&self, dataset: &str, old_passphrase: &str, new_passphrase: &str) -> Result<()> {
        info!("Changing encryption key for: {dataset}");
        self.load_key(dataset, old_passphrase).await?;

        let args = vec![
            "change-key",
            "-o", "keyformat=passphrase",
            "-o", "keylocation=prompt",
            dataset,
        ];
        let (_, stderr, success) = self.runner.run("zfs", &args, Some(new_passphrase)).await?;
        if !success {
            return Err(Error::Zfs(format!("Key change failed: {stderr}")));
        }
        info!("Encryption key changed for: {dataset}");
        Ok(())
    }

    pub async fn key_status(&self, dataset: &str) -> Result<KeyStatus> {
        let (stdout, _, success) = self.runner.run("zfs", &["get", "-H", "-o", "value", "keystatus", dataset], None).await?;
        if !success { return Ok(KeyStatus::Unknown); }
        match stdout.trim() {
            "available" => Ok(KeyStatus::Available),
            "unavailable" => Ok(KeyStatus::Unavailable),
            "none" => Ok(KeyStatus::None),
            _ => Ok(KeyStatus::Unknown),
        }
    }

    pub async fn get_encryption_props(&self, dataset: &str) -> Result<EncryptionProps> {
        let (stdout, _, success) = self.runner.run(
            "zfs",
            &["get", "-H", "-o", "property,value", "encryption,keyformat,keylocation,keystatus,pbkdf2iters", dataset],
            None,
        ).await?;
        if !success { return Ok(EncryptionProps::default()); }

        let mut props = EncryptionProps::default();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if parts.len() != 2 { continue; }
            let prop = parts[0].trim();
            let value = parts[1].trim();

            match prop {
                "encryption" => props.algorithm = if value == "off" { None } else { Some(value.to_string()) },
                "keyformat" => props.keyformat = if value == "none" || value == "-" { None } else { Some(value.to_string()) },
                "keylocation" => props.keylocation = if value == "none" || value == "-" { None } else { Some(value.to_string()) },
                "keystatus" => props.keystatus = match value {
                    "available" => KeyStatus::Available,
                    "unavailable" => KeyStatus::Unavailable,
                    "none" => KeyStatus::None,
                    _ => KeyStatus::default(),
                },
                "pbkdf2iters" => props.pbkdf2_iterations = value.parse().ok(),
                _ => {}
            }
        }
        Ok(props)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyStatus {
    #[default]
    Unknown,
    Available,
    Unavailable,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct EncryptionProps {
    pub algorithm: Option<String>,
    pub keyformat: Option<String>,
    pub keylocation: Option<String>,
    pub keystatus: KeyStatus,
    pub pbkdf2_iterations: Option<u64>,
}
