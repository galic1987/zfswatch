use std::path::PathBuf;

use tokio::sync::mpsc;
use tracing::info;

use zfswatch_core::types::{DeviceInfo, UsbSpeed};

use crate::traits::{PlatformBackend, UsbEvent};

/// macOS platform backend using IOKit and DiskArbitration
pub struct MacOsBackend {
    // Will hold notification port and runloop references when implemented
}

impl MacOsBackend {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl PlatformBackend for MacOsBackend {
    async fn start_monitoring(&self) -> anyhow::Result<mpsc::Receiver<UsbEvent>> {
        // TODO: Implement IOKit + DiskArbitration integration
        // This requires FFI bindings to CoreFoundation, IOKit, and DiskArbitration frameworks
        info!("macOS USB monitoring not yet implemented — using polling fallback");
        let (_tx, rx) = mpsc::channel(100);
        Ok(rx)
    }

    async fn stop_monitoring(&self) -> anyhow::Result<()> {
        info!("Stopping macOS USB monitoring");
        Ok(())
    }

    async fn scan_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        // Use diskutil to list external disks
        let output = tokio::process::Command::new("diskutil")
            .args(["list", "external", "physical"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();

        // Parse diskutil output to find external devices
        for line in stdout.lines() {
            if line.starts_with("/dev/disk") {
                let device_path = PathBuf::from(line.trim());
                let disk_name = device_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                // Get disk info
                let _info_output = tokio::process::Command::new("diskutil")
                    .args(["info", "-plist", &disk_name])
                    .output()
                    .await?;

                // For now, create basic device info
                devices.push(DeviceInfo {
                    device_path: device_path.clone(),
                    stable_id: disk_name.clone(),
                    model: "Unknown".to_string(),
                    vendor_id: None,
                    product_id: None,
                    serial: None,
                    usb_speed: UsbSpeed::Unknown,
                    capacity_bytes: None,
                    is_removable: true,
                    detected_fs: None,
                });
            }
        }

        Ok(devices)
    }

    fn default_socket_path(&self) -> PathBuf {
        PathBuf::from("/var/run/zfswatch.sock")
    }

    fn has_required_privileges(&self) -> bool {
        unsafe { libc::geteuid() == 0 }
    }
}
