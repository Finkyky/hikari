#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use crate::{ConfigError, Overrides, ProviderKind, load_from};

fn home() -> PathBuf {
    PathBuf::from("/hikari-home")
}

fn no_overrides() -> Overrides {
    Overrides::default()
}

#[test]
fn missing_file_uses_defaults() {
    let config = load_from(home(), None, no_overrides()).unwrap();
    assert_eq!(config.provider, "openai");
    assert_eq!(config.model, None);
    let openai = config.providers.get("openai").unwrap();
    assert_eq!(openai.kind, ProviderKind::OpenAiChat);
    assert_eq!(openai.base_url, "https://api.openai.com/v1");
    assert_eq!(openai.api_key_env.as_deref(), Some("OPENAI_API_KEY"));
}

#[test]
fn file_sets_provider_and_model() {
    let file = r#"
provider = "local"
model = "qwen3"

[providers.local]
kind = "openai_chat"
base_url = "http://localhost:11434/v1"
"#;
    let config = load_from(home(), Some(file), no_overrides()).unwrap();
    assert_eq!(config.provider, "local");
    assert_eq!(config.model.as_deref(), Some("qwen3"));
    let local = config.providers.get("local").unwrap();
    assert_eq!(local.api_key_env, None);
    // Built-in provider survives alongside custom ones.
    assert!(config.providers.contains_key("openai"));
}

#[test]
fn file_replaces_builtin_provider() {
    let file = r#"
model = "gpt-5"

[providers.openai]
kind = "openai_chat"
base_url = "https://proxy.example.com/v1"
api_key_env = "PROXY_KEY"
"#;
    let config = load_from(home(), Some(file), no_overrides()).unwrap();
    let openai = config.providers.get("openai").unwrap();
    assert_eq!(openai.base_url, "https://proxy.example.com/v1");
    assert_eq!(openai.api_key_env.as_deref(), Some("PROXY_KEY"));
}

#[test]
fn overrides_beat_file() {
    let file = "provider = \"local\"\nmodel = \"a\"";
    let overrides = Overrides {
        provider: Some("other".into()),
        model: Some("b".into()),
    };
    let config = load_from(home(), Some(file), overrides).unwrap();
    assert_eq!(config.provider, "other");
    assert_eq!(config.model.as_deref(), Some("b"));
}

#[test]
fn unknown_key_is_an_error_naming_it() {
    let err = load_from(home(), Some("nonsense = 1"), no_overrides()).unwrap_err();
    let ConfigError::Parse(e) = err else {
        panic!("expected parse error")
    };
    assert!(e.to_string().contains("nonsense"));
}

#[test]
fn invalid_toml_is_an_error() {
    let err = load_from(home(), Some("provider = ["), no_overrides()).unwrap_err();
    assert!(matches!(err, ConfigError::Parse(_)));
}
