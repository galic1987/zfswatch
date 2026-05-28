# zfswatch — Agent Notes

## Project Structure

This is a Rust workspace with multiple crates:

- `zfswatch-core` — Shared types, config (TOML), IPC protocol (JSON over Unix socket), logging
- `zfswatch-zfs` — Wrappers around `zpool` and `zfs` CLI commands
- `zfswatch-platform` — Platform-specific USB detection (macOS IOKit, Linux udev)
- `zfswatch-keys` — Secure passphrase memory management (`mlock`, zeroize)
- `zfswatchd` — The daemon binary
- `zfswatch` — The CLI binary

## Architecture

- **Daemon (`zfswatchd`)** runs as root, listens on Unix socket, monitors USB
- **CLI (`zfswatch`)** runs as user, connects to daemon via Unix socket
- Communication: JSON newline-delimited messages over Unix domain socket
- USB detection is platform-native (not libusb — libusb doesn't do storage/mounts)

## Platform Differences

| Aspect | macOS | Linux |
|--------|-------|-------|
| USB API | IOKit + DiskArbitration | udev/netlink |
| Daemon | launchd (foreground) | systemd |
| Socket | `/var/run/zfswatch.sock` | `/run/zfswatch/zfswatch.sock` |
| Config | `/usr/local/etc/zfswatch/` | `/etc/zfswatch/` |
| ZFS | OpenZFS kext (O3X) | zfsutils-linux |

## Security Critical Code

- `zfswatch-keys/src/memory.rs` — `SecureString` with `mlock` + `zeroize`
- `zfswatchd/src/main.rs` — Passphrase handling in IPC
- Never log passphrases or key material
- Unix socket peer credential verification (TODO)

## ZFS Cross-Platform Compatibility

When creating pools with `--cross-platform` (default):
- Only conservative feature flags enabled
- Risky features (block_cloning_endian, fast_dedup, raidz_expansion, longname) explicitly disabled
- This ensures pools created on Linux can be imported on macOS and vice versa

## Testing

- Unit tests: mock ZFS command outputs
- Linux integration: loop devices (`losetup`)
- macOS integration: disk images (`hdiutil`)
- Hardware tests require actual USB ZFS pools

## Build

```bash
cargo check          # Quick validation
cargo build          # Debug build
cargo build --release # Optimized build
cargo test           # Run tests
```

## Important Notes

- macOS requires SIP considerations for O3X kext
- Apple Silicon has known ZFS RAIDZ encryption panics — use single-disk or mirror
- M2 Max maxes at USB4 40Gbps, not 80Gbps
- Linux DKMS can break on new kernels — recommend LTS
