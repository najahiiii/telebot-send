use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[allow(dead_code)]
pub const URL: &str = "https://github.com/najahiiii/telebot-send";
pub const VERSION_SUMMARY: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nRust: ",
    env!("SENDTG_CLI_RUSTC_VERSION"),
    "\nOS/Arch: ",
    env!("SENDTG_CLI_TARGET_OS"),
    "/",
    env!("SENDTG_CLI_TARGET_ARCH"),
    "\nCommit: ",
    env!("SENDTG_CLI_GIT_COMMIT"),
    "\nBuilt: ",
    env!("SENDTG_CLI_BUILD_TIME")
);

pub const CONFIG_DIR: &str = ".config/sendtg";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    pub api_url: Option<String>,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

impl FileConfig {
    pub fn has_required_fields(&self) -> bool {
        self.api_url
            .as_ref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
            && self
                .bot_token
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            && self
                .chat_id
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
    }
}

pub fn config_file_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(CONFIG_DIR).join(CONFIG_FILE))
}

pub fn load_config() -> Result<Option<FileConfig>> {
    let path = config_file_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config: FileConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML from {}", path.display()))?;
    Ok(Some(config))
}

pub fn write_config(config: &FileConfig) -> Result<PathBuf> {
    let mut to_write = config.clone();

    if let Some(api_url) = to_write.api_url.as_ref() {
        if api_url.trim().is_empty() {
            to_write.api_url = None;
        }
    }
    if let Some(bot_token) = to_write.bot_token.as_ref() {
        if bot_token.trim().is_empty() {
            to_write.bot_token = None;
        }
    }
    if let Some(chat_id) = to_write.chat_id.as_ref() {
        if chat_id.trim().is_empty() {
            to_write.chat_id = None;
        }
    }

    let path = config_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let serialized =
        toml::to_string_pretty(&to_write).context("Failed to serialize config to TOML")?;
    std::fs::write(&path, serialized)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}
