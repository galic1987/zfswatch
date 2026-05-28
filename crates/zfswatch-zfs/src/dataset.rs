use std::path::PathBuf;

use tracing::info;

use zfswatch_core::{Error, Result};

use crate::command::CommandRunner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;

    #[tokio::test]
    async fn test_mount_success() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["mount", "tank"], "", "", true);
        runner.add_response("zfs", &["get", "-H", "-o", "value", "mountpoint", "tank"], "/Volumes/tank\n", "", true);

        let mgr = DatasetManager::new(runner);
        let mp = mgr.mount("tank").await.unwrap();
        assert_eq!(mp, PathBuf::from("/Volumes/tank"));
    }

    #[tokio::test]
    async fn test_mount_encrypted_error() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["mount", "tank"], "", "encryption key not loaded", false);

        let mgr = DatasetManager::new(runner);
        let err = mgr.mount("tank").await.unwrap_err().to_string();
        assert!(err.contains("encrypted"));
    }

    #[tokio::test]
    async fn test_mount_default_mountpoint() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["mount", "tank/data"], "", "", true);
        runner.add_response("zfs", &["get", "-H", "-o", "value", "mountpoint", "tank/data"], "none\n", "", true);

        let mgr = DatasetManager::new(runner);
        let mp = mgr.mount("tank/data").await.unwrap();
        assert_eq!(mp, PathBuf::from("/tank/data"));
    }

    #[tokio::test]
    async fn test_mount_with_keyload() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["mount", "-l", "tank"], "", "", true);
        runner.add_response("zfs", &["get", "-H", "-o", "value", "mountpoint", "tank"], "/mnt/tank\n", "", true);

        let mgr = DatasetManager::new(runner);
        let mp = mgr.mount_with_keyload("tank", "mypass").await.unwrap();
        assert_eq!(mp, PathBuf::from("/mnt/tank"));
    }

    #[tokio::test]
    async fn test_unmount() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["unmount", "tank"], "", "", true);

        let mgr = DatasetManager::new(runner);
        mgr.unmount("tank", false).await.unwrap();
    }

    #[tokio::test]
    async fn test_unmount_with_key_unload() {
        let runner = MockCommandRunner::new();
        runner.add_response("zfs", &["unmount", "-u", "tank"], "", "", true);

        let mgr = DatasetManager::new(runner);
        mgr.unmount("tank", true).await.unwrap();
    }
}

/// Manages ZFS dataset operations
pub struct DatasetManager<R: CommandRunner> {
    runner: R,
}

impl<R: CommandRunner> DatasetManager<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub async fn mount(&self, dataset: &str) -> Result<PathBuf> {
        info!("Mounting dataset: {dataset}");
        let (_, stderr, success) = self.runner.run("zfs", &["mount", dataset], None).await?;
        if !success {
            if stderr.contains("encryption key not loaded") {
                return Err(Error::Zfs(format!("Dataset {dataset} is encrypted — load key first")));
            }
            return Err(Error::Zfs(format!("zfs mount failed: {stderr}")));
        }

        let (stdout, _, _) = self.runner.run("zfs", &["get", "-H", "-o", "value", "mountpoint", dataset], None).await?;
        let mountpoint = stdout.trim().to_string();
        let mountpoint = if mountpoint == "none" || mountpoint == "-" {
            PathBuf::from(format!("/{dataset}"))
        } else {
            PathBuf::from(mountpoint)
        };

        info!("Mounted {dataset} at {mountpoint:?}");
        Ok(mountpoint)
    }

    pub async fn mount_with_keyload(&self, dataset: &str, passphrase: &str) -> Result<PathBuf> {
        info!("Mounting dataset with keyload: {dataset}");
        let (_, stderr, success) = self.runner.run("zfs", &["mount", "-l", dataset], Some(passphrase)).await?;
        if !success {
            return Err(Error::Zfs(format!("Mount with keyload failed: {stderr}")));
        }

        let (stdout, _, _) = self.runner.run("zfs", &["get", "-H", "-o", "value", "mountpoint", dataset], None).await?;
        let mountpoint = stdout.trim().to_string();
        let mountpoint = if mountpoint == "none" || mountpoint == "-" {
            PathBuf::from(format!("/{dataset}"))
        } else {
            PathBuf::from(mountpoint)
        };

        info!("Mounted {dataset} at {mountpoint:?}");
        Ok(mountpoint)
    }

    pub async fn unmount(&self, dataset: &str, unload_key: bool) -> Result<()> {
        info!("Unmounting dataset: {dataset} (unload_key={unload_key})");
        let mut args = vec!["unmount"];
        if unload_key { args.push("-u"); }
        args.push(dataset);

        let (_, stderr, success) = self.runner.run("zfs", &args, None).await?;
        if !success {
            return Err(Error::Zfs(format!("zfs unmount failed: {stderr}")));
        }
        info!("Unmounted dataset: {dataset}");
        Ok(())
    }
}
