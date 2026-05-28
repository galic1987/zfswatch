use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// USB connection speed classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsbSpeed {
    Unknown,
    LowSpeed,      // 1.5 Mbps (USB 1.0)
    FullSpeed,     // 12 Mbps (USB 1.1)
    HighSpeed,     // 480 Mbps (USB 2.0)
    SuperSpeed,    // 5 Gbps (USB 3.0/3.1 Gen 1)
    SuperSpeed10,  // 10 Gbps (USB 3.1 Gen 2 / 3.2 Gen 2)
    SuperSpeed20,  // 20 Gbps (USB 3.2 Gen 2x2 / USB4 20G)
    Usb4_40G,      // 40 Gbps (USB4 / Thunderbolt 3/4)
    Usb4_80G,      // 80 Gbps (USB4 v2 / Thunderbolt 5)
}

impl UsbSpeed {
    /// Get the approximate maximum throughput in MB/s for ZFS planning
    pub fn max_throughput_mbps(&self) -> u64 {
        match self {
            UsbSpeed::Unknown => 0,
            UsbSpeed::LowSpeed => 0,
            UsbSpeed::FullSpeed => 1,
            UsbSpeed::HighSpeed => 35,
            UsbSpeed::SuperSpeed => 450,
            UsbSpeed::SuperSpeed10 => 900,
            UsbSpeed::SuperSpeed20 => 1800,
            UsbSpeed::Usb4_40G => 3500,
            UsbSpeed::Usb4_80G => 7000,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            UsbSpeed::Unknown => "Unknown",
            UsbSpeed::LowSpeed => "USB 1.0 Low-Speed (1.5 Mbps)",
            UsbSpeed::FullSpeed => "USB 1.1 Full-Speed (12 Mbps)",
            UsbSpeed::HighSpeed => "USB 2.0 High-Speed (480 Mbps)",
            UsbSpeed::SuperSpeed => "USB 3.0 SuperSpeed (5 Gbps)",
            UsbSpeed::SuperSpeed10 => "USB 3.1 Gen 2 (10 Gbps)",
            UsbSpeed::SuperSpeed20 => "USB 3.2 Gen 2x2 (20 Gbps)",
            UsbSpeed::Usb4_40G => "USB4 / Thunderbolt 3/4 (40 Gbps)",
            UsbSpeed::Usb4_80G => "USB4 v2 / Thunderbolt 5 (80 Gbps)",
        }
    }
}

impl fmt::Display for UsbSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usb_speed_throughput() {
        assert_eq!(UsbSpeed::Unknown.max_throughput_mbps(), 0);
        assert_eq!(UsbSpeed::LowSpeed.max_throughput_mbps(), 0);
        assert_eq!(UsbSpeed::FullSpeed.max_throughput_mbps(), 1);
        assert_eq!(UsbSpeed::HighSpeed.max_throughput_mbps(), 35);
        assert_eq!(UsbSpeed::SuperSpeed.max_throughput_mbps(), 450);
        assert_eq!(UsbSpeed::SuperSpeed10.max_throughput_mbps(), 900);
        assert_eq!(UsbSpeed::SuperSpeed20.max_throughput_mbps(), 1800);
        assert_eq!(UsbSpeed::Usb4_40G.max_throughput_mbps(), 3500);
        assert_eq!(UsbSpeed::Usb4_80G.max_throughput_mbps(), 7000);
    }

    #[test]
    fn test_usb_speed_display() {
        assert!(UsbSpeed::Usb4_40G.to_string().contains("40 Gbps"));
        assert!(UsbSpeed::Usb4_80G.to_string().contains("80 Gbps"));
        assert!(UsbSpeed::HighSpeed.to_string().contains("480 Mbps"));
    }

    #[test]
    fn test_pool_health_display() {
        assert_eq!(PoolHealth::Online.to_string(), "ONLINE");
        assert_eq!(PoolHealth::Degraded.to_string(), "DEGRADED");
        assert_eq!(PoolHealth::Faulted.to_string(), "FAULTED");
        assert_eq!(PoolHealth::Offline.to_string(), "OFFLINE");
        assert_eq!(PoolHealth::Unavailable.to_string(), "UNAVAIL");
        assert_eq!(PoolHealth::Removed.to_string(), "REMOVED");
        assert_eq!(PoolHealth::Unknown.to_string(), "UNKNOWN");
    }

    #[test]
    fn test_device_info_serialization() {
        let device = DeviceInfo {
            device_path: PathBuf::from("/dev/disk5"),
            stable_id: "usb-WD-123".to_string(),
            model: "WD_BLACK SN850X".to_string(),
            vendor_id: Some("Western Digital".to_string()),
            product_id: None,
            serial: Some("ABC123".to_string()),
            usb_speed: UsbSpeed::Usb4_40G,
            capacity_bytes: Some(8_000_000_000_000),
            is_removable: true,
            detected_fs: Some("zfs_member".to_string()),
        };

        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("WD_BLACK SN850X"));
        assert!(json.contains("Usb4_40G"));

        let deserialized: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, "WD_BLACK SN850X");
        assert_eq!(deserialized.usb_speed, UsbSpeed::Usb4_40G);
    }

    #[test]
    fn test_pool_info_serialization() {
        let pool = PoolInfo {
            name: "tank".to_string(),
            guid: "123456".to_string(),
            health: PoolHealth::Online,
            size_bytes: 1_000_000_000_000,
            allocated_bytes: 500_000_000_000,
            free_bytes: 500_000_000_000,
            encrypted: true,
            mounted: true,
            mountpoint: Some(PathBuf::from("/Volumes/tank")),
            datasets: vec![],
            features: vec!["encryption".to_string()],
            version: "2.3".to_string(),
        };

        let json = serde_json::to_string(&pool).unwrap();
        assert!(json.contains("tank"));
        let deserialized: PoolInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "tank");
        assert_eq!(deserialized.encrypted, true);
    }
}

/// Information about a detected USB storage device
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Platform device path, e.g. /dev/disk5 or /dev/sdb
    pub device_path: PathBuf,
    /// Stable identifier (serial, WWN, or USB ID)
    pub stable_id: String,
    /// Human-readable model name
    pub model: String,
    /// USB vendor ID
    pub vendor_id: Option<String>,
    /// USB product ID
    pub product_id: Option<String>,
    /// Serial number from device
    pub serial: Option<String>,
    /// Connection speed
    pub usb_speed: UsbSpeed,
    /// Total capacity in bytes
    pub capacity_bytes: Option<u64>,
    /// Whether device is removable
    pub is_removable: bool,
    /// Filesystem type detected by OS (if any)
    pub detected_fs: Option<String>,
}

/// Information about a ZFS pool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolInfo {
    /// Pool name
    pub name: String,
    /// Pool GUID
    pub guid: String,
    /// Current health status
    pub health: PoolHealth,
    /// Total size in bytes
    pub size_bytes: u64,
    /// Allocated bytes
    pub allocated_bytes: u64,
    /// Free bytes
    pub free_bytes: u64,
    /// Whether the pool is encrypted
    pub encrypted: bool,
    /// Whether the pool is currently mounted
    pub mounted: bool,
    /// Mount point (if mounted)
    pub mountpoint: Option<PathBuf>,
    /// Dataset properties
    pub datasets: Vec<DatasetInfo>,
    /// Enabled feature flags
    pub features: Vec<String>,
    /// Current version / compatibility level
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolHealth {
    Online,
    Degraded,
    Faulted,
    Offline,
    Unavailable,
    Removed,
    Unknown,
}

impl fmt::Display for PoolHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolHealth::Online => write!(f, "ONLINE"),
            PoolHealth::Degraded => write!(f, "DEGRADED"),
            PoolHealth::Faulted => write!(f, "FAULTED"),
            PoolHealth::Offline => write!(f, "OFFLINE"),
            PoolHealth::Unavailable => write!(f, "UNAVAIL"),
            PoolHealth::Removed => write!(f, "REMOVED"),
            PoolHealth::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Information about a ZFS dataset
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub name: String,
    pub mountpoint: Option<PathBuf>,
    pub mounted: bool,
    pub encrypted: bool,
    pub compression: Option<String>,
    pub recordsize: Option<u64>,
    pub quota: Option<u64>,
}
