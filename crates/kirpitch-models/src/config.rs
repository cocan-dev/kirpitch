// SPDX-License-Identifier: GPL-3.0-or-later

//! The `kirpitch.toml` configuration: which models exist, which model does
//! which task, and the privacy switch.
//!
//! API keys are never in the file. A model names the environment variable
//! that holds its key (`api_key_env`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::prompts::Prompts;

/// Name of the configuration file looked for in the current folder.
pub const CONFIG_FILE: &str = "kirpitch.toml";

/// How Kirpitch talks to a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    /// How to talk to it.
    pub provider: Provider,
    /// The model name; `claude-sonnet`, `claude-opus` and `claude-haiku` are
    /// resolved to the current versions.
    pub model: String,
    /// Server address; the provider's usual address when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Environment variable that holds the API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Header that carries the key, when it is not the usual one (for example
    /// `api-key` for Azure OpenAI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_header: Option<String>,
    /// Whether the model can read images. Unknown when left out (the probe of
    /// `kirpitch models test` finds out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
    /// Ollama only: whether the model may think before it answers. Models
    /// that think can use all of their answer budget on it and answer
    /// nothing; set this to `false` for them. Not sent when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think: Option<bool>,
    /// The key is in the operating system keychain (set in the app), under the
    /// name of this model. A key in the environment variable wins.
    #[serde(default, skip_serializing_if = "is_false")]
    pub keychain: bool,
    /// How hard the model thinks before it answers: `none`, `minimal`, `low`,
    /// `medium` or `high`. Sent in the way each kind of server takes it
    /// (`reasoning_effort` for OpenAI-style servers, `think` for Ollama, a thinking
    /// budget for Anthropic). Not sent when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Size of the context window to ask for, in tokens. Ollama only (`num_ctx`):
    /// left out, Kirpitch asks for what each question needs, so Ollama does not
    /// cut the page text off at its small default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    /// Temperature, 0 to 2. Left out: 0 (as repeatable as the server can be),
    /// and not sent at all when `effort` is set, since reasoning models refuse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// More fields for the request, as a JSON object. They are added to the body
    /// and win over what Kirpitch sets, so any option a server has can be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<String>,
    /// Price of one million input tokens, for the cost estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_in: Option<f64>,
    /// Price of one million output tokens, for the cost estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_out: Option<f64>,
}

/// The values of `effort`.
pub const EFFORTS: [&str; 5] = ["none", "minimal", "low", "medium", "high"];

/// Fields of the request that a model's own settings may not replace.
const PROTECTED_FIELDS: [&str; 3] = ["model", "messages", "stream"];

impl ModelConfig {
    /// Checks the options that shape the request.
    ///
    /// # Errors
    ///
    /// An option has a value that makes no sense; the message says which.
    pub fn check_options(&self) -> Result<(), String> {
        if let Some(e) = &self.effort
            && !EFFORTS.contains(&e.as_str())
        {
            return Err(format!("effort `{e}` is not one of {}", EFFORTS.join(", ")));
        }
        if let Some(n) = self.context_length
            && !(512..=4_000_000).contains(&n)
        {
            return Err("the context length must be from 512 to 4000000 tokens".into());
        }
        if let Some(t) = self.temperature
            && !(0.0..=2.0).contains(&t)
        {
            return Err("the temperature must be from 0 to 2".into());
        }
        self.extra_fields().map(|_| ())
    }

    /// The extra request fields, parsed.
    ///
    /// # Errors
    ///
    /// The text is not a JSON object, or sets a field that cannot be replaced.
    pub fn extra_fields(&self) -> Result<serde_json::Map<String, serde_json::Value>, String> {
        let Some(text) = self.extra_body.as_deref().filter(|t| !t.trim().is_empty()) else {
            return Ok(serde_json::Map::new());
        };
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| format!("the extra fields are not JSON ({e})"))?;
        let serde_json::Value::Object(map) = value else {
            return Err("the extra fields must be a JSON object, like {\"top_p\": 0.9}".into());
        };
        if let Some(k) = PROTECTED_FIELDS.iter().find(|k| map.contains_key(**k)) {
            return Err(format!("the extra fields cannot set `{k}`"));
        }
        Ok(map)
    }
}

/// Which model does which task.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tasks {
    /// Reading pin tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_tables: Option<String>,
    /// Reading package drawings and dimension tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawings: Option<String>,
    /// Reading the part number, maker and description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

fn is_default_extraction(e: &Extraction) -> bool {
    *e == Extraction::default()
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Folders and defaults for the app. The command line ignores them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folders {
    /// Where new and saved parts go by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<String>,
    /// Where KiCad files are written by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Where the Browse dialog for datasheets starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheets: Option<String>,
    /// KiCad major version to write for (8, 9 or 10); the installed one when
    /// left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kicad_target: Option<u32>,
    /// A fab profile file (see `--fab`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fab_profile: Option<String>,
    /// The folder with the pdfium library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdfium_dir: Option<String>,
}

/// How the extraction runs. Every value left out has the default below.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extraction {
    /// Read the pin table twice by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_read: Option<bool>,
    /// Tries for each step: the first and then repairs of a bad answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// Seconds to wait for one answer of a model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Longest answer for the part number step, in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_metadata: Option<u32>,
    /// Longest answer for the pin table step, in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_pins: Option<u32>,
    /// Longest answer for the package step, in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_package: Option<u32>,
    /// Longest answer for the recommended footprint step, in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_footprint: Option<u32>,
    /// Most characters of page text sent for the recommended footprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint_pages_chars: Option<usize>,
    /// Most characters of page text sent for the pin table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_pages_chars: Option<usize>,
    /// Most characters of page text sent for the package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_pages_chars: Option<usize>,
    /// Keep answers on disk and reuse them for the same question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
}

impl Extraction {
    /// Read the pin table twice.
    pub fn second_read(&self) -> bool {
        self.second_read.unwrap_or(false)
    }
    /// Tries for each step.
    pub fn attempts(&self) -> u32 {
        self.attempts.unwrap_or(3)
    }
    /// Seconds to wait for one answer.
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs.unwrap_or(300)
    }
    /// Longest answer of the part number step.
    pub fn max_tokens_metadata(&self) -> u32 {
        self.max_tokens_metadata.unwrap_or(2000)
    }
    /// Longest answer of the pin table step.
    pub fn max_tokens_pins(&self) -> u32 {
        self.max_tokens_pins.unwrap_or(8000)
    }
    /// Longest answer of the package step.
    pub fn max_tokens_package(&self) -> u32 {
        self.max_tokens_package.unwrap_or(4000)
    }
    /// Longest answer of the recommended footprint step.
    pub fn max_tokens_footprint(&self) -> u32 {
        self.max_tokens_footprint.unwrap_or(2000)
    }
    /// Characters of footprint pages.
    pub fn footprint_pages_chars(&self) -> usize {
        self.footprint_pages_chars.unwrap_or(12_000)
    }
    /// Characters of pin pages.
    pub fn pin_pages_chars(&self) -> usize {
        self.pin_pages_chars.unwrap_or(24_000)
    }
    /// Characters of package pages.
    pub fn package_pages_chars(&self) -> usize {
        self.package_pages_chars.unwrap_or(16_000)
    }
    /// Whether answers are kept.
    pub fn cache(&self) -> bool {
        self.cache.unwrap_or(true)
    }

    /// The same options with every value that equals its default left out.
    pub fn normalized(&self) -> Extraction {
        let d = Extraction::default();
        Extraction {
            second_read: self.second_read.filter(|v| *v != d.second_read()),
            attempts: self.attempts.filter(|v| *v != d.attempts()),
            timeout_secs: self.timeout_secs.filter(|v| *v != d.timeout_secs()),
            max_tokens_metadata: self
                .max_tokens_metadata
                .filter(|v| *v != d.max_tokens_metadata()),
            max_tokens_footprint: self
                .max_tokens_footprint
                .filter(|v| *v != d.max_tokens_footprint()),
            footprint_pages_chars: self
                .footprint_pages_chars
                .filter(|v| *v != d.footprint_pages_chars()),
            max_tokens_pins: self.max_tokens_pins.filter(|v| *v != d.max_tokens_pins()),
            max_tokens_package: self
                .max_tokens_package
                .filter(|v| *v != d.max_tokens_package()),
            pin_pages_chars: self.pin_pages_chars.filter(|v| *v != d.pin_pages_chars()),
            package_pages_chars: self
                .package_pages_chars
                .filter(|v| *v != d.package_pages_chars()),
            cache: self.cache.filter(|v| *v != d.cache()),
        }
    }

    /// Checks the ranges.
    ///
    /// # Errors
    ///
    /// A value is outside what makes sense.
    pub fn check(&self) -> Result<(), String> {
        let range = |name: &str, ok: bool, what: &str| {
            if ok {
                Ok(())
            } else {
                Err(format!("{name} must be {what}"))
            }
        };
        range(
            "attempts",
            (1..=5).contains(&self.attempts()),
            "from 1 to 5",
        )?;
        range(
            "the time limit",
            (30..=3600).contains(&self.timeout_secs()),
            "from 30 to 3600 seconds",
        )?;
        for (name, v) in [
            (
                "the answer limit of the part number step",
                self.max_tokens_metadata(),
            ),
            (
                "the answer limit of the pin table step",
                self.max_tokens_pins(),
            ),
            (
                "the answer limit of the package step",
                self.max_tokens_package(),
            ),
            (
                "the answer limit of the footprint step",
                self.max_tokens_footprint(),
            ),
        ] {
            range(
                name,
                (256..=64_000).contains(&v),
                "from 256 to 64000 tokens",
            )?;
        }
        for (name, v) in [
            ("the page text for the pin table", self.pin_pages_chars()),
            ("the page text for the package", self.package_pages_chars()),
            (
                "the page text for the footprint",
                self.footprint_pages_chars(),
            ),
        ] {
            range(
                name,
                (2000..=200_000).contains(&v),
                "from 2000 to 200000 characters",
            )?;
        }
        Ok(())
    }
}

/// Privacy settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Privacy {
    /// Refuse every server that is not on this machine or the local network.
    #[serde(default)]
    pub local_only: bool,
}

/// The whole configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// Folders and defaults of the app.
    #[serde(default)]
    pub folders: Folders,
    /// How the extraction runs; the values left out are the defaults.
    #[serde(default, skip_serializing_if = "is_default_extraction")]
    pub extraction: Extraction,
    /// The prompts that were changed; the others are the defaults.
    #[serde(default, skip_serializing_if = "Prompts::is_default")]
    pub prompts: Prompts,
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

    /// Looks for `kirpitch.toml` in the current folder, then in the folder of
    /// the user's settings (see [`user_config_dir`]). `None` when there is none,
    /// which means no-LLM mode.
    pub fn discover() -> Result<Option<Config>, ConfigError> {
        let path = active_config_path();
        if path.is_file() {
            Config::from_file(&path).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The configuration as TOML text. Comments of a hand-written file are not
    /// kept.
    pub fn to_toml_string(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Writes the configuration to `path` (creating the folder). The file is
    /// written whole to a side file first, so an interruption cannot leave half
    /// a file.
    pub fn write_to(&self, path: &Path) -> Result<(), ConfigError> {
        self.check()?;
        let text = self.to_toml_string()?;
        let io = |e: std::io::Error| ConfigError::Io(format!("{}: {e}", path.display()));
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let side = path.with_extension("toml.tmp");
        std::fs::write(&side, text).map_err(io)?;
        std::fs::rename(&side, path).map_err(io)
    }

    fn check(&self) -> Result<(), ConfigError> {
        self.extraction.check().map_err(ConfigError::Parse)?;
        for p in &crate::prompts::PROMPT_LIST {
            if self.prompts.get(p.key).chars().count() > crate::prompts::MAX_PROMPT_CHARS {
                return Err(ConfigError::Parse(format!(
                    "the prompt `{}` is longer than {} characters",
                    p.key,
                    crate::prompts::MAX_PROMPT_CHARS
                )));
            }
        }
        for (name, m) in &self.models {
            if m.base_url.is_none() && m.provider.default_base_url().is_none() {
                return Err(ConfigError::NoBaseUrl(name.clone()));
            }
            m.check_options()
                .map_err(|e| ConfigError::Parse(format!("model `{name}`: {e}")))?;
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

/// The folder of the user's settings: `%APPDATA%\Kirpitch` on Windows,
/// `~/Library/Application Support/Kirpitch` on macOS, else `$XDG_CONFIG_HOME/kirpitch`
/// or `~/.config/kirpitch`. `KIRPITCH_CONFIG_DIR` replaces it (tests use it).
pub fn user_config_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(dir) = std::env::var_os("KIRPITCH_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Kirpitch"))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support/Kirpitch"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("kirpitch"))
            .or_else(|| home().map(|h| h.join(".config/kirpitch")))
    }
}

/// The configuration file in use: `kirpitch.toml` in the current folder when it
/// exists, else the one in the user's settings folder (which may not exist yet).
pub fn active_config_path() -> std::path::PathBuf {
    let local = Path::new(CONFIG_FILE);
    if local.is_file() {
        return local.to_path_buf();
    }
    user_config_dir().map_or_else(|| local.to_path_buf(), |d| d.join(CONFIG_FILE))
}

/// The server address of a model, without a trailing slash.
pub fn base_url(model: &ModelConfig) -> String {
    let url = model
        .base_url
        .clone()
        .or_else(|| model.provider.default_base_url().map(str::to_string))
        .unwrap_or_default();
    server_root(model.provider, &url)
}

/// The address of a server without a trailing slash. Ollama's own API lives at
/// the root (`/api/chat`), so a `/v1` left over from its OpenAI-compatible
/// address (`https://ollama.com/v1`) is dropped for the Ollama kind: sent to
/// `.../v1/api/chat`, the server answers 404.
pub fn server_root(provider: Provider, url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if provider == Provider::Ollama {
        let url = url.strip_suffix("/api").unwrap_or(url);
        return url.strip_suffix("/v1").unwrap_or(url).to_string();
    }
    url.to_string()
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
    fn a_config_survives_writing_and_reading() {
        let text = format!(
            "{EXAMPLE}
[folders]
parts = \"C:/parts\"
kicad_target = 9
"
        );
        let c = Config::from_toml_str(&text).unwrap();
        assert_eq!(c.folders.kicad_target, Some(9));
        let dir = std::env::temp_dir().join(format!("kirpitch-config-test-{}", std::process::id()));
        let path = dir.join("nested/kirpitch.toml");
        c.write_to(&path).unwrap();
        let back = Config::from_file(&path).unwrap();
        assert_eq!(back, c);
        // No side file is left behind.
        assert!(!path.with_extension("toml.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_config_with_a_broken_task_is_not_written() {
        let mut c = Config::from_toml_str(EXAMPLE).unwrap();
        c.tasks.drawings = Some("missing".into());
        let path = std::env::temp_dir().join("kirpitch-config-never-written.toml");
        assert!(matches!(
            c.write_to(&path),
            Err(ConfigError::UnknownModel { .. })
        ));
        assert!(!path.exists());
    }

    #[test]
    fn keys_are_never_written_to_the_file() {
        let mut c = Config::from_toml_str(EXAMPLE).unwrap();
        c.models.get_mut("cloud-best").unwrap().keychain = true;
        let text = c.to_toml_string().unwrap();
        assert!(text.contains("keychain = true"));
        assert!(!text.contains("sk-"));
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
