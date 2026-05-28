use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
use tracing::{info, warn};

use zfswatch_core::{
    protocol::PoolCreationOptions,
    types::{DatasetInfo, PoolHealth, PoolInfo},
    Error, Result,
};

/// Conservative feature flags for cross-platform macOS ↔ Linux compatibility.
/// These are supported by OpenZFS 2.3.x on both platforms.
const CROSS_PLATFORM_SAFE_FEATURES: &[&str] = &[
    "allocation_classes",
    "async_destroy",
    "bookmarks",
    "bookmark_v2",
    "device_rebuild",
    "edonr",
    "embedded_data",
    "empty_bpobj",
    "encryption",
    "extensible_dataset",
    "filesystem_limits",
    "hole_birth",
    "large_blocks",
    "large_dnode",
    "livelist",
    "log_spacemap",
    "lz4_compress",
    "multi_vdev_crash_dump",
    "obsolete_counts",
    "project_quota",
    "redaction_bookmarks",
    "redacted_datasets",
    "resilver_defer",
    "sha512",
    "skein",
    "spacemap_histogram",
    "spacemap_v2",
    "userobj_accounting",
    "zstd_compress",
];

/// Feature flags to explicitly DISABLE for cross-platform safety.
/// These may cause issues between macOS and Linux or different ZFS versions.
const CROSS_PLATFORM_RISKY_FEATURES: &[&str] = &[
    // Block cloning endian has had macOS-specific issues
    "block_cloning_endian",
    // Fast dedup is newer and may not be on all platforms
    "fast_dedup",
    // RAIDZ expansion is relatively new
    "raidz_expansion",
    // Long names (1023 char) is 2.3+; disable for wider compatibility
    "longname",
];

/// Manages ZFS pool operations by wrapping zpool/zfs CLI commands.
#[derive(Debug, Clone)]
pub struct PoolManager;

impl PoolManager {
    pub fn new() -> Self {
        Self
    }

    /// Check if the zpool command is available
    pub async fn check_zfs_installed(&self) -> Result<bool> {
        match Command::new("zpool").arg("--version").output().await {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                info!("ZFS installed: {}", stdout.trim());
                Ok(true)
            }
            Ok(_) => {
                warn!("zpool command found but returned error — ZFS may not be loaded");
                Ok(false)
            }
            Err(e) => {
                warn!("zpool command not found: {e}");
                Ok(false)
            }
        }
    }

    /// Get ZFS version string
    pub async fn zfs_version(&self) -> Result<String> {
        let output = Command::new("zpool")
            .arg("--version")
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to run zpool --version: {e}")))?;
        if !output.status.success() {
            return Err(Error::Zfs("zpool --version failed".to_string()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// List all pools (imported and potentially importable)
    pub async fn list_pools(&self) -> Result<Vec<PoolInfo>> {
        // Get imported pools first
        let imported = self.list_imported_pools().await?;
        Ok(imported)
    }

    /// List currently imported pools with detailed info
    async fn list_imported_pools(&self) -> Result<Vec<PoolInfo>> {
        let output = Command::new("zpool")
            .args(["list", "-H", "-o", "name,size,allocated,free,health,guid"])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to list pools: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("zpool list failed: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut pools = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 6 {
                continue;
            }

            let name = parts[0].to_string();
            let size = Self::parse_zfs_size(parts[1]);
            let allocated = Self::parse_zfs_size(parts[2]);
            let free = Self::parse_zfs_size(parts[3]);
            let health = Self::parse_health(parts[4]);
            let guid = parts[5].to_string();

            // Get encryption and mount status via zfs get
            let encrypted = self.is_encrypted(&name).await.unwrap_or(false);
            let (mounted, mountpoint) = self.get_mount_info(&name).await.unwrap_or((false, None));

            // Get datasets
            let datasets = self.list_datasets(&name).await.unwrap_or_default();

            pools.push(PoolInfo {
                name,
                guid,
                health,
                size_bytes: size,
                allocated_bytes: allocated,
                free_bytes: free,
                encrypted,
                mounted,
                mountpoint,
                datasets,
                features: Vec::new(), // populated on demand
                version: String::new(),
            });
        }

        Ok(pools)
    }

    /// Find pools that can be imported (but are not currently imported)
    pub async fn find_importable_pools(&self, device_path: Option<&PathBuf>) -> Result<Vec<String>> {
        let mut cmd = Command::new("zpool");
        cmd.arg("import");
        if let Some(dev) = device_path {
            cmd.args(["-d", &dev.to_string_lossy()]);
        }
        cmd.arg("-N"); // don't actually import, just list

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to scan for importable pools: {e}")))?;

        // zpool import returns non-zero if no pools found, which is fine
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut pools = Vec::new();

        for line in stdout.lines() {
            // Parse output like: "   pool: tank"
            if let Some(stripped) = line.trim().strip_prefix("pool: ") {
                pools.push(stripped.to_string());
            }
        }

        Ok(pools)
    }

    /// Import a pool by name, optionally specifying the device path
    pub async fn import_pool(&self, name: &str, device_path: Option<&PathBuf>) -> Result<()> {
        info!("Importing pool: {name}");
        let mut cmd = Command::new("zpool");
        cmd.arg("import");
        if let Some(dev) = device_path {
            cmd.args(["-d", &dev.to_string_lossy()]);
        }
        cmd.arg(name);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to import pool {name}: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("zpool import failed: {stderr}")));
        }

        info!("Successfully imported pool: {name}");
        Ok(())
    }

    /// Export a pool (safe unmount)
    pub async fn export_pool(&self, name: &str, force: bool) -> Result<()> {
        info!("Exporting pool: {name} (force={force})");
        let mut cmd = Command::new("zpool");
        cmd.arg("export");
        if force {
            cmd.arg("-f");
        }
        cmd.arg(name);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to export pool {name}: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("zpool export failed: {stderr}")));
        }

        info!("Successfully exported pool: {name}");
        Ok(())
    }

    /// Create a new ZFS pool with encryption
    pub async fn create_pool(
        &self,
        name: &str,
        device: &PathBuf,
        passphrase: &str,
        options: &PoolCreationOptions,
    ) -> Result<()> {
        info!("Creating encrypted pool '{name}' on {device:?}");

        let mut cmd = Command::new("zpool");
        cmd.arg("create");
        cmd.arg("-f"); // force (in case disk has existing data)

        // Pool-wide properties
        if options.disable_atime {
            cmd.args(["-O", "atime=off"]);
        }
        cmd.args(["-O", &format!("compression={}", options.compression)]);
        cmd.args(["-O", &format!("recordsize={}K", options.recordsize_kb)]);

        if options.encryption {
            cmd.args(["-O", "encryption=on"]);
            cmd.args(["-O", &format!("encryption={}", options.algorithm)]);
            cmd.args(["-O", "keyformat=passphrase"]);
            cmd.args(["-O", "keylocation=prompt"]);
        }

        // Feature flags for cross-platform safety
        if options.cross_platform_safe {
            for feature in CROSS_PLATFORM_SAFE_FEATURES {
                cmd.args(["-o", &format!("feature@{feature}=enabled")]);
            }
            for feature in CROSS_PLATFORM_RISKY_FEATURES {
                cmd.args(["-o", &format!("feature@{feature}=disabled")]);
            }
        }

        cmd.arg(name);
        cmd.arg(device);

        // Provide passphrase via stdin
        cmd.stdin(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Zfs(format!("Failed to spawn zpool create: {e}")))?;

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
            .map_err(|e| Error::Zfs(format!("zpool create failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Zfs(format!("Pool creation failed: {stderr}")));
        }

        info!("Successfully created pool: {name}");
        Ok(())
    }

    /// Check if a pool or dataset is encrypted
    async fn is_encrypted(&self, name: &str) -> Result<bool> {
        let output = Command::new("zfs")
            .args(["get", "-H", "-o", "value", "encryption", name])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to check encryption: {e}")))?;

        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(value != "off" && !value.is_empty() && value != "-" && !value.starts_with("cannot"))
    }

    /// Get mount status and mountpoint
    async fn get_mount_info(&self, name: &str) -> Result<(bool, Option<PathBuf>)> {
        let output = Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mounted,mountpoint", name])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to get mount info: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();

        let mounted = lines.get(0).map(|s| s.trim() == "yes").unwrap_or(false);
        let mountpoint = lines.get(1).and_then(|s| {
            let s = s.trim();
            if s == "none" || s == "-" || s.is_empty() {
                None
            } else {
                Some(PathBuf::from(s))
            }
        });

        Ok((mounted, mountpoint))
    }

    /// List datasets within a pool
    async fn list_datasets(&self, pool_name: &str) -> Result<Vec<DatasetInfo>> {
        let output = Command::new("zfs")
            .args([
                "list",
                "-H",
                "-r",
                "-o",
                "name,mountpoint,mounted,encryption,compression,recordsize,quota",
                pool_name,
            ])
            .output()
            .await
            .map_err(|e| Error::Zfs(format!("Failed to list datasets: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut datasets = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 7 {
                continue;
            }

            let name = parts[0].to_string();
            let mountpoint = if parts[1] == "none" || parts[1] == "-" {
                None
            } else {
                Some(PathBuf::from(parts[1]))
            };
            let mounted = parts[2] == "yes";
            let encrypted = parts[3] != "off" && parts[3] != "-";
            let compression = if parts[4] == "off" || parts[4] == "-" {
                None
            } else {
                Some(parts[4].to_string())
            };
            let recordsize = Some(Self::parse_zfs_size(parts[5]));
            let quota = if parts[6] == "none" || parts[6] == "-" || parts[6] == "0" {
                None
            } else {
                Some(Self::parse_zfs_size(parts[6]))
            };

            datasets.push(DatasetInfo {
                name,
                mountpoint,
                mounted,
                encrypted,
                compression,
                recordsize,
                quota,
            });
        }

        Ok(datasets)
    }

    /// Parse ZFS size strings like "1.5T", "500G", "128K"
    fn parse_zfs_size(s: &str) -> u64 {
        let s = s.trim();
        if s == "-" || s.is_empty() {
            return 0;
        }

        let multiplier = if s.ends_with('T') || s.ends_with('t') {
            1024u64.pow(4)
        } else if s.ends_with('G') || s.ends_with('g') {
            1024u64.pow(3)
        } else if s.ends_with('M') || s.ends_with('m') {
            1024u64.pow(2)
        } else if s.ends_with('K') || s.ends_with('k') {
            1024
        } else if s.ends_with('P') || s.ends_with('p') {
            1024u64.pow(5)
        } else {
            1
        };

        let numeric: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        numeric.parse::<f64>().unwrap_or(0.0) as u64 * multiplier
    }

    fn parse_health(s: &str) -> PoolHealth {
        match s.trim().to_uppercase().as_str() {
            "ONLINE" => PoolHealth::Online,
            "DEGRADED" => PoolHealth::Degraded,
            "FAULTED" => PoolHealth::Faulted,
            "OFFLINE" => PoolHealth::Offline,
            "UNAVAIL" | "UNAVAILABLE" => PoolHealth::Unavailable,
            "REMOVED" => PoolHealth::Removed,
            _ => PoolHealth::Unknown,
        }
    }
}

impl Default for PoolManager {
    fn default() -> Self {
        Self::new()
    }
}
