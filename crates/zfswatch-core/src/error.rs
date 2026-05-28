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
