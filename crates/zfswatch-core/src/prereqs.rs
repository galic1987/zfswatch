//! System prerequisite diagnostics for zfswatch.
//!
//! Checks ZFS installation, kernel module status, privileges, disk availability,
//! and USB speeds — producing actionable reports.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::Result;

/// Result of a single prerequisite check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckStatus {
    Ok,
    Warning(String),
    Error(String),
}

/// A single diagnostic check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrerequisiteCheck {
    pub name: String,
    pub description: String,
    pub status: CheckStatus,
    pub fix_hint: Option<String>,
}

/// Complete diagnostic report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticReport {
    pub platform: String,
    pub checks: Vec<PrerequisiteCheck>,
    pub overall_ok: bool,
}

impl DiagnosticReport {
    pub fn new() -> Self {
        Self {
            platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            checks: Vec::new(),
            overall_ok: true,
        }
    }

    pub fn add(&mut self, check: PrerequisiteCheck) {
        if matches!(check.status, CheckStatus::Error(_)) {
            self.overall_ok = false;
        }
        self.checks.push(check);
    }

    /// Returns only checks that need attention
    pub fn issues(&self) -> Vec<&PrerequisiteCheck> {
        self.checks
            .iter()
            .filter(|c| !matches!(c.status, CheckStatus::Ok))
            .collect()
    }

    /// Pretty-print the report
    pub fn print(&self) {
        println!("zfswatch System Diagnostic Report");
        println!("==================================");
        println!("Platform: {}", self.platform);
        println!();

        let (ok_count, warn_count, err_count) = self.checks.iter().fold(
            (0, 0, 0),
            |(o, w, e), c| match c.status {
                CheckStatus::Ok => (o + 1, w, e),
                CheckStatus::Warning(_) => (o, w + 1, e),
                CheckStatus::Error(_) => (o, w, e + 1),
            },
        );

        for check in &self.checks {
            let icon = match &check.status {
                CheckStatus::Ok => "✓",
                CheckStatus::Warning(_) => "⚠",
                CheckStatus::Error(_) => "✗",
            };
            println!("{} {}", icon, check.name);
            match &check.status {
                CheckStatus::Ok => {}
                CheckStatus::Warning(msg) => println!("  Warning: {}", msg),
                CheckStatus::Error(msg) => println!("  Error: {}", msg),
            }
            if let Some(hint) = &check.fix_hint {
                println!("  Fix: {}", hint);
            }
            println!();
        }

        println!(
            "Summary: {} passed, {} warnings, {} errors",
            ok_count, warn_count, err_count
        );
        if self.overall_ok && err_count == 0 {
            println!("Status: Ready to use zfswatch!");
        } else if err_count == 0 {
            println!("Status: Usable with warnings");
        } else {
            println!("Status: BLOCKED — fix errors above before using zfswatch");
        }
    }
}

/// Platform-agnostic prerequisite checker trait
pub trait PrerequisiteChecker: Send + Sync {
    /// Run all diagnostic checks and return a report
    fn check_all(&self, target_disk: Option<&Path>) -> Result<DiagnosticReport>;

    /// Check if ZFS userland tools are installed
    fn check_zfs_tools(&self) -> Result<PrerequisiteCheck>;

    /// Check if ZFS kernel module/extension is loaded
    fn check_kernel_module(&self) -> Result<PrerequisiteCheck>;

    /// Check if running with required privileges
    fn check_privileges(&self) -> Result<PrerequisiteCheck>;

    /// Check target disk availability
    fn check_disk(&self, disk: &Path) -> Result<PrerequisiteCheck>;

    /// Check ZFS version and cross-platform compatibility
    fn check_zfs_version(&self) -> Result<PrerequisiteCheck>;
}

/// Real implementation that runs actual system commands
pub struct SystemPrerequisiteChecker;

impl SystemPrerequisiteChecker {
    pub fn new() -> Self {
        Self
    }

    #[cfg(target_os = "macos")]
    fn kext_loaded(&self) -> bool {
        match std::process::Command::new("kextstat")
            .arg("-l")
            .output()
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.to_lowercase().contains("zfs")),
            Err(_) => false,
        }
    }

    #[cfg(target_os = "linux")]
    fn module_loaded(&self) -> bool {
        match std::process::Command::new("lsmod").output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.starts_with("zfs ")),
            Err(_) => false,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn kext_loaded(&self) -> bool {
        false
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn module_loaded(&self) -> bool {
        false
    }

    fn zfs_version_output(&self) -> Option<String> {
        match std::process::Command::new("zpool")
            .arg("--version")
            .output()
        {
            Ok(output) if output.status.success() => {
                let out = String::from_utf8_lossy(&output.stdout);
                out.lines().next().map(|s| s.trim().to_string())
            }
            _ => None,
        }
    }
}

impl PrerequisiteChecker for SystemPrerequisiteChecker {
    fn check_all(&self, target_disk: Option<&Path>) -> Result<DiagnosticReport> {
        let mut report = DiagnosticReport::new();

        report.add(self.check_privileges()?);
        report.add(self.check_zfs_tools()?);
        report.add(self.check_kernel_module()?);
        report.add(self.check_zfs_version()?);

        if let Some(disk) = target_disk {
            report.add(self.check_disk(disk)?);
        }

        Ok(report)
    }

    fn check_zfs_tools(&self) -> Result<PrerequisiteCheck> {
        match std::process::Command::new("zpool")
            .arg("--version")
            .output()
        {
            Ok(output) if output.status.success() => Ok(PrerequisiteCheck {
                name: "ZFS Tools".into(),
                description: "ZFS userland CLI tools (zpool, zfs)".into(),
                status: CheckStatus::Ok,
                fix_hint: None,
            }),
            Ok(_) => Ok(PrerequisiteCheck {
                name: "ZFS Tools".into(),
                description: "ZFS userland CLI tools (zpool, zfs)".into(),
                status: CheckStatus::Error("zpool command found but returned an error".into()),
                fix_hint: Some(platform_install_hint()),
            }),
            Err(e) => Ok(PrerequisiteCheck {
                name: "ZFS Tools".into(),
                description: "ZFS userland CLI tools (zpool, zfs)".into(),
                status: CheckStatus::Error(format!("zpool not found: {e}")),
                fix_hint: Some(platform_install_hint()),
            }),
        }
    }

    fn check_kernel_module(&self) -> Result<PrerequisiteCheck> {
        #[cfg(target_os = "macos")]
        {
            if self.kext_loaded() {
                Ok(PrerequisiteCheck {
                    name: "ZFS Kernel Extension".into(),
                    description: "OpenZFS kernel extension loaded".into(),
                    status: CheckStatus::Ok,
                    fix_hint: None,
                })
            } else {
                Ok(PrerequisiteCheck {
                    name: "ZFS Kernel Extension".into(),
                    description: "OpenZFS kernel extension loaded".into(),
                    status: CheckStatus::Error(
                        "ZFS kernel extension not loaded".into(),
                    ),
                    fix_hint: Some(
                        "macOS: Install 'brew install --cask openzfs', then approve in \
                         System Settings → Privacy & Security, then reboot."
                            .into(),
                    ),
                })
            }
        }

        #[cfg(target_os = "linux")]
        {
            if self.module_loaded() {
                Ok(PrerequisiteCheck {
                    name: "ZFS Kernel Module".into(),
                    description: "ZFS kernel modules (zfs, spl) loaded".into(),
                    status: CheckStatus::Ok,
                    fix_hint: None,
                })
            } else {
                Ok(PrerequisiteCheck {
                    name: "ZFS Kernel Module".into(),
                    description: "ZFS kernel modules (zfs, spl) loaded".into(),
                    status: CheckStatus::Error(
                        "ZFS kernel module not loaded".into(),
                    ),
                    fix_hint: Some(
                        "Linux: sudo apt install zfs-dkms zfsutils-linux && sudo modprobe zfs"
                            .into(),
                    ),
                })
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Ok(PrerequisiteCheck {
                name: "ZFS Kernel Support".into(),
                description: "ZFS kernel module/extension".into(),
                status: CheckStatus::Error("Unsupported platform".into()),
                fix_hint: Some("zfswatch only supports macOS and Linux".into()),
            })
        }
    }

    fn check_privileges(&self) -> Result<PrerequisiteCheck> {
        let uid = unsafe { libc::geteuid() };
        if uid == 0 {
            Ok(PrerequisiteCheck {
                name: "Root Privileges".into(),
                description: "Running with root privileges (required for ZFS)".into(),
                status: CheckStatus::Ok,
                fix_hint: None,
            })
        } else {
            Ok(PrerequisiteCheck {
                name: "Root Privileges".into(),
                description: "Running with root privileges (required for ZFS)".into(),
                status: CheckStatus::Warning(
                    format!("Running as UID {uid} — ZFS operations require root"),
                ),
                fix_hint: Some("Run with: sudo zfswatch <command>".into()),
            })
        }
    }

    fn check_disk(&self, disk: &Path) -> Result<PrerequisiteCheck> {
        let path_str = disk.to_string_lossy();
        match std::fs::metadata(disk) {
            Ok(meta) => {
                if is_special_device(&meta) {
                    Ok(PrerequisiteCheck {
                        name: format!("Disk {}", path_str),
                        description: format!("Target disk {} exists", path_str),
                        status: CheckStatus::Ok,
                        fix_hint: None,
                    })
                } else {
                    Ok(PrerequisiteCheck {
                        name: format!("Disk {}", path_str),
                        description: format!("Target disk {} exists", path_str),
                        status: CheckStatus::Warning(
                            format!("{} exists but is not a block device", path_str),
                        ),
                        fix_hint: Some(
                            "Verify the correct device path (e.g. /dev/disk5 or /dev/sdb)"
                                .into(),
                        ),
                    })
                }
            }
            Err(e) => Ok(PrerequisiteCheck {
                name: format!("Disk {}", path_str),
                description: format!("Target disk {} exists", path_str),
                status: CheckStatus::Error(format!("{} not accessible: {}", path_str, e)),
                fix_hint: Some(
                    "Verify the device path and permissions. On macOS, try /dev/disk5. \
                     On Linux, /dev/sdX or /dev/nvme0n1."
                        .into(),
                ),
            }),
        }
    }

    fn check_zfs_version(&self) -> Result<PrerequisiteCheck> {
        match self.zfs_version_output() {
            Some(version) => {
                debug!("Detected ZFS version: {}", version);
                // Check for cross-platform compatibility hint
                let hint = if version.contains("2.3") || version.contains("2.2") {
                    None
                } else {
                    Some(format!(
                        "Version {} detected. For macOS ↔ Linux pool portability, \
                         use OpenZFS 2.2.x or 2.3.x on both platforms.",
                        version
                    ))
                };

                let status = if hint.is_some() {
                    CheckStatus::Warning(format!("ZFS version {} — verify cross-platform compatibility", version))
                } else {
                    CheckStatus::Ok
                };

                Ok(PrerequisiteCheck {
                    name: "ZFS Version".into(),
                    description: "ZFS version for cross-platform compatibility".into(),
                    status,
                    fix_hint: hint,
                })
            }
            None => Ok(PrerequisiteCheck {
                name: "ZFS Version".into(),
                description: "ZFS version for cross-platform compatibility".into(),
                status: CheckStatus::Error("Unable to determine ZFS version".into()),
                fix_hint: Some(platform_install_hint()),
            }),
        }
    }
}

/// Returns a platform-specific installation hint
fn platform_install_hint() -> String {
    #[cfg(target_os = "macos")]
    {
        "brew install --cask openzfs".into()
    }
    #[cfg(target_os = "linux")]
    {
        "sudo apt install zfsutils-linux zfs-dkms  (or equivalent for your distro)".into()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        "Install OpenZFS for your platform".into()
    }
}

/// Check if a file is a special device (block or char device)
#[cfg(unix)]
fn is_special_device(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    meta.file_type().is_block_device() || meta.file_type().is_char_device()
}

#[cfg(not(unix))]
fn is_special_device(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_report_new() {
        let report = DiagnosticReport::new();
        assert!(report.checks.is_empty());
        assert!(report.overall_ok);
    }

    #[test]
    fn test_diagnostic_report_add_ok() {
        let mut report = DiagnosticReport::new();
        report.add(PrerequisiteCheck {
            name: "Test".into(),
            description: "Test check".into(),
            status: CheckStatus::Ok,
            fix_hint: None,
        });
        assert!(report.overall_ok);
        assert_eq!(report.checks.len(), 1);
    }

    #[test]
    fn test_diagnostic_report_add_error() {
        let mut report = DiagnosticReport::new();
        report.add(PrerequisiteCheck {
            name: "Test".into(),
            description: "Test check".into(),
            status: CheckStatus::Ok,
            fix_hint: None,
        });
        report.add(PrerequisiteCheck {
            name: "Test2".into(),
            description: "Test check 2".into(),
            status: CheckStatus::Error("fail".into()),
            fix_hint: Some("fix it".into()),
        });
        assert!(!report.overall_ok);
        assert_eq!(report.issues().len(), 1);
    }

    #[test]
    fn test_diagnostic_report_add_warning_only() {
        let mut report = DiagnosticReport::new();
        report.add(PrerequisiteCheck {
            name: "Test".into(),
            description: "Test check".into(),
            status: CheckStatus::Warning("careful".into()),
            fix_hint: Some("be careful".into()),
        });
        assert!(report.overall_ok); // warnings don't block
        assert_eq!(report.issues().len(), 1);
    }

    #[test]
    fn test_check_status_serde() {
        let ok = CheckStatus::Ok;
        let json = serde_json::to_string(&ok).unwrap();
        assert_eq!(json, "\"Ok\"");

        let err = CheckStatus::Error("boom".into());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("boom"));

        let warn = CheckStatus::Warning("careful".into());
        let json = serde_json::to_string(&warn).unwrap();
        assert!(json.contains("careful"));
    }

    #[test]
    fn test_platform_install_hint() {
        let hint = platform_install_hint();
        assert!(!hint.is_empty());
        #[cfg(target_os = "macos")]
        assert!(hint.contains("brew"));
        #[cfg(target_os = "linux")]
        assert!(hint.contains("apt") || hint.contains("zfsutils"));
    }

    #[test]
    fn test_diagnostic_report_print_does_not_panic() {
        let mut report = DiagnosticReport::new();
        report.add(PrerequisiteCheck {
            name: "All Good".into(),
            description: "Everything is fine".into(),
            status: CheckStatus::Ok,
            fix_hint: None,
        });
        report.add(PrerequisiteCheck {
            name: "Warn".into(),
            description: "Something to watch".into(),
            status: CheckStatus::Warning("low disk".into()),
            fix_hint: Some("clean up".into()),
        });
        report.add(PrerequisiteCheck {
            name: "Fail".into(),
            description: "Something broken".into(),
            status: CheckStatus::Error("missing".into()),
            fix_hint: Some("install it".into()),
        });
        // Just ensure print() doesn't panic
        report.print();
    }

    #[test]
    fn test_diagnostic_report_empty_print() {
        let report = DiagnosticReport::new();
        report.print();
        assert!(report.overall_ok);
        assert!(report.issues().is_empty());
    }

    // Integration-style tests against the real system state
    #[test]
    fn test_system_checker_privileges() {
        let checker = SystemPrerequisiteChecker::new();
        let check = checker.check_privileges().unwrap();
        assert_eq!(check.name, "Root Privileges");
        // We know the test runner is not root in CI
        #[cfg(not(target_os = "macos"))]
        {
            // On Linux CI we might be root, so just verify the check runs
        }
    }

    #[test]
    fn test_system_checker_disk_exists() {
        let checker = SystemPrerequisiteChecker::new();
        let check = checker.check_disk(Path::new("/tmp")).unwrap();
        assert_eq!(check.name, "Disk /tmp");
        // /tmp exists but is not a block device -> Warning on unix
        #[cfg(unix)]
        assert!(
            matches!(check.status, CheckStatus::Warning(_)),
            "Expected Warning for /tmp (not block device), got {:?}",
            check.status
        );
        #[cfg(not(unix))]
        assert!(matches!(check.status, CheckStatus::Ok));
    }

    #[test]
    fn test_system_checker_disk_missing() {
        let checker = SystemPrerequisiteChecker::new();
        let check = checker.check_disk(Path::new("/dev/nonexistent_disk_12345")).unwrap();
        assert!(matches!(check.status, CheckStatus::Error(_)));
    }

    #[test]
    fn test_system_checker_zfs_tools() {
        let checker = SystemPrerequisiteChecker::new();
        let check = checker.check_zfs_tools().unwrap();
        assert_eq!(check.name, "ZFS Tools");
        // In this test environment zpool is not installed
        assert!(
            matches!(check.status, CheckStatus::Error(_)),
            "Expected Error (zpool not installed), got {:?}",
            check.status
        );
        assert!(check.fix_hint.is_some());
    }

    #[test]
    fn test_system_checker_kernel_module() {
        let checker = SystemPrerequisiteChecker::new();
        let check = checker.check_kernel_module().unwrap();
        // Name is platform-specific
        assert!(
            check.name.contains("Kernel") || check.name.contains("Module") || check.name.contains("Extension")
        );
        // In this test environment ZFS kext/module is not loaded
        assert!(
            matches!(check.status, CheckStatus::Error(_)),
            "Expected Error (module not loaded), got {:?}",
            check.status
        );
        assert!(check.fix_hint.is_some());
    }

    #[test]
    fn test_system_checker_zfs_version() {
        let checker = SystemPrerequisiteChecker::new();
        let check = checker.check_zfs_version().unwrap();
        assert_eq!(check.name, "ZFS Version");
        // zpool is not installed in test env
        assert!(
            matches!(check.status, CheckStatus::Error(_)),
            "Expected Error (version unknown), got {:?}",
            check.status
        );
    }

    #[test]
    fn test_system_checker_check_all() {
        let checker = SystemPrerequisiteChecker::new();
        let report = checker.check_all(Some(Path::new("/dev/disk5"))).unwrap();
        // Should contain checks for disk, privileges, tools, module, version
        assert!(!report.checks.is_empty());
        let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("Disk")));
        assert!(names.iter().any(|n| n.contains("Privileges")));
        assert!(names.iter().any(|n| n.contains("ZFS Tools")));
    }

    #[test]
    fn test_is_special_device() {
        // /dev/null is a char device on unix
        #[cfg(unix)]
        {
            let meta = std::fs::metadata("/dev/null").unwrap();
            assert!(is_special_device(&meta));
        }
        // /tmp is a directory, not special
        let meta = std::fs::metadata("/tmp").unwrap();
        assert!(!is_special_device(&meta));
    }

    #[test]
    fn test_diagnostic_report_print_all_ok() {
        let mut report = DiagnosticReport::new();
        report.add(PrerequisiteCheck {
            name: "A".into(),
            description: "desc".into(),
            status: CheckStatus::Ok,
            fix_hint: None,
        });
        report.add(PrerequisiteCheck {
            name: "B".into(),
            description: "desc".into(),
            status: CheckStatus::Ok,
            fix_hint: None,
        });
        report.print();
        assert!(report.overall_ok);
    }

    #[test]
    fn test_diagnostic_report_print_warnings_only() {
        let mut report = DiagnosticReport::new();
        report.add(PrerequisiteCheck {
            name: "A".into(),
            description: "desc".into(),
            status: CheckStatus::Ok,
            fix_hint: None,
        });
        report.add(PrerequisiteCheck {
            name: "B".into(),
            description: "desc".into(),
            status: CheckStatus::Warning("warn".into()),
            fix_hint: Some("fix".into()),
        });
        report.print();
        assert!(report.overall_ok); // warnings don't block overall_ok
    }

    /// Test with a fake zpool binary to exercise success paths
    #[test]
    fn test_system_checker_with_fake_zpool() {
        use std::io::Write;

        let tmp_dir = tempfile::tempdir().unwrap();
        let zpool_path = tmp_dir.path().join("zpool");
        #[cfg(unix)]
        {
            let mut file = std::fs::File::create(&zpool_path).unwrap();
            file.write_all(b"#!/bin/sh\necho 'zfs-2.3.1'\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&zpool_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&zpool_path, perms).unwrap();
        }

        let original_path = std::env::var_os("PATH");
        let new_path = if let Some(ref p) = original_path {
            let mut paths = std::env::split_paths(p).collect::<Vec<_>>();
            paths.insert(0, tmp_dir.path().to_path_buf());
            std::env::join_paths(paths).unwrap()
        } else {
            std::ffi::OsString::from(tmp_dir.path())
        };
        unsafe { std::env::set_var("PATH", &new_path); }

        let checker = SystemPrerequisiteChecker::new();
        let tools = checker.check_zfs_tools().unwrap();
        assert!(matches!(tools.status, CheckStatus::Ok), "Expected Ok for fake zpool, got {:?}", tools.status);

        let version = checker.check_zfs_version().unwrap();
        assert!(matches!(version.status, CheckStatus::Ok), "Expected Ok for version 2.3.1, got {:?}", version.status);

        // Restore PATH
        unsafe {
            if let Some(p) = original_path {
                std::env::set_var("PATH", p);
            } else {
                std::env::remove_var("PATH");
            }
        }
    }
}
