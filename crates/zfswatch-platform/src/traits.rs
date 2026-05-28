use std::path::PathBuf;

use tokio::sync::mpsc;

use zfswatch_core::types::DeviceInfo;

/// Events emitted by the platform USB monitor
#[derive(Debug, Clone, PartialEq)]
pub enum UsbEvent {
    /// USB storage device connected
    DeviceInserted(DeviceInfo),
    /// USB storage device disconnected
    DeviceRemoved { stable_id: String },
}

/// Platform-specific backend for USB device detection
#[async_trait::async_trait]
pub trait PlatformBackend: Send + Sync {
    /// Start monitoring for USB storage devices
    async fn start_monitoring(&self) -> anyhow::Result<mpsc::Receiver<UsbEvent>>;

    /// Stop monitoring
    async fn stop_monitoring(&self) -> anyhow::Result<()>;

    /// Scan for currently connected USB storage devices
    async fn scan_devices(&self) -> anyhow::Result<Vec<DeviceInfo>>;

    /// Get the platform-specific socket path for daemon communication
    fn default_socket_path(&self) -> PathBuf;

    /// Check if running with sufficient privileges
    fn has_required_privileges(&self) -> bool;
}
