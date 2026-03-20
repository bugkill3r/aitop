use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_refresh")]
    pub refresh: f64,

    #[serde(default = "default_theme")]
    pub theme: String,

    pub weekly_budget: Option<f64>,

    /// Daily budget threshold for desktop notifications
    pub budget: Option<f64>,

    pub claude_data_dir: Option<String>,

    pub admin_api_key: Option<String>,
}

fn default_refresh() -> f64 {
    2.0
}

fn default_theme() -> String {
    "ember".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh: default_refresh(),
            theme: default_theme(),
            weekly_budget: None,
            budget: None,
            claude_data_dir: None,
            admin_api_key: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            let config = Config::default();
            // Create config dir and write default
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            Ok(config)
        }
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("aitop")
            .join("config.toml")
    }

    pub fn claude_projects_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.claude_data_dir {
            let expanded = shellexpand(dir);
            PathBuf::from(expanded)
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude")
                .join("projects")
        }
    }

    pub fn db_path() -> PathBuf {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".local").join("share"));
        let dir = data_dir.join("aitop");
        std::fs::create_dir_all(&dir).ok();
        dir.join("sessions.db")
    }
}

fn shellexpand(s: &str) -> String {
    if s.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return s.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    s.to_string()
}
