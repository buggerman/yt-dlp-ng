use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub output_dir: PathBuf,
    pub concurrent_downloads: usize,
    pub user_agent: String,
    pub timeout: u64,
    pub retries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("."),
            concurrent_downloads: 1,
            user_agent: format!("yt-dlp-ng/{}", env!("CARGO_PKG_VERSION")),
            timeout: 30,
            retries: 3,
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // TODO: Load from config file
        Ok(Self::default())
    }
}
