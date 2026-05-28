#!/bin/bash
# zfswatch macOS Setup Script
# Run: sudo bash scripts/setup-macos.sh
#
# This script:
#   - Installs OpenZFS (if missing)
#   - Checks kernel extension status
#   - Builds and installs zfswatch binaries
#   - Creates an encrypted ZFS pool on the target disk
#   - Runs dd benchmarks

set -euo pipefail

DISK="${DISK:-/dev/disk5}"
POOL_NAME="${POOL_NAME:-wdblack8tb}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

check_root() {
    if [ "${EUID:-$(id -u)}" -ne 0 ]; then
        echo -e "${RED}Error: Must run as root${NC}"
        echo "  sudo bash $0"
        exit 1
    fi
}

install_deps() {
    echo -e "${GREEN}==> Checking OpenZFS installation...${NC}"

    if ! command -v brew &>/dev/null; then
        echo -e "${RED}Error: Homebrew not found.${NC}"
        echo "  Install from: https://brew.sh"
        exit 1
    fi

    if ! command -v zpool &>/dev/null; then
        echo -e "${YELLOW}==> Installing OpenZFS via Homebrew...${NC}"
        sudo -u "${SUDO_USER:-$USER}" brew install --cask openzfs
        echo ""
        echo -e "${YELLOW}⚠️  IMPORTANT: OpenZFS kernel extension installed but NOT YET LOADED${NC}"
        echo ""
        echo "You MUST complete these steps before continuing:"
        echo "  1. Open System Settings → Privacy & Security"
        echo "  2. Scroll to 'Security' section"
        echo "  3. Click 'Allow' next to 'OpenZFS'"
        echo "  4. REBOOT your Mac"
        echo "  5. Re-run this script: sudo bash $0"
        echo ""
        exit 1
    fi

    if ! kextstat -l 2>/dev/null | grep -qi zfs; then
        echo -e "${RED}Error: ZFS kernel extension is not loaded.${NC}"
        echo ""
        echo "The OpenZFS cask is installed but the kernel extension needs approval."
        echo ""
        echo "Steps:"
        echo "  1. Open System Settings → Privacy & Security"
        echo "  2. Look for a message about 'OpenZFS' being blocked"
        echo "  3. Click 'Allow'"
        echo "  4. REBOOT"
        echo "  5. Re-run: sudo bash $0"
        echo ""
        exit 1
    fi

    echo -e "${GREEN}✓ ZFS ready: $(zpool --version | head -1)${NC}"
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

install_launchd() {
    echo -e "${GREEN}==> Installing launchd service...${NC}"
    mkdir -p /usr/local/etc/zfswatch
    if [ -f packaging/macos/com.zfswatch.daemon.plist ]; then
        cp packaging/macos/com.zfswatch.daemon.plist /Library/LaunchDaemons/
        chmod 644 /Library/LaunchDaemons/com.zfswatch.daemon.plist
        launchctl load /Library/LaunchDaemons/com.zfswatch.daemon.plist 2>/dev/null || true
        echo -e "${GREEN}✓ launchd service installed${NC}"
    else
        echo -e "${YELLOW}⚠ launchd plist not found, skipping${NC}"
    fi
}

init_pool() {
    echo -e "${GREEN}==> Initializing ZFS pool on ${DISK}...${NC}"
    echo ""

    # Check if disk exists
    if [ ! -e "$DISK" ]; then
        echo -e "${RED}Error: ${DISK} does not exist${NC}"
        echo "Available disks:"
        diskutil list | grep -E '^/dev/' || true
        exit 1
    fi

    # Warn about data destruction
    echo -e "${YELLOW}⚠️  WARNING: This will DESTROY ALL DATA on ${DISK}${NC}"
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

    # Unmount if mounted
    diskutil unmountDisk force "$DISK" 2>/dev/null || true

    # Create pool with cross-platform safe features
    echo "$PASSPHRASE" | zpool create -f \
        -O encryption=on -O encryption=aes-256-gcm \
        -O keyformat=passphrase -O keylocation=prompt \
        -O compression=zstd -O recordsize=128K -O atime=off \
        -o feature@block_cloning_endian=disabled \
        -o feature@fast_dedup=disabled \
        -o feature@raidz_expansion=disabled \
        -o feature@longname=disabled \
        "$POOL_NAME" "$DISK"

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

    # Create a temp dataset for benchmarking
    BENCH_DS="${POOL_NAME}/benchmark"
    zfs create -o compression=off "$BENCH_DS"
    BENCH_MOUNT=$(zfs get -H -o value mountpoint "$BENCH_DS")

    echo ""
    echo "Sequential Write 1GB (uncompressed dataset):"
    dd if=/dev/zero of="${BENCH_MOUNT}/w1g" bs=1m count=1024 2>&1 | tail -1
    rm -f "${BENCH_MOUNT}/w1g"
    sync

    echo ""
    echo "Sequential Write 4GB (uncompressed dataset):"
    dd if=/dev/zero of="${BENCH_MOUNT}/w4g" bs=1m count=4096 2>&1 | tail -1
    rm -f "${BENCH_MOUNT}/w4g"
    sync

    echo ""
    echo "Sequential Read 4GB (cached — first read):"
    dd if="${BENCH_MOUNT}/w4g" of=/dev/null bs=1m 2>&1 | tail -1 || true

    # Cleanup benchmark dataset
    zfs destroy "$BENCH_DS"

    echo ""
    echo -e "${GREEN}==> Pool status:${NC}"
    zpool status "$POOL_NAME"
}

# Main
 check_root
 install_deps
 build_zfswatch
 install_bins
 install_launchd
 init_pool
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
