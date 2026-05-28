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
