use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;


use zfswatch_core::{
    config::Config,
    logging,
    prereqs::{PrerequisiteChecker, SystemPrerequisiteChecker},
    protocol::{DaemonRequest, DaemonResponse, PoolCreationOptions},
};

#[derive(Parser)]
#[command(name = "zfswatch")]
#[command(about = "ZFS USB auto-mount and encryption tool")]
#[command(version)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Unix socket path for daemon communication
    #[arg(short, long, value_name = "PATH")]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new encrypted ZFS pool on a device
    Init {
        /// Pool name
        name: String,
        /// Device path (e.g., /dev/disk5 or /dev/sdb)
        device: PathBuf,
        /// Disable encryption
        #[arg(long)]
        no_encrypt: bool,
        /// Use cross-platform safe feature flags
        #[arg(long, default_value = "true")]
        cross_platform: bool,
    },
    /// List ZFS pools
    List {
        /// Show detailed info
        #[arg(short, long)]
        verbose: bool,
    },
    /// List connected USB storage devices
    Devices,
    /// Import a pool
    Import {
        /// Pool name
        name: String,
        /// Device path hint
        #[arg(short, long)]
        device: Option<PathBuf>,
    },
    /// Export (safely unmount) a pool
    Export {
        /// Pool name
        name: String,
        /// Force export
        #[arg(short, long)]
        force: bool,
    },
    /// Mount a dataset
    Mount {
        /// Dataset name (e.g., pool/dataset)
        dataset: String,
    },
    /// Unmount a dataset
    Unmount {
        /// Dataset name
        dataset: String,
        /// Also unload encryption key
        #[arg(short, long)]
        unload_key: bool,
    },
    /// Change passphrase for an encrypted dataset
    ChangeKey {
        /// Dataset or pool name
        dataset: String,
    },
    /// Show daemon status
    Status,
    /// Start the daemon (for systemd/launchd integration)
    #[command(name = "daemon-start")]
    DaemonStart,
    /// Run system diagnostics and check prerequisites
    Doctor {
        /// Target disk to check (optional)
        #[arg(short, long)]
        disk: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging for CLI
    logging::init_logging("warn", zfswatch_core::config::LogFormat::Compact)?;

    let socket_path = cli
        .socket
        .or_else(|| {
            Config::resolve(cli.config.as_ref())
                .ok()
                .map(|c| c.daemon.socket_path)
        })
        .unwrap_or_else(|| {
            #[cfg(target_os = "macos")]
            {
                PathBuf::from("/var/run/zfswatch.sock")
            }
            #[cfg(target_os = "linux")]
            {
                PathBuf::from("/run/zfswatch/zfswatch.sock")
            }
        });

    match cli.command {
        Commands::Init {
            name,
            device,
            no_encrypt,
            cross_platform,
        } => {
            let passphrase = if no_encrypt {
                String::new()
            } else {
                prompt_passphrase("Enter new passphrase: ")?
            };

            if !no_encrypt {
                let confirm = prompt_passphrase("Confirm passphrase: ")?;
                if passphrase != confirm {
                    anyhow::bail!("Passphrases do not match");
                }
                if passphrase.len() < 8 {
                    anyhow::bail!("Passphrase must be at least 8 characters");
                }
            }

            let request = DaemonRequest::InitPool {
                pool_name: name.clone(),
                device_path: device.clone(),
                passphrase,
                options: PoolCreationOptions {
                    encryption: !no_encrypt,
                    cross_platform_safe: cross_platform,
                    ..Default::default()
                },
            };

            match send_request(&socket_path, request).await? {
                DaemonResponse::Success { message } => {
                    println!("✓ {message}");
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::List { verbose } => {
            let request = DaemonRequest::ListPools;
            match send_request(&socket_path, request).await? {
                DaemonResponse::PoolList(pools) => {
                    if pools.is_empty() {
                        println!("No ZFS pools found.");
                    } else {
                        println!("{:<20} {:>10} {:>10} {:>12} {:>10}",
                            "NAME", "SIZE", "USED", "HEALTH", "ENCRYPTED");
                        for pool in pools {
                            let size_gb = pool.size_bytes / (1024 * 1024 * 1024);
                            let used_gb = pool.allocated_bytes / (1024 * 1024 * 1024);
                            println!("{:<20} {:>8}G {:>8}G {:>12} {:>10}",
                                pool.name,
                                size_gb,
                                used_gb,
                                pool.health.to_string(),
                                if pool.encrypted { "yes" } else { "no" }
                            );
                            if verbose {
                                for ds in &pool.datasets {
                                    println!("  {:<30} {:>10}",
                                        ds.name,
                                        if ds.mounted { "mounted" } else { "-" });
                                }
                            }
                        }
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::Devices => {
            let request = DaemonRequest::ListDevices;
            match send_request(&socket_path, request).await? {
                DaemonResponse::DeviceList(devices) => {
                    if devices.is_empty() {
                        println!("No USB storage devices detected.");
                    } else {
                        println!("{:<15} {:<30} {:<15} {:<20}",
                            "DEVICE", "MODEL", "SIZE", "SPEED");
                        for dev in devices {
                            let size = dev.capacity_bytes.map(|b| {
                                if b > 1024u64.pow(4) {
                                    format!("{}T", b / 1024u64.pow(4))
                                } else if b > 1024u64.pow(3) {
                                    format!("{}G", b / 1024u64.pow(3))
                                } else if b > 1024u64.pow(2) {
                                    format!("{}M", b / 1024u64.pow(2))
                                } else {
                                    format!("{}B", b)
                                }
                            }).unwrap_or_else(|| "?".to_string());
                            println!("{:<15} {:<30} {:<15} {:<20}",
                                dev.device_path.display(),
                                dev.model.chars().take(28).collect::<String>(),
                                size,
                                format!("{}", dev.usb_speed),
                            );
                        }
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::Import { name, device } => {
            let request = DaemonRequest::ImportPool {
                pool_name: name.clone(),
                device_path: device,
            };
            match send_request(&socket_path, request).await? {
                DaemonResponse::Success { message } => {
                    println!("✓ {message}");
                }
                DaemonResponse::Error { code, message } => {
                    // If it's an encryption error, prompt for passphrase
                    if message.contains("encryption") || message.contains("key") {
                        println!("Pool appears to be encrypted.");
                        let passphrase = prompt_passphrase("Enter passphrase: ")?;
                        let mount_req = DaemonRequest::Mount {
                            dataset: name,
                            passphrase: Some(passphrase),
                        };
                        match send_request(&socket_path, mount_req).await? {
                            DaemonResponse::Success { message } => {
                                println!("✓ {message}");
                            }
                            DaemonResponse::Error { code, message } => {
                                eprintln!("✗ Error [{code}]: {message}");
                                std::process::exit(1);
                            }
                            _ => {}
                        }
                    } else {
                        eprintln!("✗ Error [{code}]: {message}");
                        std::process::exit(1);
                    }
                }
                _ => {}
            }
        }

        Commands::Export { name, force } => {
            let request = DaemonRequest::ExportPool { pool_name: name, force };
            match send_request(&socket_path, request).await? {
                DaemonResponse::Success { message } => {
                    println!("✓ {message}");
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::Mount { dataset } => {
            let passphrase = prompt_passphrase_opt("Passphrase (empty if unencrypted): ")?;
            let request = DaemonRequest::Mount {
                dataset,
                passphrase,
            };
            match send_request(&socket_path, request).await? {
                DaemonResponse::Success { message } => {
                    println!("✓ {message}");
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::Unmount { dataset, unload_key } => {
            let request = DaemonRequest::Unmount { dataset, unload_key };
            match send_request(&socket_path, request).await? {
                DaemonResponse::Success { message } => {
                    println!("✓ {message}");
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::ChangeKey { dataset } => {
            let old_pass = prompt_passphrase("Current passphrase: ")?;
            let new_pass = prompt_passphrase("New passphrase: ")?;
            let confirm = prompt_passphrase("Confirm new passphrase: ")?;
            if new_pass != confirm {
                anyhow::bail!("Passphrases do not match");
            }

            let request = DaemonRequest::ChangeKey {
                dataset,
                old_passphrase: old_pass,
                new_passphrase: new_pass,
            };
            match send_request(&socket_path, request).await? {
                DaemonResponse::Success { message } => {
                    println!("✓ {message}");
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::Status => {
            let request = DaemonRequest::Status;
            match send_request(&socket_path, request).await? {
                DaemonResponse::Status {
                    version,
                    uptime_sec,
                    pools_imported,
                    auto_detect_enabled,
                    ..
                } => {
                    println!("zfswatchd {version}");
                    println!("Uptime: {uptime_sec}s");
                    println!("Auto-detect: {auto_detect_enabled}");
                    println!("Imported pools: {}", pools_imported.len());
                    for pool in pools_imported {
                        println!("  {} ({})", pool.name, pool.health);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("✗ Error [{code}]: {message}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Commands::DaemonStart => {
            println!("Use 'zfswatchd' to start the daemon directly.");
            println!("On macOS: sudo launchctl load /Library/LaunchDaemons/com.zfswatch.daemon.plist");
            println!("On Linux: sudo systemctl start zfswatchd");
        }

        Commands::Doctor { disk } => {
            let checker = SystemPrerequisiteChecker::new();
            match checker.check_all(disk.as_deref()) {
                Ok(report) => {
                    report.print();
                    if !report.overall_ok {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Diagnostic failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

async fn send_request(
    socket_path: &PathBuf,
    request: DaemonRequest,
) -> anyhow::Result<DaemonResponse> {
    let mut stream = UnixStream::connect(socket_path).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to connect to daemon at {socket_path:?}: {e}\n\
             Is zfswatchd running?"
        )
    })?;

    let json = serde_json::to_string(&request)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let (reader, _) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: DaemonResponse = serde_json::from_str(&line)?;
    Ok(response)
}

fn prompt_passphrase(prompt: &str) -> anyhow::Result<String> {
    let pass = rpassword::prompt_password(prompt)?;
    Ok(pass)
}

fn prompt_passphrase_opt(prompt: &str) -> anyhow::Result<Option<String>> {
    let pass = rpassword::prompt_password(prompt)?;
    if pass.is_empty() {
        Ok(None)
    } else {
        Ok(Some(pass))
    }
}
