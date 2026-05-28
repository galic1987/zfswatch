#!/bin/bash
# zfswatch macOS Setup Script
# Run: sudo bash setup-macos.sh

set -euo pipefail

DISK="${DISK:-/dev/disk5}"
POOL_NAME="${POOL_NAME:-wdblack8tb}"

check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo "Run with: sudo bash $0"
        exit 1
    fi
}

install_deps() {
    echo "==> Installing OpenZFS..."
    if ! command -v zpool &>/dev/null; then
        if ! command -v brew &>/dev/null; then
            echo "Homebrew required. Install from https://brew.sh"
            exit 1
        fi
        sudo -u "$SUDO_USER" brew install --cask openzfs
    fi
    
    if ! kextstat | grep -q zfs; then
        echo "Approve ZFS kext in: System Settings → Privacy & Security"
        echo "Then re-run this script."
        exit 1
    fi
    
    echo "==> ZFS: $(zpool --version | head -1)"
}

build_zfswatch() {
    echo "==> Building zfswatch..."
    if ! command -v cargo &>/dev/null; then
        echo "Rust required. Install from https://rustup.rs"
        exit 1
    fi
    cargo build --release
}

install_bins() {
    echo "==> Installing binaries..."
    cp target/release/zfswatchd /usr/local/bin/
    cp target/release/zfswatch /usr/local/bin/
    chmod 755 /usr/local/bin/zfswatchd /usr/local/bin/zfswatch
}

install_launchd() {
    echo "==> Installing launchd service..."
    mkdir -p /usr/local/etc/zfswatch
    cp packaging/macos/com.zfswatch.daemon.plist /Library/LaunchDaemons/
    chmod 644 /Library/LaunchDaemons/com.zfswatch.daemon.plist
    launchctl load /Library/LaunchDaemons/com.zfswatch.daemon.plist 2>/dev/null || true
}

init_pool() {
    echo "==> Initializing ZFS pool on $DISK..."
    echo ""
    echo -n "Enter passphrase: "
    read -rs PASSPHRASE
    echo ""
    
    diskutil unmountDisk force "$DISK" 2>/dev/null || true
    
    echo "$PASSPHRASE" | zpool create -f \
        -O encryption=on -O encryption=aes-256-gcm \
        -O keyformat=passphrase -O keylocation=prompt \
        -O compression=zstd -O recordsize=128K -O atime=off \
        -o feature@block_cloning_endian=disabled \
        -o feature@fast_dedup=disabled \
        -o feature@raidz_expansion=disabled \
        -o feature@longname=disabled \
        "$POOL_NAME" "$DISK"
    
    zpool list "$POOL_NAME"
}

benchmark() {
    echo "==> Running benchmarks..."
    BENCH="/tmp/zfs_bench"
    mkdir -p "$BENCH"
    
    echo "Sequential Write 1GB:"
    dd if=/dev/zero of="$BENCH/w" bs=1m count=1024 2>&1 | tail -1
    rm -f "$BENCH/w"
    
    echo "Sequential Write 4GB:"
    dd if=/dev/zero of="$BENCH/w" bs=1m count=4096 2>&1 | tail -1
    rm -f "$BENCH/w"
    
    rm -rf "$BENCH"
}

check_root
install_deps
build_zfswatch
install_bins
install_launchd
init_pool
benchmark

echo ""
echo "Done! Pool: $POOL_NAME"
echo "zfswatch list     # show pools"
echo "zfswatch status   # daemon status"
