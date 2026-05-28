use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
use tracing::info;

use zfswatch_core::{Error, Result};

/// Manages ZFS dataset operations
#[derive(Debug, Clone)]
pub struct DatasetManager;

impl DatasetManager {
    pub fn new() -> Self {
        Self
    }

    /// Mount a dataset. For encrypted datasets, the key must be loaded first.
    pub async fn mount(&self, dataset: &str) -> Result<PathBuf> {
        info!("Mounting dataset: {dataset}");
        let output = Command::new("zfs")
            .args(["mount", dataset])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to mount {dataset}: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if it's an encryption error
            if stderr.contains("encryption key not loaded") {
                return Err(Error::Zfs(format!(
                    "Dataset {dataset} is encrypted — load key first"
                )));
            }
            return Err(Error::Zfs(format!("zfs mount failed: {stderr}")));
        }

        // Get mountpoint
        let mp_output = Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", dataset])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to get mountpoint: {e}")))?;

        let mountpoint = String::from_utf8_lossy(&mp_output.stdout)
            .trim()
            .to_string();
        let mountpoint = if mountpoint == "none" || mountpoint == "-" {
            PathBuf::from(format!("/{dataset}"))
        } else {
            PathBuf::from(mountpoint)
        };

        info!("Mounted {dataset} at {mountpoint:?}");
        Ok(mountpoint)
    }

    /// Mount with automatic key loading (-l flag)
    pub async fn mount_with_keyload(&self, dataset: &str, passphrase: &str) -> Result<PathBuf> {
        info!("Mounting dataset with keyload: {dataset}");

        let mut cmd = Command::new("zfs");
        cmd.args(["mount", "-l", dataset]);
        cmd.stdin(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Zfs(format!("Failed to spawn zfs mount -l: {e}")))?;

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
            .map_err(|e| Error::Zfs(format!("zfs mount -l failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("Mount with keyload failed: {stderr}")));
        }

        // Get mountpoint
        let mp_output = Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", dataset])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to get mountpoint: {e}")))?;

        let mountpoint = String::from_utf8_lossy(&mp_output.stdout)
            .trim()
            .to_string();
        let mountpoint = if mountpoint == "none" || mountpoint == "-" {
            PathBuf::from(format!("/{dataset}"))
        } else {
            PathBuf::from(mountpoint)
        };

        info!("Mounted {dataset} at {mountpoint:?}");
        Ok(mountpoint)
    }

    /// Unmount a dataset. Optionally unload the encryption key.
    pub async fn unmount(&self, dataset: &str, unload_key: bool) -> Result<()> {
        info!("Unmounting dataset: {dataset} (unload_key={unload_key})");
        let mut cmd = Command::new("zfs");
        cmd.arg("unmount");
        if unload_key {
            cmd.arg("-u");
        }
        cmd.arg(dataset);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to unmount {dataset}: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("zfs unmount failed: {stderr}")));
        }

        info!("Unmounted dataset: {dataset}");
        Ok(())
    }
}

impl Default for DatasetManager {
    fn default() -> Self {
        Self::new()
    }
}
