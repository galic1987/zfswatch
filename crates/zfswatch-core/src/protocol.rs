use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::{DeviceInfo, PoolInfo};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_daemon_request_serialization() {
        let req = DaemonRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, "\"Ping\"");

        let req = DaemonRequest::ListPools;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, "\"ListPools\"");

        let req = DaemonRequest::ImportPool {
            pool_name: "tank".to_string(),
            device_path: Some(PathBuf::from("/dev/sdb")),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"ImportPool\""));
        assert!(json.contains("tank"));
    }

    #[test]
    fn test_daemon_response_serialization() {
        let resp = DaemonResponse::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "\"Pong\"");

        let resp = DaemonResponse::Success { message: "done".to_string() };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("done"));

        let resp = DaemonResponse::Error { code: "E1".to_string(), message: "fail".to_string() };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("E1"));
        assert!(json.contains("fail"));
    }

    #[test]
    fn test_pool_creation_options_default() {
        let opts = PoolCreationOptions::default();
        assert_eq!(opts.encryption, true);
        assert_eq!(opts.algorithm, "aes-256-gcm");
        assert_eq!(opts.compression, "zstd");
        assert_eq!(opts.recordsize_kb, 128);
        assert_eq!(opts.cross_platform_safe, true);
    }

    #[test]
    fn test_event_notification_serialization() {
        let event = EventNotification::PassphraseRequired {
            pool_name: "tank".to_string(),
            device_path: PathBuf::from("/dev/sdb"),
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("PassphraseRequired"));
        assert!(json.contains("tank"));
    }
}

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
