use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::{DeviceInfo, PoolInfo};

/// Messages sent from CLI → Daemon
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Ping the daemon for liveness
    Ping,
    /// Get daemon status and recent events
    Status,
    /// List all known/imported pools
    ListPools,
    /// List all connected USB storage devices
    ListDevices,
    /// Manually trigger pool import
    ImportPool {
        pool_name: String,
        device_path: Option<PathBuf>,
    },
    /// Manually trigger pool export
    ExportPool {
        pool_name: String,
        force: bool,
    },
    /// Mount a dataset (with optional passphrase)
    Mount {
        dataset: String,
        passphrase: Option<String>,
    },
    /// Unmount a dataset and optionally unload key
    Unmount {
        dataset: String,
        unload_key: bool,
    },
    /// Create a new encrypted ZFS pool on a device
    InitPool {
        pool_name: String,
        device_path: PathBuf,
        passphrase: String,
        options: PoolCreationOptions,
    },
    /// Change passphrase for an encrypted dataset
    ChangeKey {
        dataset: String,
        old_passphrase: String,
        new_passphrase: String,
    },
    /// Subscribe to real-time events (daemon will push EventNotifications)
    SubscribeEvents,
    /// Reload configuration
    ReloadConfig,
}

/// Options for pool creation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolCreationOptions {
    pub encryption: bool,
    pub algorithm: String,
    pub compression: String,
    pub recordsize_kb: u64,
    pub disable_atime: bool,
    /// Enable only conservative feature flags for cross-platform compatibility
    pub cross_platform_safe: bool,
}

impl Default for PoolCreationOptions {
    fn default() -> Self {
        Self {
            encryption: true,
            algorithm: "aes-256-gcm".to_string(),
            compression: "zstd".to_string(),
            recordsize_kb: 128,
            disable_atime: true,
            cross_platform_safe: true,
        }
    }
}

/// Messages sent from Daemon → CLI
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DaemonResponse {
    Pong,
    Status {
        version: String,
        uptime_sec: u64,
        pools_imported: Vec<PoolInfo>,
        devices_connected: Vec<DeviceInfo>,
        auto_detect_enabled: bool,
    },
    PoolList(Vec<PoolInfo>),
    DeviceList(Vec<DeviceInfo>),
    Success { message: String },
    Error { code: String, message: String },
    EventStream,
}

/// Asynchronous events pushed from daemon to subscribed clients
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventNotification {
    DeviceInserted {
        device: DeviceInfo,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    DeviceRemoved {
        stable_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PoolImported {
        pool: PoolInfo,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PoolExported {
        pool_name: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PoolMounted {
        dataset: String,
        mountpoint: PathBuf,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PoolUnmounted {
        dataset: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PassphraseRequired {
        pool_name: String,
        device_path: PathBuf,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Error {
        context: String,
        message: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}
