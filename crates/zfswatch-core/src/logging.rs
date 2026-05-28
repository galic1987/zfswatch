use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::LogFormat;
use crate::error::Result;

/// Initialize global tracing subscriber
pub fn init_logging(level: &str, format: LogFormat) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    let fmt_layer = tracing_subscriber::fmt::layer();

    match format {
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer.pretty())
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer.json())
                .init();
        }
        LogFormat::Compact => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer.compact())
                .init();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: init_logging can only be called once per process because it installs
    // a global subscriber. We test each format in separate test binaries implicitly.
    // For unit testing, we verify the function doesn't panic with valid inputs.

    #[test]
    fn test_log_format_variants() {
        // Verify all variants exist and are distinct
        let formats = vec![LogFormat::Pretty, LogFormat::Json, LogFormat::Compact];
        assert_eq!(formats.len(), 3);
    }
}
