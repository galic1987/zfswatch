#!/bin/bash
# zfswatch Linux Setup Script
# Run: sudo bash scripts/setup-linux.sh
#
# Auto-detects distro, installs ZFS, builds zfswatch, creates pool, benchmarks.

set -euo pipefail

DISK="${DISK:-}"          # Auto-detect if empty
POOL_NAME="${POOL_NAME:-wdblack8tb}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

check_root() {
    if [ "${EUID:-$(id -u)}" -ne 0 ]; then
        echo -e "${RED}Error: Must run as root${NC}"
        echo "  sudo bash $0"
        exit 1
    fi
}

detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        echo "$ID"
    elif command -v lsb_release &>/dev/null; then
        lsb_release -is | tr '[:upper:]' '[:lower:]'
    else
        echo "unknown"
    fi
}

install_zfs() {
    local distro
    distro=$(detect_distro)
    echo -e "${GREEN}==> Detected distro: ${distro}${NC}"

    if command -v zpool &>/dev/null; then
        echo -e "${GREEN}✓ ZFS already installed: $(zpool --version | head -1)${NC}"
        return 0
    fi

    echo -e "${YELLOW}==> Installing ZFS for ${distro}...${NC}"

    case "$distro" in
        ubuntu|debian)
            apt-get update
            apt-get install -y zfsutils-linux zfs-dkms
            ;;
        fedora)
            dnf install -y zfs
            ;;
        centos|rhel|rocky|almalinux)
            dnf install -y epel-release
            dnf install -y zfs
            ;;
        arch|manjaro)
            pacman -Sy --noconfirm zfs-utils
            ;;
        alpine)
            apk add zfs zfs-lts
            ;;
        *)
            echo -e "${RED}Error: Unsupported distro: ${distro}${NC}"
            echo "Please install ZFS manually for your distribution."
            exit 1
            ;;
    esac

    # Try to load the module
    echo -e "${GREEN}==> Loading ZFS kernel module...${NC}"
    modprobe zfs || true

    if ! command -v zpool &>/dev/null; then
        echo -e "${RED}Error: ZFS installation failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ ZFS installed: $(zpool --version | head -1)${NC}"
}

detect_disk() {
    if [ -n "$DISK" ]; then
        echo "$DISK"
        return 0
    fi

    echo -e "${YELLOW}==> Auto-detecting external NVMe/SATA disk...${NC}"

    # Look for USB/NVMe disks that are not the system disk
    local candidates
    candidates=$(lsblk -d -o NAME,SIZE,TYPE,ROTA,TRAN | awk '
        NR>1 && $3=="disk" && $4==0 {
            # Exclude small disks (< 100GB) and loop devices
            cmd = "blockdev --getsize64 /dev/" $1
            cmd | getline size
            close(cmd)
            if (size > 100000000000) {
                print "/dev/" $1
            }
        }
    ')

    if [ -z "$candidates" ]; then
        echo -e "${RED}Error: No suitable disk detected automatically.${NC}"
        echo "Please specify the disk manually:"
        echo "  sudo DISK=/dev/nvme0n1 bash $0"
        echo ""
        echo "Available disks:"
        lsblk -d -o NAME,SIZE,TYPE,MODEL || true
        exit 1
    fi

    local count
    count=$(echo "$candidates" | wc -l)
    if [ "$count" -eq 1 ]; then
        echo "$candidates"
    else
        echo -e "${YELLOW}Multiple candidate disks found:${NC}"
        echo "$candidates"
        echo ""
        echo "Please specify the target disk:"
        echo "  sudo DISK=/dev/sdX bash $0"
        exit 1
    fi
}

build_zfswatch() {
    echo -e "${GREEN}==> Building zfswatch...${NC}"
    if ! command -v cargo &>/dev/null; then
        echo -e "${RED}Error: Rust not found.${NC}"
        echo "  Install from: https://rustup.rs"
        exit 1
    fi
    cargo build --release
}

install_bins() {
    echo -e "${GREEN}==> Installing binaries...${NC}"
    mkdir -p /usr/local/bin
    cp target/release/zfswatchd /usr/local/bin/
    cp target/release/zfswatch /usr/local/bin/
    chmod 755 /usr/local/bin/zfswatchd /usr/local/bin/zfswatch
    echo -e "${GREEN}✓ Installed to /usr/local/bin/${NC}"
}

install_systemd() {
    echo -e "${GREEN}==> Installing systemd service...${NC}"
    mkdir -p /usr/local/etc/zfswatch
    if [ -f packaging/linux/zfswatchd.service ]; then
        cp packaging/linux/zfswatchd.service /etc/systemd/system/
        systemctl daemon-reload
        systemctl enable zfswatchd.service 2>/dev/null || true
        systemctl start zfswatchd.service 2>/dev/null || true
        echo -e "${GREEN}✓ systemd service installed${NC}"
    else
        echo -e "${YELLOW}⚠ systemd service file not found, skipping${NC}"
    fi
}

init_pool() {
    local target_disk="$1"
    echo -e "${GREEN}==> Initializing ZFS pool on ${target_disk}...${NC}"
    echo ""

    if [ ! -e "$target_disk" ]; then
        echo -e "${RED}Error: ${target_disk} does not exist${NC}"
        exit 1
    fi

    # Warn about data destruction
    echo -e "${YELLOW}⚠️  WARNING: This will DESTROY ALL DATA on ${target_disk}${NC}"
    echo -n "Type 'yes' to continue: "
    read -r CONFIRM
    if [ "$CONFIRM" != "yes" ]; then
        echo "Aborted."
        exit 1
    fi

    echo -n "Enter pool passphrase: "
    read -rs PASSPHRASE
    echo ""
    echo -n "Confirm passphrase: "
    read -rs CONFIRM_PASS
    echo ""

    if [ "$PASSPHRASE" != "$CONFIRM_PASS" ]; then
        echo -e "${RED}Error: Passphrases do not match${NC}"
        exit 1
    fi

    if [ "${#PASSPHRASE}" -lt 8 ]; then
        echo -e "${RED}Error: Passphrase must be at least 8 characters${NC}"
        exit 1
    fi

    # Create pool with cross-platform safe features
    echo "$PASSPHRASE" | zpool create -f \
        -O encryption=on -O encryption=aes-256-gcm \
        -O keyformat=passphrase -O keylocation=prompt \
        -O compression=zstd -O recordsize=128K -O atime=off \
        -o feature@block_cloning_endian=disabled \
        -o feature@fast_dedup=disabled \
        -o feature@raidz_expansion=disabled \
        -o feature@longname=disabled \
        "$POOL_NAME" "$target_disk"

    echo ""
    echo -e "${GREEN}✓ Pool created:${NC}"
    zpool list "$POOL_NAME"
    echo ""
    echo -e "${GREEN}✓ Dataset info:${NC}"
    zfs list "$POOL_NAME"
}

benchmark() {
    echo ""
    echo -e "${GREEN}==> Running benchmarks...${NC}"

    BENCH_DS="${POOL_NAME}/benchmark"
    zfs create -o compression=off "$BENCH_DS"
    BENCH_MOUNT=$(zfs get -H -o value mountpoint "$BENCH_DS")

    echo ""
    echo "Sequential Write 1GB (uncompressed):"
    dd if=/dev/zero of="${BENCH_MOUNT}/w1g" bs=1M count=1024 2>&1 | tail -1
    rm -f "${BENCH_MOUNT}/w1g"
    sync

    echo ""
    echo "Sequential Write 4GB (uncompressed):"
    dd if=/dev/zero of="${BENCH_MOUNT}/w4g" bs=1M count=4096 2>&1 | tail -1
    rm -f "${BENCH_MOUNT}/w4g"
    sync

    echo ""
    echo "Sequential Write 1GB with ZFS compression (zstd):"
    zfs create -o compression=zstd "${POOL_NAME}/bench-compressed"
    CMP_MOUNT=$(zfs get -H -o value mountpoint "${POOL_NAME}/bench-compressed")
    dd if=/dev/zero of="${CMP_MOUNT}/w1g" bs=1M count=1024 2>&1 | tail -1
    rm -f "${CMP_MOUNT}/w1g"
    sync

    # Cleanup
    zfs destroy "$BENCH_DS"
    zfs destroy "${POOL_NAME}/bench-compressed"

    echo ""
    echo -e "${GREEN}==> Pool status:${NC}"
    zpool status "$POOL_NAME"
}

# Main
check_root
install_zfs

TARGET_DISK=$(detect_disk)
echo -e "${GREEN}✓ Target disk: ${TARGET_DISK}${NC}"

build_zfswatch
install_bins
install_systemd
init_pool "$TARGET_DISK"
benchmark

echo ""
echo -e "${GREEN}===============================================${NC}"
echo -e "${GREEN}Done! Pool '${POOL_NAME}' is ready.${NC}"
echo ""
echo "Commands:"
echo "  zfswatch list              # show pools"
echo "  zfswatch status            # daemon status"
echo "  zfswatch doctor            # system diagnostics"
echo "  zpool status ${POOL_NAME}  # pool health"
echo "  zfs list                   # datasets"
echo ""
echo "Mount point: $(zfs get -H -o value mountpoint ${POOL_NAME})"
