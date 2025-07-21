mod config;
mod manager;
mod tui;

use crate::manager::ServiceManager;
use anyhow::Result;
use config::Config;
use std::{env, path::Path};

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());
    let config = Config::load_from_file(Path::new(&config_path)).await?;

    let mut manager = ServiceManager::new(config);

    tui::launch(&mut manager).await?;

    manager.shutdown().await;

    Ok(())
}
