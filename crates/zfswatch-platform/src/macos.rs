use std::path::PathBuf;

use tokio::sync::mpsc;
use tracing::{error, info};

use zfswatch_core::types::{DeviceInfo, UsbSpeed};

use crate::traits::{PlatformBackend, UsbEvent};

/// macOS platform backend using DiskArbitration (via polling fallback for now)
pub struct MacOsBackend {
    stop_tx: Option<mpsc::Sender<()>>,
}

impl MacOsBackend {
    pub fn new() -> Self {
        Self { stop_tx: None }
    }

    /// Parse device info from diskutil output string
    fn parse_device_info_from_string(disk_name: &str, info: &str) -> Option<DeviceInfo> {
        // Check if external/removable
        let is_external = info.contains("External: Yes") || info.contains("External:  Yes");
        let is_removable = info.contains("Removable Media: Yes") || info.contains("Removable Media:  Yes");
        let is_ejectable = info.contains("Ejectable: Yes") || info.contains("Ejectable:  Yes");

        if !is_external && !is_removable && !is_ejectable {
            return None;
        }

        // Extract model
        let model = info
            .lines()
            .find(|l| l.contains("Device / Media Name:"))
            .map(|l| {
                l.split(":")
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|| disk_name.to_string());

        // Extract size
        let capacity_bytes = info
            .lines()
            .find(|l| l.contains("Disk Size:"))
            .and_then(|l| {
                l.split("(")
                    .nth(1)
                    .and_then(|s| s.split("Bytes").next())
                    .and_then(|s| s.trim().parse::<u64>().ok())
            });

        // Determine USB speed from protocol
        let usb_speed = if info.contains("PCI-Express") || info.contains("Thunderbolt") {
            UsbSpeed::Usb4_40G
        } else if info.contains("USB") {
            UsbSpeed::SuperSpeed10
        } else {
            UsbSpeed::Unknown
        };

        Some(DeviceInfo {
            device_path: PathBuf::from(format!("/dev/{disk_name}")),
            stable_id: disk_name.to_string(),
            model,
            vendor_id: None,
            product_id: None,
            serial: None,
            usb_speed,
            capacity_bytes,
            is_removable: is_removable || is_ejectable,
            detected_fs: None,
        })
    }
}

#[async_trait::async_trait]
impl PlatformBackend for MacOsBackend {
    async fn start_monitoring(&self) -> anyhow::Result<mpsc::Receiver<UsbEvent>> {
        info!("macOS native monitoring not yet implemented — using polling fallback");
        let (event_tx, event_rx) = mpsc::channel(100);
        let (_stop_tx, mut stop_rx) = mpsc::channel(1);

        tokio::spawn(async move {
            let mut known_devices: std::collections::HashSet<String> = std::collections::HashSet::new();

            loop {
                match stop_rx.try_recv() {
                    Ok(()) => break,
                    Err(_) => {}
                }

                match tokio::process::Command::new("diskutil")
                    .args(["list", "external", "physical"])
                    .output()
                    .await
                {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let mut current_devices = std::collections::HashSet::new();

                        for line in stdout.lines() {
                            if line.starts_with("/dev/disk") {
                                let disk_name = line
                                    .trim()
                                    .strip_prefix("/dev/")
                                    .unwrap_or("")
                                    .to_string();

                                if disk_name.is_empty() {
                                    continue;
                                }

                                current_devices.insert(disk_name.clone());

                                if !known_devices.contains(&disk_name) {
                                    if let Some(info) = Self::parse_device_info_from_string(&disk_name, &"External: Yes\n") {
                                        let _ = event_tx
                                            .send(UsbEvent::DeviceInserted(info))
                                            .await;
                                    }
                                }
                            }
                        }

                        for old_device in &known_devices {
                            if !current_devices.contains(old_device) {
                                let _ = event_tx
                                    .send(UsbEvent::DeviceRemoved {
                                        stable_id: old_device.clone(),
                                    })
                                    .await;
                            }
                        }

                        known_devices = current_devices;
                    }
                    Err(e) => {
                        error!("diskutil scan failed: {e}");
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });

        Ok(event_rx)
    }

    async fn stop_monitoring(&self) -> anyhow::Result<()> {
        if let Some(tx) = &self.stop_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    async fn scan_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        let output = tokio::process::Command::new("diskutil")
            .args(["list", "external", "physical"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();

        for line in stdout.lines() {
            if line.starts_with("/dev/disk") {
                let disk_name = line
                    .trim()
                    .strip_prefix("/dev/")
                    .unwrap_or("")
                    .to_string();

                if let Some(info) = Self::parse_device_info_from_string(&disk_name, &"External: Yes\n") {
                    devices.push(info);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_info_external() {
        let info = "Device Identifier: disk5\nExternal: Yes\nRemovable Media: Fixed\nEjectable: Yes\nDevice / Media Name: WD_BLACK\nProtocol: PCI-Express\nDisk Size: 8.0 TB\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk5", info).unwrap();
        assert_eq!(dev.device_path, PathBuf::from("/dev/disk5"));
        assert_eq!(dev.model, "WD_BLACK");
        assert_eq!(dev.usb_speed, UsbSpeed::Usb4_40G);
        assert!(dev.is_removable);
    }

    #[test]
    fn test_parse_device_info_not_external() {
        let info = "Device Identifier: disk0\nExternal: No\n";
        assert!(MacOsBackend::parse_device_info_from_string("disk0", info).is_none());
    }

    #[test]
    fn test_parse_device_info_usb_speed() {
        let tb_info = "External: Yes\nProtocol: PCI-Express\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk5", tb_info).unwrap();
        assert_eq!(dev.usb_speed, UsbSpeed::Usb4_40G);

        let usb_info = "External: Yes\nProtocol: USB\n";
        let dev2 = MacOsBackend::parse_device_info_from_string("disk6", usb_info).unwrap();
        assert_eq!(dev2.usb_speed, UsbSpeed::SuperSpeed10);
    }

    #[test]
    fn test_parse_device_info_capacity() {
        let info = "External: Yes\nDisk Size: 4.0 TB (4000787030016 Bytes)\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk4", info).unwrap();
        assert_eq!(dev.capacity_bytes, Some(4000787030016));
    }

    #[test]
    fn test_macos_backend_default_socket() {
        let backend = MacOsBackend::new();
        assert_eq!(backend.default_socket_path(), PathBuf::from("/var/run/zfswatch.sock"));
    }

    #[test]
    fn test_macos_backend_privileges() {
        let backend = MacOsBackend::new();
        assert_eq!(backend.has_required_privileges(), false);
    }

    #[test]
    fn test_parse_device_info_removable_media() {
        let info = "External: No\nRemovable Media: Yes\nDevice / Media Name: SD Card\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk2", info).unwrap();
        assert_eq!(dev.model, "SD Card");
        assert!(dev.is_removable);
    }

    #[test]
    fn test_parse_device_info_unknown_speed() {
        let info = "External: Yes\nEjectable: Yes\nProtocol: SATA\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk3", info).unwrap();
        assert_eq!(dev.usb_speed, UsbSpeed::Unknown);
    }

    #[test]
    fn test_parse_device_info_no_size() {
        let info = "External: Yes\nEjectable: Yes\nDevice / Media Name: NoSize\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk7", info).unwrap();
        assert_eq!(dev.capacity_bytes, None);
        assert_eq!(dev.model, "NoSize");
    }

    #[test]
    fn test_parse_device_info_usb_protocol() {
        let info = "External: Yes\nEjectable: Yes\nProtocol: USB\nDevice / Media Name: USBDrive\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk8", info).unwrap();
        assert_eq!(dev.usb_speed, UsbSpeed::SuperSpeed10);
        assert_eq!(dev.model, "USBDrive");
    }

    #[test]
    fn test_parse_device_info_default_model() {
        let info = "External: Yes\nEjectable: Yes\n";
        let dev = MacOsBackend::parse_device_info_from_string("disk9", info).unwrap();
        assert_eq!(dev.model, "disk9");
    }
}
