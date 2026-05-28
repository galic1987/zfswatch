# zfswatch

Cross-platform ZFS USB auto-mount and encryption tool for macOS and Linux.

## Features

- **Auto-detect** USB-connected ZFS disks on insertion
- **Auto-mount** with secure passphrase handling
- **Initialize** new disks with ZFS + AES-256-GCM encryption
- **Cross-platform** pool creation with conservative feature flags
- **Secure memory** — passphrases stored in `mlock`-ed, zeroed memory
- **Optimized** for USB2/USB3/USB4/USB4-80Gbps performance

## Supported Platforms

| Platform | ZFS Implementation | Max USB Speed |
|----------|-------------------|---------------|
| macOS (Intel/Apple Silicon) | OpenZFS on macOS (O3X) | USB4 40Gbps (TB4) |
| Linux (Ubuntu/Debian) | ZFS on Linux | USB4 80Gbps (TB5) |

> **Note**: Apple Silicon M2 Max supports USB4/Thunderbolt 4 at 40 Gbps maximum. For 80 Gbps you need M4 Pro/Max or M3 Ultra, or an AMD Threadripper/Minisforum USB4 Ryzen system.

## Installation

### macOS

```bash
# 1. Install ZFS
brew install --cask openzfs

# 2. Approve the kernel extension in System Settings → Privacy & Security
# 3. Reboot if prompted

# 4. Install zfswatch
brew tap yourname/zfswatch
brew install zfswatch

# 5. Start the daemon
sudo launchctl load /Library/LaunchDaemons/com.zfswatch.daemon.plist
```

### Linux (Ubuntu)

```bash
# 1. Install ZFS
sudo apt update
sudo apt install zfsutils-linux zfs-dkms

# 2. Install zfswatch
sudo dpkg -i zfswatch_0.1.0_amd64.deb

# 3. Start the daemon
sudo systemctl enable --now zfswatchd
```

## Quick Start

### Initialize a new encrypted pool on your USB drive

```bash
# Find your device
zfswatch devices

# Create encrypted pool
sudo zfswatch init mypool /dev/disk5
# Enter passphrase when prompted

# The pool will auto-mount on future insertions (after passphrase entry)
```

### List pools and devices

```bash
zfswatch list        # List all pools
zfswatch devices     # List connected USB storage
zfswatch status      # Show daemon status
```

### Manual operations

```bash
# Import an existing pool
sudo zfswatch import mypool

# Mount (with passphrase prompt for encrypted pools)
sudo zfswatch mount mypool

# Unmount and unload key
sudo zfswatch unmount --unload-key mypool

# Change passphrase
sudo zfswatch change-key mypool

# Export (safe removal)
sudo zfswatch export mypool
```

## Configuration

Edit `/etc/zfswatch/zfswatch.toml` (Linux) or `/usr/local/etc/zfswatch/zfswatch.toml` (macOS):

```toml
[daemon]
auto_detect = true
auto_mount = false  # encrypted pools require passphrase

[encryption]
default_algorithm = "aes-256-gcm"
min_passphrase_length = 12

[performance]
arc_max_mb = 16384
recordsize_kb = 128
disable_atime = true
compression = "zstd"
```

## Security

- Daemon runs as root (required for ZFS operations)
- CLI runs as user and communicates via authenticated Unix socket
- Passphrases are:
  - Prompted securely (hidden input)
  - Stored in `mlock()`-ed memory
  - Zeroed from memory immediately after use
  - Never written to disk or logs
- By default, encrypted pools require manual passphrase entry (no auto-mount without key caching)

## Performance Tuning

| USB Speed | ZFS Recommendations |
|-----------|---------------------|
| USB 2.0 | `sync=standard`, small ARC |
| USB 3.x | `recordsize=128K`, `compression=zstd` |
| USB4 40G | `recordsize=1M`, max ARC = RAM - 4GB |
| USB4 80G | `recordsize=1M`, disable `atime`, direct IO for databases |

## Development

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run daemon manually
sudo cargo run --bin zfswatchd

# Run CLI
 cargo run --bin zfswatch -- list
```

## License

MIT OR Apache-2.0
