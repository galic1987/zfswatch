use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::Result;

/// Default configuration file name
pub const DEFAULT_CONFIG_NAME: &str = "zfswatch.toml";

/// Main configuration structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Encryption defaults
    #[serde(default)]
    pub encryption: EncryptionConfig,
    /// Performance tuning
    #[serde(default)]
    pub performance: PerformanceConfig,
    /// Known pools / per-pool overrides
    #[serde(default)]
    pub pools: Vec<PoolConfig>,
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Unix socket path for daemon ↔ CLI communication
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
    /// Enable automatic pool detection on USB insertion
    #[serde(default = "default_true")]
    pub auto_detect: bool,
    /// Enable automatic mounting (only for unencrypted pools unless key cached)
    #[serde(default = "default_false")]
    pub auto_mount: bool,
    /// Require user confirmation before importing unknown pools
    #[serde(default = "default_true")]
    pub confirm_unknown_pools: bool,
    /// Polling interval in seconds for manual device scans (0 = disabled)
    #[serde(default = "default_scan_interval")]
    pub scan_interval_sec: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Default encryption algorithm
    #[serde(default = "default_encryption_algorithm")]
    pub default_algorithm: String,
    /// Default key format (passphrase, raw, hex)
    #[serde(default = "default_keyformat")]
    pub default_keyformat: String,
    /// Default key location (prompt, file://..., https://...)
    #[serde(default = "default_keylocation")]
    pub default_keylocation: String,
    /// Minimum passphrase length
    #[serde(default = "default_min_passphrase_length")]
    pub min_passphrase_length: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Limit ZFS ARC maximum size in MB (0 = use ZFS default)
    #[serde(default)]
    pub arc_max_mb: u64,
    /// Default recordsize for new pools in KB (128 = 128K)
    #[serde(default = "default_recordsize")]
    pub recordsize_kb: u64,
    /// Disable atime on new pools
    #[serde(default = "default_true")]
    pub disable_atime: bool,
    /// Default compression algorithm
    #[serde(default = "default_compression")]
    pub compression: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Pool name
    pub name: String,
    /// Expected device stable identifier (for matching on insert)
    pub device_uuid: Option<String>,
    /// Auto-import this pool when device is connected
    #[serde(default = "default_true")]
    pub auto_import: bool,
    /// Auto-mount this pool when imported
    #[serde(default = "default_false")]
    pub auto_mount: bool,
    /// Key source for this pool
    #[serde(default = "default_key_source")]
    pub key_source: KeySource,
    /// Path to key file (if key_source is File)
    pub key_file: Option<PathBuf>,
    /// URL for remote key (if key_source is Https)
    pub key_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Prompt,
    Keyring,
    File,
    Https,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: LogFormat,
    /// Optional log file path (default: stderr/syslog)
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
    Compact,
}

// Default functions for serde
fn default_socket_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/var/run/zfswatch.sock")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/run/zfswatch/zfswatch.sock")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        PathBuf::from("/tmp/zfswatch.sock")
    }
}

fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_scan_interval() -> u64 { 0 }
fn default_encryption_algorithm() -> String { "aes-256-gcm".to_string() }
fn default_keyformat() -> String { "passphrase".to_string() }
fn default_keylocation() -> String { "prompt".to_string() }
fn default_min_passphrase_length() -> usize { 12 }
fn default_recordsize() -> u64 { 128 }
fn default_compression() -> String { "zstd".to_string() }
fn default_key_source() -> KeySource { KeySource::Prompt }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> LogFormat { LogFormat::Pretty }

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            auto_detect: true,
            auto_mount: false,
            confirm_unknown_pools: true,
            scan_interval_sec: 0,
        }
    }
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            default_algorithm: default_encryption_algorithm(),
            default_keyformat: default_keyformat(),
            default_keylocation: default_keylocation(),
            min_passphrase_length: default_min_passphrase_length(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            arc_max_mb: 0,
            recordsize_kb: default_recordsize(),
            disable_atime: true,
            compression: default_compression(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            encryption: EncryptionConfig::default(),
            performance: PerformanceConfig::default(),
            pools: Vec::new(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Config {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from("/usr/local/etc/zfswatch").join(DEFAULT_CONFIG_NAME)
        }
        #[cfg(target_os = "linux")]
        {
            PathBuf::from("/etc/zfswatch").join(DEFAULT_CONFIG_NAME)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            PathBuf::from(".").join(DEFAULT_CONFIG_NAME)
        }
    }

    pub fn resolve(path: Option<&PathBuf>) -> Result<Self> {
        if let Some(p) = path {
            return Self::from_file(p);
        }
        let default = Self::default_path();
        if default.exists() {
            return Self::from_file(&default);
        }
        Ok(Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.daemon.auto_detect, true);
        assert_eq!(config.daemon.auto_mount, false);
        assert_eq!(config.encryption.default_algorithm, "aes-256-gcm");
        assert_eq!(config.performance.recordsize_kb, 128);
        assert_eq!(config.performance.compression, "zstd");
        assert!(config.pools.is_empty());
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
[daemon]
auto_detect = false
auto_mount = true

[encryption]
default_algorithm = "aes-128-gcm"
min_passphrase_length = 8

[[pools]]
name = "testpool"
device_uuid = "usb-test-123"
auto_import = true
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.daemon.auto_detect, false);
        assert_eq!(config.daemon.auto_mount, true);
        assert_eq!(config.encryption.default_algorithm, "aes-128-gcm");
        assert_eq!(config.encryption.min_passphrase_length, 8);
    }

    #[test]
    fn test_config_roundtrip() {
        let mut config = Config::default();
        config.pools.push(PoolConfig {
            name: "mypool".to_string(),
            device_uuid: Some("uuid-123".to_string()),
            auto_import: true,
            auto_mount: false,
            key_source: KeySource::File,
            key_file: Some(PathBuf::from("/keys/mypool.key")),
            key_url: None,
        });

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        config.to_file(&tmpfile.path().to_path_buf()).unwrap();

        let loaded = Config::from_file(&tmpfile.path().to_path_buf()).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn test_key_source_serialization() {
        // KeySource serializes to lowercase strings via serde_json
        assert_eq!(serde_json::to_string(&KeySource::Prompt).unwrap(), "\"prompt\"");
        assert_eq!(serde_json::to_string(&KeySource::Keyring).unwrap(), "\"keyring\"");
        assert_eq!(serde_json::to_string(&KeySource::File).unwrap(), "\"file\"");
        assert_eq!(serde_json::to_string(&KeySource::Https).unwrap(), "\"https\"");
    }

    #[test]
    fn test_log_format_serialization() {
        assert_eq!(serde_json::to_string(&LogFormat::Pretty).unwrap(), "\"pretty\"");
        assert_eq!(serde_json::to_string(&LogFormat::Json).unwrap(), "\"json\"");
        assert_eq!(serde_json::to_string(&LogFormat::Compact).unwrap(), "\"compact\"");
    }

    #[test]
    fn test_config_invalid_toml() {
        let result: Result<Config> = toml::from_str("[invalid").map_err(|e| e.into());
        assert!(result.is_err());
    }

    #[test]
    fn test_daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert_eq!(config.confirm_unknown_pools, true);
        assert_eq!(config.scan_interval_sec, 0);
    }
}
