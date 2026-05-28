#!/bin/bash
# zfswatch Linux Setup Script (Ubuntu/Debian)
# Run: sudo bash setup-linux.sh

set -euo pipefail

DISK="${DISK:-/dev/nvme0n1}"
POOL_NAME="${POOL_NAME:-wdblack8tb}"

check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo "Run with: sudo bash $0"
        exit 1
    fi
}

install_deps() {
    echo "==> Installing ZFS..."
    apt-get update
    apt-get install -y zfsutils-linux zfs-dkms linux-headers-$(uname -r)
    modprobe zfs || true
    
    if ! command -v zpool &>/dev/null; then
        echo "ZFS installation failed"
        exit 1
    fi
    
    echo "==> ZFS: $(zpool --version | head -1)"
}

build_zfswatch() {
    echo "==> Building zfswatch..."
    if ! command -v cargo &>/dev/null; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    cargo build --release
}

install_bins() {
    echo "==> Installing binaries..."
    cp target/release/zfswatchd /usr/local/bin/
    cp target/release/zfswatch /usr/local/bin/
    chmod 755 /usr/local/bin/zfswatchd /usr/local/bin/zfswatch
}

install_systemd() {
    echo "==> Installing systemd service..."
    mkdir -p /etc/zfswatch /run/zfswatch
    cp packaging/linux/zfswatchd.service /etc/systemd/system/
    cp packaging/zfswatch.toml.example /etc/zfswatch/zfswatch.toml
    systemctl daemon-reload
    systemctl enable zfswatchd
    systemctl start zfswatchd
}

init_pool() {
    echo "==> Initializing ZFS pool on $DISK..."
    echo ""
    echo -n "Enter passphrase: "
    read -rs PASSPHRASE
    echo ""
    
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
    dd if=/dev/zero of="$BENCH/w" bs=1M count=1024 2>&1 | tail -1
    rm -f "$BENCH/w"
    
    echo "Sequential Write 4GB:"
    dd if=/dev/zero of="$BENCH/w" bs=1M count=4096 2>&1 | tail -1
    rm -f "$BENCH/w"
    
    if command -v fio &>/dev/null; then
        echo "fio random read 4K:"
        fio --name=randread --ioengine=libaio --iodepth=32 \
            --rw=randread --bs=4k --direct=1 --size=1G \
            --numjobs=4 --runtime=30 --directory="$BENCH" \
            --group_reporting 2>&1 | grep -E "read:|IOPS|bw="
    fi
    
    rm -rf "$BENCH"
}

check_root
install_deps
build_zfswatch
install_bins
install_systemd
init_pool
benchmark

echo ""
echo "Done! Pool: $POOL_NAME"
echo "zfswatch list     # show pools"
echo "zfswatch status   # daemon status"
