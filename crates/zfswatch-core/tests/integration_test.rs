use std::path::PathBuf;

use zfswatch_core::{
    config::{Config, DaemonConfig, EncryptionConfig, KeySource, LogFormat, PerformanceConfig, PoolConfig},
    protocol::{DaemonRequest, DaemonResponse, EventNotification, PoolCreationOptions},
    types::{DeviceInfo, PoolHealth, PoolInfo, UsbSpeed},
};

#[test]
fn test_config_full() {
    let config = Config {
        daemon: DaemonConfig {
            socket_path: PathBuf::from("/tmp/test.sock"),
            auto_detect: true,
            auto_mount: false,
            confirm_unknown_pools: true,
            scan_interval_sec: 30,
        },
        encryption: EncryptionConfig {
            default_algorithm: "aes-256-gcm".to_string(),
            default_keyformat: "passphrase".to_string(),
            default_keylocation: "prompt".to_string(),
            min_passphrase_length: 12,
        },
        performance: PerformanceConfig {
            arc_max_mb: 8192,
            recordsize_kb: 256,
            disable_atime: true,
            compression: "lz4".to_string(),
        },
        pools: vec![
            PoolConfig {
                name: "tank".to_string(),
                device_uuid: Some("usb-123".to_string()),
                auto_import: true,
                auto_mount: false,
                key_source: KeySource::Prompt,
                key_file: None,
                key_url: None,
            },
        ],
        logging: zfswatch_core::config::LoggingConfig {
            level: "debug".to_string(),
            format: LogFormat::Json,
            file: Some(PathBuf::from("/tmp/zfswatch.log")),
        },
    };

    let toml = config.to_file(&PathBuf::from("/dev/null")).unwrap();
}

#[test]
fn test_protocol_all_variants() {
    let requests = vec![
        DaemonRequest::Ping,
        DaemonRequest::Status,
        DaemonRequest::ListPools,
        DaemonRequest::ListDevices,
        DaemonRequest::ImportPool { pool_name: "a".to_string(), device_path: None },
        DaemonRequest::ExportPool { pool_name: "a".to_string(), force: false },
        DaemonRequest::Mount { dataset: "a".to_string(), passphrase: None },
        DaemonRequest::Unmount { dataset: "a".to_string(), unload_key: true },
        DaemonRequest::InitPool {
            pool_name: "a".to_string(),
            device_path: PathBuf::from("/dev/sdb"),
            passphrase: "pass".to_string(),
            options: PoolCreationOptions::default(),
        },
        DaemonRequest::ChangeKey { dataset: "a".to_string(), old_passphrase: "old".to_string(), new_passphrase: "new".to_string() },
        DaemonRequest::SubscribeEvents,
        DaemonRequest::ReloadConfig,
    ];

    for req in requests {
        let json = serde_json::to_string(&req).unwrap();
        let _deserialized: DaemonRequest = serde_json::from_str(&json).unwrap();
    }

    let responses = vec![
        DaemonResponse::Pong,
        DaemonResponse::Success { message: "ok".to_string() },
        DaemonResponse::Error { code: "E".to_string(), message: "err".to_string() },
        DaemonResponse::EventStream,
    ];

    for resp in responses {
        let json = serde_json::to_string(&resp).unwrap();
        let _deserialized: DaemonResponse = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn test_types_comprehensive() {
    let speeds = vec![
        UsbSpeed::Unknown,
        UsbSpeed::LowSpeed,
        UsbSpeed::FullSpeed,
        UsbSpeed::HighSpeed,
        UsbSpeed::SuperSpeed,
        UsbSpeed::SuperSpeed10,
        UsbSpeed::SuperSpeed20,
        UsbSpeed::Usb4_40G,
        UsbSpeed::Usb4_80G,
    ];

    for speed in speeds {
        let desc = speed.to_string();
        assert!(!desc.is_empty());
        let _mbps = speed.max_throughput_mbps();
    }

    let healths = vec![
        PoolHealth::Online,
        PoolHealth::Degraded,
        PoolHealth::Faulted,
        PoolHealth::Offline,
        PoolHealth::Unavailable,
        PoolHealth::Removed,
        PoolHealth::Unknown,
    ];

    for health in healths {
        let s = health.to_string();
        assert!(!s.is_empty());
    }

    let device = DeviceInfo {
        device_path: PathBuf::from("/dev/sdb"),
        stable_id: "usb-123".to_string(),
        model: "Test".to_string(),
        vendor_id: Some("Vendor".to_string()),
        product_id: Some("Product".to_string()),
        serial: Some("SN123".to_string()),
        usb_speed: UsbSpeed::SuperSpeed,
        capacity_bytes: Some(1_000_000_000_000),
        is_removable: true,
        detected_fs: Some("zfs_member".to_string()),
    };

    let json = serde_json::to_string(&device).unwrap();
    let _d2: DeviceInfo = serde_json::from_str(&json).unwrap();

    let pool = PoolInfo {
        name: "tank".to_string(),
        guid: "guid123".to_string(),
        health: PoolHealth::Online,
        size_bytes: 1_000_000,
        allocated_bytes: 500_000,
        free_bytes: 500_000,
        encrypted: true,
        mounted: true,
        mountpoint: Some(PathBuf::from("/mnt")),
        datasets: vec![],
        features: vec![],
        version: "2.3".to_string(),
    };

    let json = serde_json::to_string(&pool).unwrap();
    let _p2: PoolInfo = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_event_notification_variants() {
    let now = chrono::Utc::now();
    let events = vec![
        EventNotification::DeviceInserted { device: DeviceInfo {
            device_path: PathBuf::from("/dev/sdb"),
            stable_id: "x".to_string(),
            model: "M".to_string(),
            vendor_id: None,
            product_id: None,
            serial: None,
            usb_speed: UsbSpeed::Unknown,
            capacity_bytes: None,
            is_removable: true,
            detected_fs: None,
        }, timestamp: now },
        EventNotification::DeviceRemoved { stable_id: "x".to_string(), timestamp: now },
        EventNotification::PoolImported { pool: PoolInfo {
            name: "p".to_string(),
            guid: "g".to_string(),
            health: PoolHealth::Online,
            size_bytes: 1,
            allocated_bytes: 0,
            free_bytes: 1,
            encrypted: false,
            mounted: false,
            mountpoint: None,
            datasets: vec![],
            features: vec![],
            version: "".to_string(),
        }, timestamp: now },
        EventNotification::PoolExported { pool_name: "p".to_string(), timestamp: now },
        EventNotification::PoolMounted { dataset: "p".to_string(), mountpoint: PathBuf::from("/mnt"), timestamp: now },
        EventNotification::PoolUnmounted { dataset: "p".to_string(), timestamp: now },
        EventNotification::PassphraseRequired { pool_name: "p".to_string(), device_path: PathBuf::from("/dev/sdb"), timestamp: now },
        EventNotification::Error { context: "ctx".to_string(), message: "msg".to_string(), timestamp: now },
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let _e2: EventNotification = serde_json::from_str(&json).unwrap();
    }
}
