pub mod traits;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod linux;

pub use traits::{PlatformBackend, UsbEvent};

/// Create the platform-specific backend
pub fn create_backend() -> Box<dyn PlatformBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsBackend::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxBackend::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        compile_error!("zfswatch only supports macOS and Linux")
    }
}
