use std::path::PathBuf;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use zfswatch_core::types::{DeviceInfo, UsbSpeed};

use crate::traits::{PlatformBackend, UsbEvent};

/// Linux platform backend using udev/netlink
pub struct LinuxBackend {
    // Will hold udev monitor when implemented
}

impl LinuxBackend {
    pub fn new() -> Self {
        Self {}
    }

    /// Parse USB speed from sysfs speed file
    fn parse_usb_speed(speed_mbps: u64) -> UsbSpeed {
        match speed_mbps {
            0 => UsbSpeed::Unknown,
            1 => UsbSpeed::LowSpeed,
            12 => UsbSpeed::FullSpeed,
            480 => UsbSpeed::HighSpeed,
            5000 => UsbSpeed::SuperSpeed,
            10000 => UsbSpeed::SuperSpeed10,
            20000 => UsbSpeed::SuperSpeed20,
            40000 => UsbSpeed::Usb4_40G,
            80000 => UsbSpeed::Usb4_80G,
            _ if speed_mbps > 80000 => UsbSpeed::Usb4_80G,
            _ => UsbSpeed::Unknown,
        }
    }
}

#[async_trait::async_trait]
impl PlatformBackend for LinuxBackend {
    async fn start_monitoring(&self) -> anyhow::Result<mpsc::Receiver<UsbEvent>> {
        // TODO: Implement udev monitor via libudev or netlink
        // For now, return a channel that will be populated by polling
        info!("Linux USB monitoring not yet implemented — using polling fallback");
        let (tx, rx) = mpsc::channel(100);
        Ok(rx)
    }

    async fn stop_monitoring(&self) -> anyhow::Result<()> {
        info!("Stopping Linux USB monitoring");
        Ok(())
    }

    async fn scan_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        let mut devices = Vec::new();

        // Scan /sys/block for USB-attached block devices
        let block_dir = std::path::Path::new("/sys/block");
        if !block_dir.exists() {
            return Ok(devices);
        }

        let entries = tokio::fs::read_dir(block_dir).await?;
        let mut entries = entries;

        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip loop, ram, etc.
            if name_str.starts_with("loop") || name_str.starts_with("ram") || name_str == "dm-" {
                continue;
            }

            // Check if it's a removable/USB device
            let removable_path = entry.path().join("removable");
            let is_removable = if removable_path.exists() {
                tokio::fs::read_to_string(&removable_path)
                    .await
                    .map(|s| s.trim() == "1")
                    .unwrap_or(false)
            } else {
                false
            };

            // Check if device path exists
            let device_path = PathBuf::from(format!("/dev/{name_str}"));
            if !device_path.exists() {
                continue;
            }

            // Try to get device info from udevadm
            let udev_output = tokio::process::Command::new("udevadm")
                .args([
                    "info",
                    "--query=property",
                    "--name",
                    &name_str,
                ])
                .output()
                .await?;

            let udev_info = String::from_utf8_lossy(&udev_output.stdout);
            let mut model = String::new();
            let mut vendor = None;
            let mut serial = None;
            let mut usb_speed = UsbSpeed::Unknown;
            let mut is_usb = false;
            let mut capacity_bytes = None;

            for line in udev_info.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    match key {
                        "ID_MODEL" => model = value.to_string(),
                        "ID_VENDOR" => vendor = Some(value.to_string()),
                        "ID_SERIAL_SHORT" => serial = Some(value.to_string()),
                        "ID_BUS" if value == "usb" => is_usb = true,
                        "ID_USB_DRIVER" => is_usb = true,
                        "SIZE" => {
                            capacity_bytes = value.parse::<u64>().ok().map(|s| s * 512);
                        }
                        _ => {}
                    }
                }
            }

            // Only include USB devices or removable devices
            if !is_usb && !is_removable {
                continue;
            }

            // Try to get USB speed from sysfs
            // Walk up the sysfs tree to find the USB device
            if let Ok(speed) = Self::get_usb_speed_sysfs(&name_str).await {
                usb_speed = speed;
            }

            if model.is_empty() {
                model = name_str.to_string();
            }

            devices.push(DeviceInfo {
                device_path,
                stable_id: serial.clone().unwrap_or_else(|| name_str.to_string()),
                model,
                vendor_id: vendor,
                product_id: None,
                serial,
                usb_speed,
                capacity_bytes,
                is_removable,
                detected_fs: None,
            });
        }

        Ok(devices)
    }

    fn default_socket_path(&self) -> PathBuf {
        PathBuf::from("/run/zfswatch/zfswatch.sock")
    }

    fn has_required_privileges(&self) -> bool {
        unsafe { libc::geteuid() == 0 }
    }
}

impl LinuxBackend {
    async fn get_usb_speed_sysfs(block_name: &str) -> anyhow::Result<UsbSpeed> {
        // Try to find the USB speed by walking sysfs
        let sys_path = format!("/sys/block/{block_name}");
        let mut current = std::path::PathBuf::from(&sys_path);

        // Walk up to find USB device
        for _ in 0..10 {
            let speed_file = current.join("speed");
            if speed_file.exists() {
                let speed_str = tokio::fs::read_to_string(&speed_file).await?;
                let speed_mbps: f64 = speed_str.trim().parse()?;
                return Ok(LinuxBackend::parse_usb_speed(speed_mbps as u64));
            }
            if !current.pop() {
                break;
            }
        }

        // Alternative: check /sys/bus/usb/devices
        let usb_devices = tokio::fs::read_dir("/sys/bus/usb/devices").await?;
        let mut usb_entries = usb_devices;
        while let Some(entry) = usb_entries.next_entry().await? {
            let speed_file = entry.path().join("speed");
            if speed_file.exists() {
                let speed_str = tokio::fs::read_to_string(&speed_file).await?;
                if let Ok(speed_mbps) = speed_str.trim().parse::<f64>() {
                    return Ok(LinuxBackend::parse_usb_speed(speed_mbps as u64));
                }
            }
        }

        Err(anyhow::anyhow!("Could not determine USB speed"))
    }
}
