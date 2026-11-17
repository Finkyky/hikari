//! Layered TOML configuration and provider profiles.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

/// Resolved configuration after layering defaults, the config file, and CLI overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub home: PathBuf,
    pub provider: String,
    pub model: Option<String>,
    pub providers: BTreeMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "openai_chat")]
    OpenAiChat,
}

/// CLI-level overrides applied last during layering.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    provider: Option<String>,
    model: Option<String>,
    providers: Option<BTreeMap<String, ProviderConfig>>,
}

impl<'de> Deserialize<'de> for ProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            kind: ProviderKind,
            base_url: String,
            api_key_env: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            kind: raw.kind,
            base_url: raw.base_url,
            api_key_env: raw.api_key_env,
        })
    }
}

fn builtin_providers() -> BTreeMap<String, ProviderConfig> {
    BTreeMap::from([(
        "openai".to_string(),
        ProviderConfig {
            kind: ProviderKind::OpenAiChat,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_env: Some("OPENAI_API_KEY".to_string()),
        },
    )])
}

fn default_home() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hikari")
}

/// Load `$HIKARI_HOME/config.toml` (default `~/.hikari`) with CLI overrides applied.
/// A missing file is not an error; nothing is created on disk.
pub fn load(overrides: Overrides) -> Result<Config, ConfigError> {
    let home = std::env::var_os("HIKARI_HOME").map_or_else(default_home, PathBuf::from);
    let path = home.join("config.toml");
    let file = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    load_from(home, file.as_deref(), overrides)
}

/// The testable seam: resolves a config from explicit inputs, no env or file access.
fn load_from(
    home: PathBuf,
    file: Option<&str>,
    overrides: Overrides,
) -> Result<Config, ConfigError> {
    let parsed: FileConfig = match file {
        Some(text) => toml::from_str(text)?,
        None => FileConfig {
            provider: None,
            model: None,
            providers: None,
        },
    };
    let mut providers = builtin_providers();
    if let Some(tables) = parsed.providers {
        for (name, provider) in tables {
            providers.insert(name, provider);
        }
    }
    let provider = overrides
        .provider
        .or(parsed.provider)
        .unwrap_or_else(|| "openai".to_string());
    let model = overrides.model.or(parsed.model);
    Ok(Config {
        home,
        provider,
        model,
        providers,
    })
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
