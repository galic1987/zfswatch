use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use zfswatch_core::{
    config::Config,
    logging,
    protocol::{DaemonRequest, DaemonResponse},
    types::PoolInfo,
};
#[cfg(target_os = "linux")]
use zfswatch_keys::LinuxKeyring;
use zfswatch_keys::MemoryKeyVault;
use zfswatch_platform::create_backend;
use zfswatch_zfs::{DatasetManager, EncryptionManager, PoolManager, RealCommandRunner};

/// Daemon state shared across async tasks
struct DaemonState {
    config: Config,
    key_vault: Box<dyn zfswatch_keys::KeyStorage>,
    imported_pools: Vec<PoolInfo>,
}

fn new_pool_mgr() -> PoolManager<RealCommandRunner> {
    PoolManager::new(RealCommandRunner::new())
}

fn new_dataset_mgr() -> DatasetManager<RealCommandRunner> {
    DatasetManager::new(RealCommandRunner::new())
}

fn new_encryption_mgr() -> EncryptionManager<RealCommandRunner> {
    EncryptionManager::new(RealCommandRunner::new())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let socket_path = std::env::var("ZFSWATCH_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Config::default_path().parent().unwrap().join("zfswatch.sock"));

    logging::init_logging("info", zfswatch_core::config::LogFormat::Pretty)?;
    info!("Starting zfswatchd v{}", env!("CARGO_PKG_VERSION"));

    let backend = create_backend();
    if !backend.has_required_privileges() {
        warn!("zfswatchd is not running as root — ZFS operations may fail");
    }

    let pool_mgr = new_pool_mgr();

    if !pool_mgr.check_zfs_installed().await? {
        error!("ZFS does not appear to be installed or loaded on this system");
        error!("Run 'zfswatch doctor' for a full system diagnostic.");
        #[cfg(target_os = "macos")]
        {
            error!("On macOS:");
            error!("  1. Install:  brew install --cask openzfs");
            error!("  2. Approve:  System Settings → Privacy & Security → allow OpenZFS");
            error!("  3. Reboot, then re-run zfswatchd");
        }
        #[cfg(target_os = "linux")]
        {
            error!("On Debian/Ubuntu: sudo apt install zfsutils-linux zfs-dkms");
            error!("On Fedora:        sudo dnf install zfs");
            error!("On Arch:          sudo pacman -S zfs-utils");
            error!("Then: sudo modprobe zfs");
        }
        std::process::exit(1);
    }

    let version = pool_mgr.zfs_version().await?;
    info!("ZFS version: {version}");

    let config = Config::resolve(None).unwrap_or_default();
    info!("Configuration loaded");

    #[cfg(target_os = "linux")]
    let key_vault: Box<dyn zfswatch_keys::KeyStorage> = Box::new(LinuxKeyring::new());
    #[cfg(target_os = "macos")]
    let key_vault: Box<dyn zfswatch_keys::KeyStorage> = Box::new(MemoryKeyVault::new());

    let state = Arc::new(Mutex::new(DaemonState {
        config: config.clone(),
        key_vault,
        imported_pools: Vec::new(),
    }));

    let _ = std::fs::remove_file(&socket_path);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    info!("Listening on Unix socket: {socket_path:?}");

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o660);
        std::fs::set_permissions(&socket_path, perms)?;
    }

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, state).await {
                        warn!("Client handler error: {e}");
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {e}");
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    state: Arc<Mutex<DaemonState>>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: DaemonRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = DaemonResponse::Error {
                    code: "INVALID_REQUEST".to_string(),
                    message: e.to_string(),
                };
                send_response(&mut writer, &response).await?;
                line.clear();
                continue;
            }
        };

        let response = process_request(request, &state).await;
        send_response(&mut writer, &response).await?;
        line.clear();
    }

    Ok(())
}

async fn send_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &DaemonResponse,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

async fn process_request(
    request: DaemonRequest,
    state: &Arc<Mutex<DaemonState>>,
) -> DaemonResponse {
    match request {
        DaemonRequest::Ping => DaemonResponse::Pong,

        DaemonRequest::Status => {
            let state = state.lock().await;
            DaemonResponse::Status {
                version: env!("CARGO_PKG_VERSION").to_string(),
                uptime_sec: 0,
                pools_imported: state.imported_pools.clone(),
                devices_connected: Vec::new(),
                auto_detect_enabled: state.config.daemon.auto_detect,
            }
        }

        DaemonRequest::ListPools => {
            match new_pool_mgr().list_pools().await {
                Ok(pools) => DaemonResponse::PoolList(pools),
                Err(e) => DaemonResponse::Error {
                    code: "POOL_LIST_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ListDevices => {
            let backend = create_backend();
            match backend.scan_devices().await {
                Ok(devices) => DaemonResponse::DeviceList(devices),
                Err(e) => DaemonResponse::Error {
                    code: "DEVICE_SCAN_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ImportPool { pool_name, device_path } => {
            match new_pool_mgr().import_pool(&pool_name, device_path.as_ref()).await {
                Ok(()) => DaemonResponse::Success {
                    message: format!("Pool '{pool_name}' imported successfully"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "IMPORT_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ExportPool { pool_name, force } => {
            match new_pool_mgr().export_pool(&pool_name, force).await {
                Ok(()) => DaemonResponse::Success {
                    message: format!("Pool '{pool_name}' exported successfully"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "EXPORT_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::Mount { dataset, passphrase } => {
            let result: zfswatch_core::Result<std::path::PathBuf> = if let Some(pass) = passphrase {
                match new_encryption_mgr().load_key(&dataset, &pass).await {
                    Ok(()) => new_dataset_mgr().mount(&dataset).await,
                    Err(e) => Err(e),
                }
            } else {
                new_dataset_mgr().mount(&dataset).await
            };

            match result {
                Ok(mp) => DaemonResponse::Success {
                    message: format!("Dataset '{dataset}' mounted at {mp:?}"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "MOUNT_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::Unmount { dataset, unload_key } => {
            match new_dataset_mgr().unmount(&dataset, unload_key).await {
                Ok(()) => DaemonResponse::Success {
                    message: format!("Dataset '{dataset}' unmounted"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "UNMOUNT_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::InitPool { pool_name, device_path, passphrase, options } => {
            match new_pool_mgr().create_pool(&pool_name, &device_path, &passphrase, &options).await {
                Ok(()) => DaemonResponse::Success {
                    message: format!("Pool '{pool_name}' created on {device_path:?} with encryption"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "INIT_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ChangeKey { dataset, old_passphrase, new_passphrase } => {
            match new_encryption_mgr().change_key(&dataset, &old_passphrase, &new_passphrase).await {
                Ok(()) => DaemonResponse::Success {
                    message: format!("Passphrase changed for '{dataset}'"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "CHANGE_KEY_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::SubscribeEvents => {
            DaemonResponse::EventStream
        }

        DaemonRequest::ReloadConfig => {
            let mut state = state.lock().await;
            match Config::resolve(None) {
                Ok(new_config) => {
                    state.config = new_config;
                    DaemonResponse::Success {
                        message: "Configuration reloaded".to_string(),
                    }
                }
                Err(e) => DaemonResponse::Error {
                    code: "CONFIG_ERROR".to_string(),
                    message: e.to_string(),
                },
            }
        }
    }
}
