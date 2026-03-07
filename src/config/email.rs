//! Email (JMAP) configuration.

use crate::config::helpers::{optional_env, parse_optional_env};
use crate::error::ConfigError;
use crate::settings::Settings;

/// Email integration configuration.
#[derive(Debug, Clone, Default)]
pub struct EmailConfig {
    /// Whether email integration is enabled.
    pub enabled: bool,
    /// JMAP server URL (e.g. "https://localhost:8080" for local Stalwart).
    pub jmap_url: Option<String>,
    /// Username for JMAP authentication.
    pub username: Option<String>,
    /// Password or app-specific token for JMAP authentication.
    pub password: Option<String>,
    /// How often to poll for new emails, in seconds (default: 60).
    pub poll_interval_secs: u64,
    /// Maximum emails to fetch per poll (default: 50).
    pub max_fetch: u32,
}

impl EmailConfig {
    pub(crate) fn resolve(_settings: &Settings) -> Result<Self, ConfigError> {
        let enabled = parse_optional_env("EMAIL_ENABLED", false)?;
        let jmap_url = optional_env("EMAIL_JMAP_URL")?;
        let username = optional_env("EMAIL_USERNAME")?;
        let password = optional_env("EMAIL_PASSWORD")?;
        let poll_interval_secs = parse_optional_env("EMAIL_POLL_INTERVAL_SECS", 60)?;
        let max_fetch = parse_optional_env("EMAIL_MAX_FETCH", 50)?;

        if enabled && jmap_url.is_none() {
            return Err(ConfigError::MissingRequired {
                key: "EMAIL_JMAP_URL".to_string(),
                hint: "Set EMAIL_JMAP_URL when EMAIL_ENABLED=true (e.g. https://localhost:8080)"
                    .to_string(),
            });
        }

        Ok(Self {
            enabled,
            jmap_url,
            username,
            password,
            poll_interval_secs,
            max_fetch,
        })
    }
}
