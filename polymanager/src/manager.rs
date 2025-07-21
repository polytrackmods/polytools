use crate::config::Config;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::{Child, Command};

pub struct ServiceManager {
    pub config: Config,
    pub processes: HashMap<String, Child>,
}

impl ServiceManager {
    pub fn new(config: Config) -> Self {
        let mut manager = Self {
            config: config.clone(),
            processes: HashMap::new(),
        };
        if let Some(default_preset_name) = config.default_preset {
            if let Some(default_preset) = config
                .presets
                .clone()
                .unwrap_or_default()
                .iter()
                .find(|preset| preset.name == default_preset_name)
            {
                for service in default_preset.services.clone() {
                    let _ = manager.start_service(&service);
                }
            }
        }
        manager
    }

    pub fn is_service_running(&self, name: &str) -> bool {
        self.processes.contains_key(name)
    }

    pub async fn restart_service(&mut self, name: &str) -> Result<()> {
        if self.is_service_running(name) {
            self.stop_service(name).await?;
            self.start_service(name)?;
            Ok(())
        } else {
            Err(anyhow!("Service not running"))
        }
    }

    pub async fn stop_service(&mut self, name: &str) -> Result<()> {
        if let Some(mut child) = self.processes.remove(name) {
            child.kill().await?;
            let log_path = format!("logs/{name}.log");
            tokio::fs::remove_file(log_path).await?;
        }
        Ok(())
    }

    pub fn start_service(&mut self, name: &str) -> Result<()> {
        if let Some(svc) = self.config.services.iter().find(|s| s.name == name) {
            let mut cmd = Command::new(&svc.binary);
            let args = svc.args.clone().unwrap_or_default();
            let log_path = format!("logs/{}.log", svc.name);
            let log_file = std::fs::File::create(log_path)?;
            cmd.args(args)
                .stdout(Stdio::from(log_file.try_clone()?))
                .stderr(Stdio::from(log_file));

            let child = cmd.spawn()?;
            self.processes.insert(svc.name.clone(), child);
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        for (_, mut child) in self.processes.drain() {
            let _ = child.kill().await;
        }
    }
}
