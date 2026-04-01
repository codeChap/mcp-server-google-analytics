use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
}

#[derive(Debug, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub credentials: String,
}

/// Config directory: ~/.config/mcp-server-google-analytics/
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config")
        })
        .join("mcp-server-google-analytics")
}

/// Load config.toml if it exists. Returns None if not found.
pub fn load_config() -> Result<Option<Config>> {
    let path = config_dir().join("config.toml");
    if !path.exists() {
        return Ok(None);
    }
    info!("loading config from {}", path.display());
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    if config.accounts.is_empty() {
        bail!("config.toml exists but has no [[accounts]] entries");
    }

    Ok(Some(config))
}

/// Resolve a credentials path from config. Relative paths resolve against config_dir().
pub fn resolve_credentials_path(credentials: &str) -> PathBuf {
    let p = Path::new(credentials);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        config_dir().join(credentials)
    }
}
