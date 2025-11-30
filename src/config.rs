use std::fs;

use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub slack_webhook_url: String,
    pub systemd_unit: String,
    pub alerts: Vec<AlertRule>,
    pub heartbeats: Vec<HeartbeatRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub pattern: String,
    pub prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRule {
    pub pattern: String,
    pub prefix: String,
    pub tolerance: u64,
}

const DEFAULT_CONFIGS: [&str; 2] = ["config.toml", "/etc/journal-alerts/config.toml"];

impl Config {
    pub fn load(path: Option<String>) -> Result<Self> {
        let path = match path {
            Some(p) => p,
            None => DEFAULT_CONFIGS
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("No config file found"))?,
        };

        info!("Loading config from: {path}");

        let data = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path))?;

        let config: Config =
            toml::from_str(&data).with_context(|| "Invalid TOML in config file")?;

        Ok(config)
    }
}
