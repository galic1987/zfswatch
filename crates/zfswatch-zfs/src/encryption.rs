use std::process::Stdio;

use tokio::process::Command;
use tracing::info;

use zfswatch_core::{Error, Result};

/// Manages ZFS encryption key operations
#[derive(Debug, Clone)]
pub struct EncryptionManager;

impl EncryptionManager {
    pub fn new() -> Self {
        Self
    }

    /// Load encryption key for a dataset from passphrase
    pub async fn load_key(&self, dataset: &str, passphrase: &str) -> Result<()> {
        info!("Loading encryption key for: {dataset}");

        let mut cmd = Command::new("zfs");
        cmd.args(["load-key", dataset]);
        cmd.stdin(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Zfs(format!("Failed to spawn zfs load-key: {e}")))?;

        if let Some(stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            stdin
                .write_all(format!("{passphrase}\n").as_bytes())
                .await
                .map_err(|e| Error::Zfs(format!("Failed to send passphrase: {e}")))?;
            stdin.shutdown().await.ok();
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| Error::Zfs(format!("zfs load-key failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("Key load failed: {stderr}")));
        }

        info!("Encryption key loaded for: {dataset}");
        Ok(())
    }

    /// Unload encryption key for a dataset
    pub async fn unload_key(&self, dataset: &str) -> Result<()> {
        info!("Unloading encryption key for: {dataset}");
        let output = Command::new("zfs")
            .args(["unload-key", dataset])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to unload key: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("Key unload failed: {stderr}")));
        }

        info!("Encryption key unloaded for: {dataset}");
        Ok(())
    }

    /// Change passphrase for an encrypted dataset
    pub async fn change_key(
        &self,
        dataset: &str,
        old_passphrase: &str,
        new_passphrase: &str,
    ) -> Result<()> {
        info!("Changing encryption key for: {dataset}");

        // First verify old passphrase by loading it
        self.load_key(dataset, old_passphrase).await?;

        let mut cmd = Command::new("zfs");
        cmd.args([
            "change-key",
            "-o",
            "keyformat=passphrase",
            "-o",
            "keylocation=prompt",
            dataset,
        ]);
        cmd.stdin(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Zfs(format!("Failed to spawn zfs change-key: {e}")))?;

        if let Some(stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            stdin
                .write_all(format!("{new_passphrase}\n").as_bytes())
                .await
                .map_err(|e| Error::Zfs(format!("Failed to send new passphrase: {e}")))?;
            stdin.shutdown().await.ok();
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| Error::Zfs(format!("zfs change-key failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("Key change failed: {stderr}")));
        }

        info!("Encryption key changed for: {dataset}");
        Ok(())
    }

    /// Check if a dataset's key is currently loaded
    pub async fn key_status(&self, dataset: &str) -> Result<KeyStatus> {
        let output = Command::new("zfs")
            .args(["get", "-H", "-o", "value", "keystatus", dataset])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to get keystatus: {e}")))?;

        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match value.as_str() {
            "available" => Ok(KeyStatus::Available),
            "unavailable" => Ok(KeyStatus::Unavailable),
            "none" => Ok(KeyStatus::None),
            _ => Ok(KeyStatus::Unknown),
        }
    }

    /// Get encryption properties for a dataset
    pub async fn get_encryption_props(&self, dataset: &str) -> Result<EncryptionProps> {
        let output = Command::new("zfs")
            .args([
                "get",
                "-H",
                "-o",
                "property,value",
                "encryption,keyformat,keylocation,keystatus,pbkdf2iters",
                dataset,
            ])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to get encryption props: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut props = EncryptionProps::default();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if parts.len() != 2 {
                continue;
            }
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

impl Default for EncryptionManager {
    fn default() -> Self {
        Self::new()
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
