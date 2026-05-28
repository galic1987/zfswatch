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

    /// Parse a device entry from diskutil info output
    async fn parse_device_info(disk_name: &str) -> Option<DeviceInfo> {
        let output = tokio::process::Command::new("diskutil")
            .args(["info", disk_name])
            .output()
            .await
            .ok()?;

        let info = String::from_utf8_lossy(&output.stdout);

        // Check if external/removable
        let is_external = info.contains("External") && !info.contains("No");
        let is_removable = info.contains("Removable") && !info.contains("No");
        let is_ejectable = info.contains("Ejectable") && !info.contains("No");

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
                // Parse "8.0 TB (8001563222016 Bytes)"
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
        // TODO: Implement native DiskArbitration/IOKit notifications
        // For now, use polling fallback
        info!("macOS native monitoring not yet implemented — using polling fallback");
        let (event_tx, event_rx) = mpsc::channel(100);
        let (_stop_tx, mut stop_rx) = mpsc::channel(1);

        tokio::spawn(async move {
            let mut known_devices: std::collections::HashSet<String> = std::collections::HashSet::new();

            loop {
                // Check stop signal
                match stop_rx.try_recv() {
                    Ok(()) => break,
                    Err(_) => {}
                }

                // Scan for external devices
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

                                // New device detected
                                if !known_devices.contains(&disk_name) {
                                    if let Some(info) = Self::parse_device_info(&disk_name).await {
                                        let _ = event_tx
                                            .send(UsbEvent::DeviceInserted(info))
                                            .await;
                                    }
                                }
                            }
                        }

                        // Check for removed devices
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

                if let Some(info) = Self::parse_device_info(&disk_name).await {
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
