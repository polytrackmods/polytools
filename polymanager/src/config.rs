use std::path::Path;

use anyhow::Result;
use facet::Facet;
use tokio::fs;

#[derive(Debug, Facet, Clone)]
pub struct Config {
    pub logging: LoggingConfig,
    pub services: Vec<ServiceConfig>,
    pub presets: Option<Vec<PresetConfig>>,
    pub default_preset: Option<String>,
}

#[derive(Debug, Facet, Clone)]
pub struct LoggingConfig {
    pub log_dir: String,
}

#[derive(Debug, Facet, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub binary: String,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Facet, Clone)]
pub struct PresetConfig {
    pub name: String,
    pub services: Vec<String>,
}

impl Config {
    pub async fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .unwrap_or_else(|_| String::from(include_str!("../default.toml")));
        Ok(facet_toml::from_str(&content).expect("invalid config"))
    }
}
