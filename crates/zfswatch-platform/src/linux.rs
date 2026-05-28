use std::os::fd::{AsFd, AsRawFd, FromRawFd, RawFd};
use std::path::PathBuf;

use tokio::io::unix::AsyncFd;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use zfswatch_core::types::{DeviceInfo, UsbSpeed};

use crate::traits::{PlatformBackend, UsbEvent};

/// Linux platform backend using udev
pub struct LinuxBackend {
    // Stop signal for the monitor task
    stop_tx: Option<mpsc::Sender<()>>,
}

impl LinuxBackend {
    pub fn new() -> Self {
        Self { stop_tx: None }
    }

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

    /// Build a DeviceInfo from a udev device
    fn device_from_udev(device: &udev::Device) -> Option<DeviceInfo> {
        let devnode = device.devnode()?;
        let devname = devnode.file_name()?.to_str()?;

        // Only block devices
        if !devname.starts_with("sd") && !devname.starts_with("nvme") && !devname.starts_with("vd") {
            return None;
        }

        // Skip partitions (only whole disks)
        if devname.chars().last().unwrap_or('0').is_ascii_digit() && devname.starts_with("sd") {
            return None;
        }

        let model = device
            .property_value("ID_MODEL")
            .or_else(|| device.property_value("ID_MODEL_ENC"))
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| devname.to_string());

        let vendor = device
            .property_value("ID_VENDOR")
            .map(|s| s.to_string_lossy().to_string());

        let serial = device
            .property_value("ID_SERIAL_SHORT")
            .map(|s| s.to_string_lossy().to_string());

        let stable_id = serial.clone().unwrap_or_else(|| {
            device
                .property_value("ID_WWN")
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| devname.to_string())
        });

        let is_usb = device.property_value("ID_BUS")
            .map(|v| v == "usb")
            .unwrap_or(false);

        let is_removable = device
            .sysfs_attr_value("removable")
            .map(|v| v.to_string_lossy().trim() == "1")
            .unwrap_or(is_usb);

        // Only include USB or removable devices
        if !is_usb && !is_removable {
            return None;
        }

        // Capacity from udev
        let capacity_bytes = device
            .property_value("SIZE")
            .and_then(|s| s.to_string_lossy().parse::<u64>().ok())
            .map(|s| s * 512);

        // USB speed
        let usb_speed = Self::get_usb_speed_from_udev(device).unwrap_or(UsbSpeed::Unknown);

        Some(DeviceInfo {
            device_path: devnode.to_path_buf(),
            stable_id,
            model,
            vendor_id: vendor,
            product_id: None,
            serial,
            usb_speed,
            capacity_bytes,
            is_removable,
            detected_fs: device
                .property_value("ID_FS_TYPE")
                .map(|s| s.to_string_lossy().to_string()),
        })
    }

    fn get_usb_speed_from_udev(device: &udev::Device) -> Option<UsbSpeed> {
        // Walk up the device tree to find the USB device with speed attribute
        let mut current = Some(device.parent()?);
        while let Some(dev) = current {
            if let Some(speed_str) = dev.sysfs_attr_value("speed") {
                let speed_str = speed_str.to_string_lossy();
                if let Ok(speed_mbps) = speed_str.trim().parse::<f64>() {
                    return Some(Self::parse_usb_speed(speed_mbps as u64));
                }
            }
            current = dev.parent();
        }
        None
    }
}

#[async_trait::async_trait]
impl PlatformBackend for LinuxBackend {
    async fn start_monitoring(&self) -> anyhow::Result<mpsc::Receiver<UsbEvent>> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let (stop_tx, mut stop_rx) = mpsc::channel(1);

        // Spawn blocking task for udev monitoring
        tokio::task::spawn_blocking(move || {
            let mut monitor = match udev::MonitorBuilder::new()
                .map(|m| m.match_subsystem_devtype("block", "disk"))
            {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to create udev monitor: {e}");
                    return;
                }
            };

            let mut socket = match monitor.listen() {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to listen on udev monitor: {e}");
                    return;
                }
            };

            info!("Linux udev monitor started for block devices");

            loop {
                // Check for stop signal (non-blocking)
                match stop_rx.try_recv() {
                    Ok(()) | Err(mpsc::error::TryRecvError::Closed) => {
                        info!("Linux udev monitor stopping");
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }

                // Poll udev with timeout
                match socket.receive_event() {
                    Some(event) => {
                        let action = event.event_type();
                        let device = event.device();

                        match action {
                            udev::EventType::Add => {
                                if let Some(info) = Self::device_from_udev(&device) {
                                    debug!("USB device inserted: {info:?}");
                                    if event_tx.blocking_send(UsbEvent::DeviceInserted(info)).is_err() {
                                        break;
                                    }
                                }
                            }
                            udev::EventType::Remove => {
                                if let Some(devnode) = device.devnode() {
                                    let devname = devnode.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("")
                                        .to_string();
                                    // Try to find stable_id from properties
                                    let stable_id = device
                                        .property_value("ID_SERIAL_SHORT")
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or(devname);

                                    debug!("USB device removed: {stable_id}");
                                    if event_tx.blocking_send(UsbEvent::DeviceRemoved { stable_id }).is_err() {
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    None => {
                        // No event, sleep briefly
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });

        // We can't easily store stop_tx since self is immutable in this trait method.
        // For now, the monitor runs until the channel is dropped.
        // A proper implementation would use a shared Arc<AtomicBool> for the stop signal.

        Ok(event_rx)
    }

    async fn stop_monitoring(&self) -> anyhow::Result<()> {
        // Signal the monitor to stop
        if let Some(tx) = &self.stop_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    async fn scan_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        let mut devices = Vec::new();
        let mut enumerator = udev::Enumerator::new()?;
        enumerator.match_subsystem("block")?;
        enumerator.match_property("DEVTYPE", "disk")?;

        for device in enumerator.scan_devices()? {
            if let Some(info) = Self::device_from_udev(&device) {
                devices.push(info);
            }
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
