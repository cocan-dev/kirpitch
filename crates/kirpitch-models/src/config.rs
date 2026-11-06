// SPDX-License-Identifier: GPL-3.0-or-later

//! The `kirpitch.toml` configuration: which models exist, which model does
//! which task, and the privacy switch.
//!
//! API keys are never in the file. A model names the environment variable
//! that holds its key (`api_key_env`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// Name of the configuration file looked for in the current folder.
pub const CONFIG_FILE: &str = "kirpitch.toml";

/// How Kirpitch talks to a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    /// Anthropic Messages API.
    Anthropic,
    /// OpenAI chat completions API at OpenAI itself.
    Openai,
    /// Any server that speaks the OpenAI chat API (LM Studio, llama.cpp
    /// server, vLLM, LocalAI, OpenRouter, Mistral, Gemini's compatible
    /// endpoint, Azure OpenAI, ...). Needs `base_url`.
    OpenaiCompatible,
    /// Ollama's native API, local or at ollama.com.
    Ollama,
}

impl Provider {
    /// The address used when the model gives no `base_url`.
    pub fn default_base_url(self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => Some("https://api.anthropic.com"),
            Provider::Openai => Some("https://api.openai.com"),
            Provider::OpenaiCompatible => None,
            Provider::Ollama => Some("http://localhost:11434"),
        }
    }
}

/// One model.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    /// How to talk to it.
    pub provider: Provider,
    /// The model name; `claude-sonnet`, `claude-opus` and `claude-haiku` are
    /// resolved to the current versions.
    pub model: String,
    /// Server address; the provider's usual address when left out.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Environment variable that holds the API key.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Header that carries the key, when it is not the usual one (for example
    /// `api-key` for Azure OpenAI).
    #[serde(default)]
    pub api_key_header: Option<String>,
    /// Whether the model can read images. Unknown when left out (the probe of
    /// `kirpitch models test` finds out).
    #[serde(default)]
    pub vision: Option<bool>,
    /// Ollama only: whether the model may think before it answers. Models
    /// that think can use all of their answer budget on it and answer
    /// nothing; set this to `false` for them. Not sent when left out.
    #[serde(default)]
    pub think: Option<bool>,
    /// Price of one million input tokens, for the cost estimate.
    #[serde(default)]
    pub price_in: Option<f64>,
    /// Price of one million output tokens, for the cost estimate.
    #[serde(default)]
    pub price_out: Option<f64>,
}

/// Which model does which task.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tasks {
    /// Reading pin tables.
    #[serde(default)]
    pub pin_tables: Option<String>,
    /// Reading package drawings and dimension tables.
    #[serde(default)]
    pub drawings: Option<String>,
    /// Reading the part number, maker and description.
    #[serde(default)]
    pub metadata: Option<String>,
}

/// Privacy settings.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Privacy {
    /// Refuse every server that is not on this machine or the local network.
    #[serde(default)]
    pub local_only: bool,
}

/// The whole configuration.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The models by name.
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
    /// The model of each task.
    #[serde(default)]
    pub tasks: Tasks,
    /// Privacy settings.
    #[serde(default)]
    pub privacy: Privacy,
}

/// The kinds of work a model is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// Pin tables.
    PinTables,
    /// Package dimensions.
    Drawings,
    /// Part number, maker, description.
    Metadata,
}

impl Task {
    /// The name used in the configuration.
    pub fn name(self) -> &'static str {
        match self {
            Task::PinTables => "pin_tables",
            Task::Drawings => "drawings",
            Task::Metadata => "metadata",
        }
    }
}

/// Reasons the configuration cannot be used.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    /// The file is not valid TOML or has unknown settings.
    #[error("{0}")]
    Parse(String),
    /// A task names a model that is not defined.
    #[error("task `{task}` uses model `{model}`, which is not defined in [models]")]
    UnknownModel {
        /// The task.
        task: &'static str,
        /// The missing model.
        model: String,
    },
    /// A model needs a `base_url`.
    #[error("model `{0}` needs a base_url")]
    NoBaseUrl(String),
    /// No model is set for a task and there is no fallback.
    #[error("no model is configured for `{0}`")]
    NoModel(&'static str),
    /// `local_only` is on and the server is not local.
    #[error(
        "model `{model}` would send data to {url}, which is not on this machine or the local network, and `local_only` is on"
    )]
    NotLocal {
        /// The model.
        model: String,
        /// The address.
        url: String,
    },
    /// The file could not be read.
    #[error("{0}")]
    Io(String),
}

impl Config {
    /// Reads the configuration from TOML text.
    pub fn from_toml_str(text: &str) -> Result<Config, ConfigError> {
        let config: Config =
            toml::from_str(text).map_err(|e| ConfigError::Parse(e.message().to_string()))?;
        config.check()?;
        Ok(config)
    }

    /// Reads the configuration from a file.
    pub fn from_file(path: &Path) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(format!("{}: {e}", path.display())))?;
        Config::from_toml_str(&text).map_err(|e| match e {
            ConfigError::Parse(m) => ConfigError::Parse(format!("{}: {m}", path.display())),
            other => other,
        })
    }

    /// Looks for `kirpitch.toml` in the current folder. `None` when there is
    /// none, which means no-LLM mode.
    pub fn discover() -> Result<Option<Config>, ConfigError> {
        let path = Path::new(CONFIG_FILE);
        if path.is_file() {
            Config::from_file(path).map(Some)
        } else {
            Ok(None)
        }
    }

    fn check(&self) -> Result<(), ConfigError> {
        for (name, m) in &self.models {
            if m.base_url.is_none() && m.provider.default_base_url().is_none() {
                return Err(ConfigError::NoBaseUrl(name.clone()));
            }
        }
        for (task, model) in [
            (Task::PinTables, &self.tasks.pin_tables),
            (Task::Drawings, &self.tasks.drawings),
            (Task::Metadata, &self.tasks.metadata),
        ] {
            if let Some(model) = model
                && !self.models.contains_key(model)
            {
                return Err(ConfigError::UnknownModel {
                    task: task.name(),
                    model: model.clone(),
                });
            }
        }
        Ok(())
    }

    /// The model of a task. A task without its own model uses the model of
    /// `pin_tables`, then the only model when there is just one.
    pub fn model_for(&self, task: Task) -> Result<(&str, &ModelConfig), ConfigError> {
        let named = match task {
            Task::PinTables => self.tasks.pin_tables.as_ref(),
            Task::Drawings => self.tasks.drawings.as_ref(),
            Task::Metadata => self.tasks.metadata.as_ref(),
        }
        .or(self.tasks.pin_tables.as_ref());
        let name = match named {
            Some(n) => n.as_str(),
            None if self.models.len() == 1 => self.models.keys().next().map_or("", |k| k.as_str()),
            None => return Err(ConfigError::NoModel(task.name())),
        };
        let model = self
            .models
            .get(name)
            .ok_or(ConfigError::NoModel(task.name()))?;
        self.allowed(name, model)?;
        Ok((name, model))
    }

    /// Refuses a non-local server when `local_only` is on.
    pub fn allowed(&self, name: &str, model: &ModelConfig) -> Result<(), ConfigError> {
        let url = base_url(model);
        if self.privacy.local_only && !model_is_local(model) {
            return Err(ConfigError::NotLocal {
                model: name.to_string(),
                url,
            });
        }
        Ok(())
    }

    /// True when the data of `task` stays on this machine or the local network.
    pub fn stays_local(&self, task: Task) -> bool {
        self.model_for(task)
            .map(|(_, m)| model_is_local(m))
            .unwrap_or(true)
    }
}

/// The server address of a model, without a trailing slash.
pub fn base_url(model: &ModelConfig) -> String {
    model
        .base_url
        .clone()
        .or_else(|| model.provider.default_base_url().map(str::to_string))
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

/// True when the data sent to the model stays on this machine or the local
/// network. The address alone is not enough: a local Ollama server forwards
/// models with a `cloud` tag (for example `name:cloud` or `name-cloud`) to
/// ollama.com.
pub fn model_is_local(model: &ModelConfig) -> bool {
    let forwarded = model.provider == Provider::Ollama
        && (model.model.ends_with(":cloud") || model.model.ends_with("-cloud"));
    !forwarded && is_local_url(&base_url(model))
}

/// The model name sent to the server: the short Claude names are resolved to
/// the current versions.
pub fn resolve_model_name(name: &str) -> &str {
    match name {
        "claude-sonnet" => "claude-sonnet-5-5",
        "claude-opus" => "claude-opus-5-5",
        "claude-haiku" => "claude-haiku-5-5",
        other => other,
    }
}

/// True when the address is on this machine or the local network: localhost,
/// loopback, private IPv4 ranges, link-local, `.local` and `.lan` names, and
/// names without a dot.
pub fn is_local_url(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    }
    .to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") || host == "::1" {
        return true;
    }
    if host.ends_with(".local") || host.ends_with(".lan") || host.ends_with(".internal") {
        return true;
    }
    let octets: Vec<&str> = host.split('.').collect();
    if octets.len() == 4 && octets.iter().all(|o| o.parse::<u8>().is_ok()) {
        let n: Vec<u8> = octets.iter().filter_map(|o| o.parse().ok()).collect();
        return n[0] == 127
            || n[0] == 10
            || (n[0] == 192 && n[1] == 168)
            || (n[0] == 172 && (16..=31).contains(&n[1]))
            || (n[0] == 169 && n[1] == 254);
    }
    // A name without a dot is a machine on the local network.
    !host.contains('.') && !host.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[models.local-vision]
provider = "ollama"
base_url = "http://localhost:11434"
model = "qwen3-vl:7b"

[models.cloud-best]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "ANTHROPIC_API_KEY"

[tasks]
pin_tables = "local-vision"
drawings   = "cloud-best"
metadata   = "local-vision"

[privacy]
local_only = false
"#;

    #[test]
    fn the_example_of_the_requirements_loads() {
        let c = Config::from_toml_str(EXAMPLE).unwrap();
        assert_eq!(c.models.len(), 2);
        let (name, m) = c.model_for(Task::Drawings).unwrap();
        assert_eq!(name, "cloud-best");
        assert_eq!(m.provider, Provider::Anthropic);
        assert_eq!(resolve_model_name(&m.model), "claude-sonnet-5-5");
        assert!(c.stays_local(Task::PinTables));
        assert!(!c.stays_local(Task::Drawings));
    }

    #[test]
    fn local_only_refuses_cloud_models() {
        let text = EXAMPLE.replace("local_only = false", "local_only = true");
        let c = Config::from_toml_str(&text).unwrap();
        assert!(c.model_for(Task::PinTables).is_ok());
        assert!(matches!(
            c.model_for(Task::Drawings),
            Err(ConfigError::NotLocal { .. })
        ));
    }

    #[test]
    fn mistakes_are_named() {
        let bad = EXAMPLE.replace("pin_tables = \"local-vision\"", "pin_tables = \"nope\"");
        assert!(matches!(
            Config::from_toml_str(&bad),
            Err(ConfigError::UnknownModel { .. })
        ));
        let no_url = "[models.x]\nprovider = \"openai-compatible\"\nmodel = \"m\"\n";
        assert_eq!(
            Config::from_toml_str(no_url),
            Err(ConfigError::NoBaseUrl("x".into()))
        );
        // A key in the file is not accepted: only the name of a variable is.
        let key = "[models.x]\nprovider = \"openai\"\nmodel = \"m\"\napi_key = \"sk-secret\"\n";
        assert!(matches!(
            Config::from_toml_str(key),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn a_single_model_serves_every_task() {
        let text = "[models.only]\nprovider = \"ollama\"\nmodel = \"m\"\n";
        let c = Config::from_toml_str(text).unwrap();
        assert_eq!(c.model_for(Task::Metadata).unwrap().0, "only");
        let two = format!("{text}[models.b]\nprovider = \"ollama\"\nmodel = \"n\"\n");
        let c = Config::from_toml_str(&two).unwrap();
        assert!(c.model_for(Task::Metadata).is_err());
    }

    #[test]
    fn ollama_cloud_models_are_not_local_even_on_a_local_server() {
        let text = "[models.c]
provider = \"ollama\"
model = \"glm-5.3-flash:cloud\"
[privacy]
local_only = true
";
        let c = Config::from_toml_str(text).unwrap();
        assert!(matches!(
            c.model_for(Task::PinTables),
            Err(ConfigError::NotLocal { .. })
        ));
        let open = text.replace("local_only = true", "local_only = false");
        let c = Config::from_toml_str(&open).unwrap();
        assert!(!c.stays_local(Task::PinTables));
        let local = open.replace(":cloud", ":7b");
        assert!(
            Config::from_toml_str(&local)
                .unwrap()
                .stays_local(Task::PinTables)
        );
    }

    #[test]
    fn local_addresses_are_recognised() {
        for url in [
            "http://localhost:11434",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:1234",
            "http://192.168.1.20:8000",
            "http://10.0.0.5",
            "http://172.20.1.1",
            "http://gpu-box:11434",
            "http://box.local",
        ] {
            assert!(is_local_url(url), "{url}");
        }
        for url in [
            "https://api.anthropic.com",
            "https://ollama.com",
            "http://172.32.0.1",
            "http://8.8.8.8",
            "https://localhost.evil.com",
            "http://user@evil.com",
        ] {
            assert!(!is_local_url(url), "{url}");
        }
    }
}
