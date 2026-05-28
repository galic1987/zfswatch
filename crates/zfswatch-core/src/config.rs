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

fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_scan_interval() -> u64 {
    0
}
fn default_encryption_algorithm() -> String {
    "aes-256-gcm".to_string()
}
fn default_keyformat() -> String {
    "passphrase".to_string()
}
fn default_keylocation() -> String {
    "prompt".to_string()
}
fn default_min_passphrase_length() -> usize {
    12
}
fn default_recordsize() -> u64 {
    128
}
fn default_compression() -> String {
    "zstd".to_string()
}
fn default_key_source() -> KeySource {
    KeySource::Prompt
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_format() -> LogFormat {
    LogFormat::Pretty
}

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
    /// Load configuration from a file path
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a file path
    pub fn to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the default config path for the platform
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
            PathBuf::from(".")
                .join(DEFAULT_CONFIG_NAME)
        }
    }

    /// Find config file: explicit path > default system path
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
