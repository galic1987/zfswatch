use std::fmt;
use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Config(String),
    Protocol(String),
    Zfs(String),
    Platform(String),
    KeyVault(String),
    NotFound(String),
    PermissionDenied(String),
    InvalidArgument(String),
    CrossPlatform(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Config(msg) => write!(f, "Configuration error: {msg}"),
            Error::Protocol(msg) => write!(f, "Protocol error: {msg}"),
            Error::Zfs(msg) => write!(f, "ZFS error: {msg}"),
            Error::Platform(msg) => write!(f, "Platform error: {msg}"),
            Error::KeyVault(msg) => write!(f, "Key vault error: {msg}"),
            Error::NotFound(msg) => write!(f, "Not found: {msg}"),
            Error::PermissionDenied(msg) => write!(f, "Permission denied: {msg}"),
            Error::InvalidArgument(msg) => write!(f, "Invalid argument: {msg}"),
            Error::CrossPlatform(msg) => write!(f, "Cross-platform compatibility: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::Config(err.to_string())
    }
}

impl From<toml::ser::Error> for Error {
    fn from(err: toml::ser::Error) -> Self {
        Error::Config(err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Protocol(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert!(Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "io fail")).to_string().contains("IO error"));
        assert!(Error::Config("bad".to_string()).to_string().contains("Configuration error"));
        assert!(Error::Protocol("bad".to_string()).to_string().contains("Protocol error"));
        assert!(Error::Zfs("bad".to_string()).to_string().contains("ZFS error"));
        assert!(Error::Platform("bad".to_string()).to_string().contains("Platform error"));
        assert!(Error::KeyVault("bad".to_string()).to_string().contains("Key vault error"));
        assert!(Error::NotFound("x".to_string()).to_string().contains("Not found"));
        assert!(Error::PermissionDenied("x".to_string()).to_string().contains("Permission denied"));
        assert!(Error::InvalidArgument("x".to_string()).to_string().contains("Invalid argument"));
        assert!(Error::CrossPlatform("x".to_string()).to_string().contains("Cross-platform"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn test_error_from_toml_de() {
        let toml_err: toml::de::Error = toml::from_str::<Config>("[invalid").unwrap_err();
        let err: Error = toml_err.into();
        assert!(matches!(err, Error::Config(_)));
    }

    use crate::config::Config;
}
