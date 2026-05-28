use std::path::PathBuf;

use tracing::{info, warn};

use zfswatch_core::{
    protocol::PoolCreationOptions,
    types::{DatasetInfo, PoolHealth, PoolInfo},
    Error, Result,
};

use crate::command::CommandRunner;

/// Conservative feature flags for cross-platform macOS ↔ Linux compatibility.
const CROSS_PLATFORM_SAFE_FEATURES: &[&str] = &[
    "allocation_classes", "async_destroy", "bookmarks", "bookmark_v2",
    "device_rebuild", "edonr", "embedded_data", "empty_bpobj",
    "encryption", "extensible_dataset", "filesystem_limits", "hole_birth",
    "large_blocks", "large_dnode", "livelist", "log_spacemap", "lz4_compress",
    "multi_vdev_crash_dump", "obsolete_counts", "project_quota",
    "redaction_bookmarks", "redacted_datasets", "resilver_defer",
    "sha512", "skein", "spacemap_histogram", "spacemap_v2",
    "userobj_accounting", "zstd_compress",
];

const CROSS_PLATFORM_RISKY_FEATURES: &[&str] = &[
    "block_cloning_endian", "fast_dedup", "raidz_expansion", "longname",
];

/// Manages ZFS pool operations
pub struct PoolManager<R: CommandRunner> {
    runner: R,
}

impl<R: CommandRunner> PoolManager<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub async fn check_zfs_installed(&self) -> Result<bool> {
        match self.runner.run("zpool", &["--version"], None).await {
            Ok((stdout, _, true)) => {
                info!("ZFS installed: {}", stdout.trim());
                Ok(true)
            }
            Ok((_, _, false)) => {
                warn!("zpool command found but returned error");
                Ok(false)
            }
            Err(_) => {
                warn!("zpool command not found");
                Ok(false)
            }
        }
    }

    pub async fn zfs_version(&self) -> Result<String> {
        let (stdout, _, success) = self.runner.run("zpool", &["--version"], None).await?;
        if !success {
            return Err(Error::Zfs("zpool --version failed".to_string()));
        }
        Ok(stdout.trim().to_string())
    }

    pub async fn list_pools(&self) -> Result<Vec<PoolInfo>> {
        self.list_imported_pools().await
    }

    async fn list_imported_pools(&self) -> Result<Vec<PoolInfo>> {
        let (stdout, stderr, success) = self
            .runner
            .run("zpool", &["list", "-H", "-o", "name,size,allocated,free,health,guid"], None)
            .await?;

        if !success {
            return Err(Error::Zfs(format!("zpool list failed: {stderr}")));
        }

        let mut pools = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 6 { continue; }

            let name = parts[0].to_string();
            let size = parse_zfs_size(parts[1]);
            let allocated = parse_zfs_size(parts[2]);
            let free = parse_zfs_size(parts[3]);
            let health = parse_health(parts[4]);
            let guid = parts[5].to_string();

            let encrypted = self.is_encrypted(&name).await.unwrap_or(false);
            let (mounted, mountpoint) = self.get_mount_info(&name).await.unwrap_or((false, None));
            let datasets = self.list_datasets(&name).await.unwrap_or_default();

            pools.push(PoolInfo {
                name, guid, health,
                size_bytes: size,
                allocated_bytes: allocated,
                free_bytes: free,
                encrypted, mounted, mountpoint,
                datasets,
                features: Vec::new(),
                version: String::new(),
            });
        }
        Ok(pools)
    }

    pub async fn find_importable_pools(&self, device_path: Option<&PathBuf>) -> Result<Vec<String>> {
        let mut args = vec!["import", "-N"];
        let device_str;
        if let Some(dev) = device_path {
            device_str = dev.to_string_lossy().to_string();
            args.push("-d");
            args.push(&device_str);
        }

        let (stdout, _, _) = self.runner.run("zpool", &args, None).await?;

        let mut pools = Vec::new();
        for line in stdout.lines() {
            if let Some(stripped) = line.trim().strip_prefix("pool: ") {
                pools.push(stripped.to_string());
            }
        }
        Ok(pools)
    }

    pub async fn import_pool(&self, name: &str, device_path: Option<&PathBuf>) -> Result<()> {
        info!("Importing pool: {name}");
        let mut args = vec!["import"];
        let device_str;
        if let Some(dev) = device_path {
            device_str = dev.to_string_lossy().to_string();
            args.push("-d");
            args.push(&device_str);
        }
        args.push(name);

        let (_, stderr, success) = self.runner.run("zpool", &args, None).await?;
        if !success {
            return Err(Error::Zfs(format!("zpool import failed: {stderr}")));
        }
        info!("Successfully imported pool: {name}");
        Ok(())
    }

    pub async fn export_pool(&self, name: &str, force: bool) -> Result<()> {
        info!("Exporting pool: {name} (force={force})");
        let mut args = vec!["export"];
        if force { args.push("-f"); }
        args.push(name);

        let (_, stderr, success) = self.runner.run("zpool", &args, None).await?;
        if !success {
            return Err(Error::Zfs(format!("zpool export failed: {stderr}")));
        }
        info!("Successfully exported pool: {name}");
        Ok(())
    }

    pub async fn create_pool(
        &self,
        name: &str,
        device: &PathBuf,
        passphrase: &str,
        options: &PoolCreationOptions,
    ) -> Result<()> {
        info!("Creating encrypted pool '{name}' on {device:?}");

        let mut args: Vec<String> = vec!["create".into(), "-f".into()];

        if options.disable_atime {
            args.push("-O".into()); args.push("atime=off".into());
        }
        args.push("-O".into()); args.push(format!("compression={}", options.compression));
        args.push("-O".into()); args.push(format!("recordsize={}K", options.recordsize_kb));

        if options.encryption {
            args.push("-O".into()); args.push("encryption=on".into());
            args.push("-O".into()); args.push(format!("encryption={}", options.algorithm));
            args.push("-O".into()); args.push("keyformat=passphrase".into());
            args.push("-O".into()); args.push("keylocation=prompt".into());
        }

        if options.cross_platform_safe {
            for feature in CROSS_PLATFORM_SAFE_FEATURES {
                args.push("-o".into()); args.push(format!("feature@{feature}=enabled"));
            }
            for feature in CROSS_PLATFORM_RISKY_FEATURES {
                args.push("-o".into()); args.push(format!("feature@{feature}=disabled"));
            }
        }

        args.push(name.into());
        args.push(device.to_string_lossy().to_string());

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let (_, stderr, success) = self.runner.run("zpool", &arg_refs, Some(passphrase)).await?;
        if !success {
            return Err(Error::Zfs(format!("Pool creation failed: {stderr}")));
        }
        info!("Successfully created pool: {name}");
        Ok(())
    }

    async fn is_encrypted(&self, name: &str) -> Result<bool> {
        let (stdout, _, success) = self
            .runner
            .run("zfs", &["get", "-H", "-o", "value", "encryption", name], None)
            .await?;
        if !success { return Ok(false); }
        let value = stdout.trim();
        Ok(value != "off" && !value.is_empty() && value != "-" && !value.starts_with("cannot"))
    }

    async fn get_mount_info(&self, name: &str) -> Result<(bool, Option<PathBuf>)> {
        let (stdout, _, success) = self
            .runner
            .run("zfs", &["get", "-H", "-o", "value", "mounted,mountpoint", name], None)
            .await?;
        if !success { return Ok((false, None)); }

        let lines: Vec<&str> = stdout.lines().collect();
        let mounted = lines.get(0).map(|s| s.trim() == "yes").unwrap_or(false);
        let mountpoint = lines.get(1).and_then(|s| {
            let s = s.trim();
            if s == "none" || s == "-" || s.is_empty() { None } else { Some(PathBuf::from(s)) }
        });
        Ok((mounted, mountpoint))
    }

    async fn list_datasets(&self, pool_name: &str) -> Result<Vec<DatasetInfo>> {
        let (stdout, _, success) = self.runner.run(
            "zfs",
            &["list", "-H", "-r", "-o", "name,mountpoint,mounted,encryption,compression,recordsize,quota", pool_name],
            None,
        ).await?;
        if !success { return Ok(Vec::new()); }

        let mut datasets = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 7 { continue; }

            datasets.push(DatasetInfo {
                name: parts[0].to_string(),
                mountpoint: if parts[1] == "none" || parts[1] == "-" { None } else { Some(PathBuf::from(parts[1])) },
                mounted: parts[2] == "yes",
                encrypted: parts[3] != "off" && parts[3] != "-",
                compression: if parts[4] == "off" || parts[4] == "-" { None } else { Some(parts[4].to_string()) },
                recordsize: Some(parse_zfs_size(parts[5])),
                quota: if parts[6] == "none" || parts[6] == "-" || parts[6] == "0" { None } else { Some(parse_zfs_size(parts[6])) },
            });
        }
        Ok(datasets)
    }
}

/// Parse ZFS size strings like "1.5T", "500G", "128K"
pub fn parse_zfs_size(s: &str) -> u64 {
    let s = s.trim();
    if s == "-" || s.is_empty() { return 0; }

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
    let value = numeric.parse::<f64>().unwrap_or(0.0);
    (value * multiplier as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;

    #[test]
    fn test_parse_zfs_size() {
        // 1.5T = 1.5 * 1024^4 = 1,649,267,441,664
        assert_eq!(parse_zfs_size("1.5T"), 1_649_267_441_664);
        assert_eq!(parse_zfs_size("500G"), 536_870_912_000);
        assert_eq!(parse_zfs_size("128K"), 131_072);
        assert_eq!(parse_zfs_size("8M"), 8_388_608);
        assert_eq!(parse_zfs_size("-"), 0);
        assert_eq!(parse_zfs_size(""), 0);
        assert_eq!(parse_zfs_size("0"), 0);
    }

    #[test]
    fn test_parse_health() {
        assert!(matches!(parse_health("ONLINE"), PoolHealth::Online));
        assert!(matches!(parse_health("DEGRADED"), PoolHealth::Degraded));
        assert!(matches!(parse_health("FAULTED"), PoolHealth::Faulted));
        assert!(matches!(parse_health("OFFLINE"), PoolHealth::Offline));
        assert!(matches!(parse_health("UNAVAIL"), PoolHealth::Unavailable));
        assert!(matches!(parse_health("REMOVED"), PoolHealth::Removed));
        assert!(matches!(parse_health("UNKNOWN"), PoolHealth::Unknown));
        assert!(matches!(parse_health("online"), PoolHealth::Online));
    }

    #[tokio::test]
    async fn test_check_zfs_installed() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["--version"], "zfs-2.3.1", "", true);
        let mgr = PoolManager::new(runner);
        assert!(mgr.check_zfs_installed().await.unwrap());
    }

    #[tokio::test]
    async fn test_check_zfs_not_installed() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["--version"], "", "command not found", false);
        let mgr = PoolManager::new(runner);
        assert!(!mgr.check_zfs_installed().await.unwrap());
    }

    #[tokio::test]
    async fn test_zfs_version() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["--version"], "zfs-2.3.1", "", true);
        let mgr = PoolManager::new(runner);
        assert_eq!(mgr.zfs_version().await.unwrap(), "zfs-2.3.1");
    }

    #[tokio::test]
    async fn test_list_pools() {
        let runner = MockCommandRunner::new();
        let stdout = "tank\t1.5T\t500G\t1T\tONLINE\t12345\n";
        runner.add_response("zpool", &["list", "-H", "-o", "name,size,allocated,free,health,guid"], stdout, "", true);
        runner.add_response("zfs", &["get", "-H", "-o", "value", "encryption", "tank"], "off\n", "", true);
        runner.add_response("zfs", &["get", "-H", "-o", "value", "mounted,mountpoint", "tank"], "yes\n/Volumes/tank\n", "", true);
        runner.add_response("zfs", &["list", "-H", "-r", "-o", "name,mountpoint,mounted,encryption,compression,recordsize,quota", "tank"], "", "", true);

        let mgr = PoolManager::new(runner);
        let pools = mgr.list_pools().await.unwrap();
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].name, "tank");
        assert_eq!(pools[0].health, PoolHealth::Online);
        assert_eq!(pools[0].mounted, true);
    }

    #[tokio::test]
    async fn test_import_pool() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["import", "tank"], "", "", true);
        let mgr = PoolManager::new(runner.clone());
        mgr.import_pool("tank", None).await.unwrap();

        let calls = runner.get_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "zpool");
        assert_eq!(calls[0].1, vec!["import", "tank"]);
    }

    #[tokio::test]
    async fn test_import_pool_with_device() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["import", "-d", "/dev/sdb", "tank"], "", "", true);
        let mgr = PoolManager::new(runner);
        mgr.import_pool("tank", Some(&PathBuf::from("/dev/sdb"))).await.unwrap();
    }

    #[tokio::test]
    async fn test_export_pool() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["export", "tank"], "", "", true);
        let mgr = PoolManager::new(runner);
        mgr.export_pool("tank", false).await.unwrap();
    }

    #[tokio::test]
    async fn test_export_pool_force() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["export", "-f", "tank"], "", "", true);
        let mgr = PoolManager::new(runner);
        mgr.export_pool("tank", true).await.unwrap();
    }

    #[tokio::test]
    async fn test_create_pool() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["create", "-f", "-O", "atime=off", "-O", "compression=zstd", "-O", "recordsize=128K", "-O", "encryption=on", "-O", "encryption=aes-256-gcm", "-O", "keyformat=passphrase", "-O", "keylocation=prompt", "-o", "feature@allocation_classes=enabled", "-o", "feature@async_destroy=enabled", "-o", "feature@bookmarks=enabled", "-o", "feature@encryption=enabled", "-o", "feature@extensible_dataset=enabled", "-o", "feature@large_blocks=enabled", "-o", "feature@lz4_compress=enabled", "-o", "feature@spacemap_v2=enabled", "-o", "feature@zstd_compress=enabled", "-o", "feature@block_cloning_endian=disabled", "-o", "feature@fast_dedup=disabled", "-o", "feature@raidz_expansion=disabled", "-o", "feature@longname=disabled", "testpool", "/dev/sdb"], "", "", true);

        let mgr = PoolManager::new(runner);
        let options = PoolCreationOptions::default();
        mgr.create_pool("testpool", &PathBuf::from("/dev/sdb"), "mypass", &options).await.unwrap();
    }

    #[tokio::test]
    async fn test_find_importable_pools() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["import", "-N"], "pool: tank\n     id: 123\n", "", true);
        let mgr = PoolManager::new(runner);
        let pools = mgr.find_importable_pools(None).await.unwrap();
        assert_eq!(pools, vec!["tank"]);
    }

    #[tokio::test]
    async fn test_create_pool_no_cross_platform() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["create", "-f", "-O", "atime=off", "-O", "compression=zstd", "-O", "recordsize=128K", "-O", "encryption=on", "-O", "encryption=aes-256-gcm", "-O", "keyformat=passphrase", "-O", "keylocation=prompt", "testpool2", "/dev/sdc"], "", "", true);

        let mgr = PoolManager::new(runner);
        let mut options = PoolCreationOptions::default();
        options.cross_platform_safe = false;
        mgr.create_pool("testpool2", &PathBuf::from("/dev/sdc"), "mypass", &options).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_imported_pools_empty() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["list", "-H", "-o", "name,size,allocated,free,health,guid"], "", "", true);
        let mgr = PoolManager::new(runner);
        let pools = mgr.list_pools().await.unwrap();
        assert!(pools.is_empty());
    }

    #[tokio::test]
    async fn test_list_imported_pools_malformed_line() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["list", "-H", "-o", "name,size,allocated,free,health,guid"], "short_line\n", "", true);
        let mgr = PoolManager::new(runner);
        let pools = mgr.list_pools().await.unwrap();
        assert!(pools.is_empty());
    }

    #[tokio::test]
    async fn test_create_pool_no_encryption() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["create", "-f", "-O", "atime=off", "-O", "compression=zstd", "-O", "recordsize=128K", "-o", "feature@allocation_classes=enabled", "-o", "feature@async_destroy=enabled", "-o", "feature@bookmarks=enabled", "-o", "feature@encryption=enabled", "-o", "feature@extensible_dataset=enabled", "-o", "feature@large_blocks=enabled", "-o", "feature@lz4_compress=enabled", "-o", "feature@spacemap_v2=enabled", "-o", "feature@zstd_compress=enabled", "-o", "feature@block_cloning_endian=disabled", "-o", "feature@fast_dedup=disabled", "-o", "feature@raidz_expansion=disabled", "-o", "feature@longname=disabled", "plainpool", "/dev/sdd"], "", "", true);

        let mgr = PoolManager::new(runner);
        let mut options = PoolCreationOptions::default();
        options.encryption = false;
        mgr.create_pool("plainpool", &PathBuf::from("/dev/sdd"), "", &options).await.unwrap();
    }
}

pub fn parse_health(s: &str) -> PoolHealth {
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
