// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings screen: models, keys, tasks, privacy and folders.
//!
//! Everything is kept in the same `kirpitch.toml` that the command line reads.
//! Keys never go into that file: they are kept in the operating system
//! keychain, under the name of the model.

use std::path::Path;
use std::sync::Arc;

use kirpitch_models::config::{base_url, model_is_local};
use kirpitch_models::{
    Client, Config, Folders, KeyStore, ModelConfig, OsKeyStore, Provider, Transport,
    active_config_path,
};
use serde::{Deserialize, Serialize};

/// One model as the screen shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelView {
    /// Name in the configuration.
    pub name: String,
    /// `anthropic`, `openai`, `openai-compatible` or `ollama`.
    pub provider: String,
    /// The address as typed (empty for the usual one).
    pub base_url: String,
    /// The address that is used.
    pub effective_url: String,
    /// The model name sent to the server.
    pub model: String,
    /// Environment variable that holds the key, if any.
    pub api_key_env: String,
    /// Whether the key is kept in the keychain.
    pub keychain: bool,
    /// Whether a key is stored in the keychain now.
    pub has_key: bool,
    /// Whether the environment variable is set now.
    pub env_set: bool,
    /// Header that carries the key, if not the usual one.
    pub api_key_header: String,
    /// Ollama: may the model think first.
    pub think: Option<bool>,
    /// Price of a million input tokens.
    pub price_in: Option<f64>,
    /// Price of a million output tokens.
    pub price_out: Option<f64>,
    /// Whether data sent to this model stays on this machine or network.
    pub local: bool,
}

/// What the settings screen shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsView {
    /// The file the settings are read from and written to.
    pub path: String,
    /// Whether it exists yet.
    pub exists: bool,
    /// Set when the file could not be read; saving replaces it.
    pub config_error: Option<String>,
    /// The models.
    pub models: Vec<ModelView>,
    /// Model of each task (empty: the first model).
    pub pin_tables: String,
    /// Model for package drawings.
    pub drawings: String,
    /// Model for the part number, maker and description.
    pub metadata: String,
    /// Refuse every server that is not local.
    pub local_only: bool,
    /// Folders and defaults.
    pub parts: String,
    /// Default folder for KiCad files.
    pub output: String,
    /// Where the datasheet dialog starts.
    pub datasheets: String,
    /// KiCad version to write for (empty: the installed one).
    pub kicad_target: String,
    /// A fab profile file.
    pub fab_profile: String,
    /// The folder with pdfium, as set.
    pub pdfium_dir: String,
    /// The pdfium folder that is found now, if any.
    pub pdfium_found: Option<String>,
    /// The ways to talk to a model, for the menu.
    pub providers: Vec<ProviderInfo>,
    /// The prompts of the extraction, with their defaults.
    pub prompts: Vec<PromptView>,
    /// A short code naming this set of prompts.
    pub prompts_fingerprint: String,
}

/// One prompt as the screen shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PromptView {
    /// Name in the file.
    pub key: String,
    /// Short title.
    pub title: String,
    /// What it does.
    pub about: String,
    /// The placeholders it may use.
    pub placeholders: String,
    /// The default text.
    pub default: String,
    /// The text in use (the default when not changed).
    pub text: String,
}

fn prompt_views(prompts: &kirpitch_models::Prompts) -> Vec<PromptView> {
    kirpitch_models::PROMPT_LIST
        .iter()
        .map(|p| PromptView {
            key: p.key.into(),
            title: p.title.into(),
            about: p.about.into(),
            placeholders: p.placeholders.into(),
            default: p.default.into(),
            text: prompts.get(p.key).into(),
        })
        .collect()
}

/// A kind of server.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderInfo {
    /// The name in the file.
    pub id: String,
    /// What the menu shows.
    pub label: String,
    /// The usual address, empty when there is none.
    pub default_url: String,
    /// Hint about keys.
    pub key_hint: String,
}

fn providers() -> Vec<ProviderInfo> {
    let info = |id: &str, label: &str, key_hint: &str| ProviderInfo {
        id: id.into(),
        label: label.into(),
        default_url: serde_json::from_value::<Provider>(serde_json::Value::String(id.into()))
            .ok()
            .and_then(Provider::default_base_url)
            .unwrap_or("")
            .into(),
        key_hint: key_hint.into(),
    };
    vec![
        info("anthropic", "Anthropic (Claude)", "needs a key"),
        info("openai", "OpenAI", "needs a key"),
        info(
            "openai-compatible",
            "OpenAI-compatible server (LM Studio, llama.cpp, vLLM, OpenRouter, Mistral, Gemini, Azure ...)",
            "key if the server wants one",
        ),
        info(
            "ollama",
            "Ollama (local, or ollama.com)",
            "no key for a local server",
        ),
    ]
}

fn provider_id(p: Provider) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn model_view(name: &str, m: &ModelConfig, keys: &dyn KeyStore) -> ModelView {
    ModelView {
        name: name.to_string(),
        provider: provider_id(m.provider),
        base_url: m.base_url.clone().unwrap_or_default(),
        effective_url: base_url(m),
        model: m.model.clone(),
        api_key_env: m.api_key_env.clone().unwrap_or_default(),
        keychain: m.keychain,
        has_key: m.keychain && matches!(keys.get(name), Ok(Some(k)) if !k.is_empty()),
        env_set: m
            .api_key_env
            .as_ref()
            .is_some_and(|v| std::env::var(v).is_ok_and(|x| !x.is_empty())),
        api_key_header: m.api_key_header.clone().unwrap_or_default(),
        think: m.think,
        price_in: m.price_in,
        price_out: m.price_out,
        local: model_is_local(m),
    }
}

fn text(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

/// Reads the settings file (a missing file is an empty configuration).
pub fn settings_view(path: &Path, keys: &dyn KeyStore) -> SettingsView {
    let (config, error) = if path.is_file() {
        match Config::from_file(path) {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        }
    } else {
        (Config::default(), None)
    };
    SettingsView {
        path: path.display().to_string(),
        exists: path.is_file(),
        config_error: error,
        models: config
            .models
            .iter()
            .map(|(n, m)| model_view(n, m, keys))
            .collect(),
        pin_tables: text(&config.tasks.pin_tables),
        drawings: text(&config.tasks.drawings),
        metadata: text(&config.tasks.metadata),
        local_only: config.privacy.local_only,
        parts: text(&config.folders.parts),
        output: text(&config.folders.output),
        datasheets: text(&config.folders.datasheets),
        kicad_target: config
            .folders
            .kicad_target
            .map_or(String::new(), |t| t.to_string()),
        fab_profile: text(&config.folders.fab_profile),
        pdfium_dir: text(&config.folders.pdfium_dir),
        pdfium_found: crate::find_pdfium_dir().map(|d| d.display().to_string()),
        providers: providers(),
        prompts: prompt_views(&config.prompts),
        prompts_fingerprint: config.prompts.fingerprint(),
    }
}

/// A model as the form sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInput {
    /// The name before the edit, when a model is renamed.
    #[serde(default)]
    pub old_name: Option<String>,
    /// The name.
    pub name: String,
    /// `anthropic`, `openai`, `openai-compatible` or `ollama`.
    pub provider: String,
    /// Address; empty for the usual one.
    #[serde(default)]
    pub base_url: String,
    /// The model name.
    pub model: String,
    /// Environment variable with the key.
    #[serde(default)]
    pub api_key_env: String,
    /// Keep the key in the keychain.
    #[serde(default)]
    pub keychain: bool,
    /// Header for the key.
    #[serde(default)]
    pub api_key_header: String,
    /// Ollama: may the model think first.
    #[serde(default)]
    pub think: Option<bool>,
    /// Price of a million input tokens.
    #[serde(default)]
    pub price_in: Option<f64>,
    /// Price of a million output tokens.
    #[serde(default)]
    pub price_out: Option<f64>,
}

/// All settings as the screen sends them.
#[derive(Debug, Clone, Deserialize)]
pub struct SettingsInput {
    /// The models.
    pub models: Vec<ModelInput>,
    /// Model for pin tables.
    #[serde(default)]
    pub pin_tables: String,
    /// Model for drawings.
    #[serde(default)]
    pub drawings: String,
    /// Model for metadata.
    #[serde(default)]
    pub metadata: String,
    /// Local models only.
    #[serde(default)]
    pub local_only: bool,
    /// Default folder for new parts.
    #[serde(default)]
    pub parts: String,
    /// Default folder for KiCad files.
    #[serde(default)]
    pub output: String,
    /// Where the datasheet dialog starts.
    #[serde(default)]
    pub datasheets: String,
    /// KiCad version (8, 9 or 10), empty for the installed one.
    #[serde(default)]
    pub kicad_target: String,
    /// A fab profile file.
    #[serde(default)]
    pub fab_profile: String,
    /// The folder with pdfium.
    #[serde(default)]
    pub pdfium_dir: String,
    /// Prompt texts by key; a prompt that is empty or equal to its default is
    /// not saved.
    #[serde(default)]
    pub prompts: std::collections::BTreeMap<String, String>,
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Checks the form and builds the configuration from it.
pub fn config_from_input(input: &SettingsInput) -> Result<Config, String> {
    let mut config = Config::default();
    for m in &input.models {
        let name = m.name.trim();
        if !valid_name(name) {
            return Err(format!(
                "`{name}` is not a good model name: use letters, digits, - _ and . (no spaces)"
            ));
        }
        if config.models.contains_key(name) {
            return Err(format!("there are two models called `{name}`"));
        }
        if m.model.trim().is_empty() {
            return Err(format!(
                "model `{name}` needs a model name (what the server calls it)"
            ));
        }
        let provider: Provider =
            serde_json::from_value(serde_json::Value::String(m.provider.clone()))
                .map_err(|_| format!("model `{name}`: unknown kind of server `{}`", m.provider))?;
        let url = opt(&m.base_url);
        if let Some(u) = &url
            && !(u.starts_with("http://") || u.starts_with("https://"))
        {
            return Err(format!(
                "model `{name}`: the address must start with http:// or https://"
            ));
        }
        config.models.insert(
            name.to_string(),
            ModelConfig {
                provider,
                model: m.model.trim().to_string(),
                base_url: url,
                api_key_env: opt(&m.api_key_env),
                api_key_header: opt(&m.api_key_header),
                vision: None,
                think: m.think,
                keychain: m.keychain,
                price_in: m.price_in,
                price_out: m.price_out,
            },
        );
    }
    config.tasks.pin_tables = opt(&input.pin_tables);
    config.tasks.drawings = opt(&input.drawings);
    config.tasks.metadata = opt(&input.metadata);
    config.privacy.local_only = input.local_only;
    for (key, text) in &input.prompts {
        config.prompts.set(key, text)?;
    }
    config.folders = Folders {
        parts: opt(&input.parts),
        output: opt(&input.output),
        datasheets: opt(&input.datasheets),
        kicad_target: match input.kicad_target.trim() {
            "" => None,
            "8" => Some(8),
            "9" => Some(9),
            "10" => Some(10),
            other => return Err(format!("KiCad version `{other}` is not 8, 9 or 10")),
        },
        fab_profile: opt(&input.fab_profile),
        pdfium_dir: opt(&input.pdfium_dir),
    };
    Ok(config)
}

/// Saves the settings. Models that were removed or renamed take their keychain
/// entry with them.
pub fn save_settings(
    path: &Path,
    input: &SettingsInput,
    keys: &dyn KeyStore,
) -> Result<SettingsView, String> {
    let config = config_from_input(input)?;
    let before = if path.is_file() {
        Config::from_file(path).ok()
    } else {
        None
    };
    config.write_to(path).map_err(|e| e.to_string())?;
    // Renamed models move their key; removed models lose it.
    for m in &input.models {
        if let Some(old) = m
            .old_name
            .as_deref()
            .filter(|o| !o.is_empty() && *o != m.name.trim())
            && let Ok(Some(key)) = keys.get(old)
        {
            keys.set(m.name.trim(), &key).map_err(|e| e.to_string())?;
            keys.delete(old).map_err(|e| e.to_string())?;
        }
    }
    if let Some(before) = before {
        let kept: Vec<&str> = input
            .models
            .iter()
            .flat_map(|m| [Some(m.name.trim()), m.old_name.as_deref()])
            .flatten()
            .collect();
        for (name, m) in &before.models {
            if m.keychain && !kept.contains(&name.as_str()) {
                keys.delete(name).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(settings_view(path, keys))
}

/// Stores the key of a model in the keychain. The model must be in the saved
/// settings and marked as keeping its key there.
pub fn set_api_key(
    path: &Path,
    name: &str,
    key: &str,
    keys: &dyn KeyStore,
) -> Result<SettingsView, String> {
    if key.trim().is_empty() {
        return Err("the key is empty".into());
    }
    let config = Config::from_file(path).map_err(|e| e.to_string())?;
    let model = config
        .models
        .get(name)
        .ok_or_else(|| format!("save the model `{name}` first, then set its key"))?;
    if !model.keychain {
        return Err(format!(
            "model `{name}` is not set to keep its key in the keychain"
        ));
    }
    keys.set(name, key.trim()).map_err(|e| e.to_string())?;
    Ok(settings_view(path, keys))
}

/// Removes the key of a model from the keychain.
pub fn delete_api_key(
    path: &Path,
    name: &str,
    keys: &dyn KeyStore,
) -> Result<SettingsView, String> {
    keys.delete(name).map_err(|e| e.to_string())?;
    Ok(settings_view(path, keys))
}

/// The result of testing a model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelTest {
    /// The model.
    pub name: String,
    /// Reached at all.
    pub reachable: bool,
    /// Answered a request with a JSON schema correctly.
    pub structured_json: bool,
    /// Can read images (`None`: not tested).
    pub vision: Option<bool>,
    /// Found a code word in about 8,000 tokens (`None`: not tested).
    pub context_8k: Option<bool>,
    /// Seconds for the first request.
    pub seconds: f64,
    /// Problems.
    pub notes: Vec<String>,
}

/// Tests a saved model: reachable, structured JSON, vision, long text.
pub fn test_model(
    path: &Path,
    name: &str,
    transport: Arc<dyn Transport>,
    keys: &dyn KeyStore,
) -> Result<ModelTest, String> {
    let config = Config::from_file(path).map_err(|e| e.to_string())?;
    let model = config
        .models
        .get(name)
        .ok_or_else(|| format!("save the model `{name}` first, then test it"))?;
    config.allowed(name, model).map_err(|e| e.to_string())?;
    let client = Client::connect(name, model, transport, keys).map_err(|e| e.to_string())?;
    let r = kirpitch_models::probe(name, &client);
    Ok(ModelTest {
        name: r.name,
        reachable: r.reachable,
        structured_json: r.structured_json,
        vision: r.vision,
        context_8k: r.context_8k,
        seconds: r.seconds,
        notes: r.notes,
    })
}

// ---- With the real keychain and the file in use ------------------------------------

/// The settings as the screen shows them now.
pub fn current_settings() -> SettingsView {
    settings_view(&active_config_path(), &OsKeyStore)
}

/// The folders and defaults of the active configuration; empty when there is
/// none or it cannot be read.
pub fn user_folders() -> Folders {
    Config::discover()
        .ok()
        .flatten()
        .map(|c| c.folders)
        .unwrap_or_default()
}

// ---- Finding the models a server offers -------------------------------------------

/// What the screen knows about a model when it asks for the server's list.
#[derive(Debug, Clone, Deserialize)]
pub struct FindInput {
    /// The model's name now.
    #[serde(default)]
    pub name: String,
    /// The name it is saved under, when it is saved.
    #[serde(default)]
    pub old_name: Option<String>,
    /// `anthropic`, `openai`, `openai-compatible` or `ollama`.
    pub provider: String,
    /// Address; empty for the usual one.
    #[serde(default)]
    pub base_url: String,
    /// Environment variable with the key.
    #[serde(default)]
    pub api_key_env: String,
    /// Header for the key.
    #[serde(default)]
    pub api_key_header: String,
    /// The key is kept in the keychain.
    #[serde(default)]
    pub keychain: bool,
    /// A key typed on the screen and not saved yet.
    #[serde(default)]
    pub new_key: String,
}

/// Asks the server of a model which models it offers, with what is known about
/// their prices. The key is the one typed on the screen, else the environment
/// variable, else the keychain. It goes only to the model's own server.
///
/// # Errors
///
/// The kind of server is unknown, a key that is needed is missing, or the
/// server cannot be reached or refuses.
pub fn find_models(
    input: &FindInput,
    transport: &dyn Transport,
    keys: &dyn KeyStore,
) -> Result<Vec<kirpitch_models::FoundModel>, String> {
    let provider: Provider =
        serde_json::from_value(serde_json::Value::String(input.provider.clone()))
            .map_err(|_| format!("unknown kind of server `{}`", input.provider))?;
    let typed = input.new_key.trim();
    let from_env = (!input.api_key_env.is_empty())
        .then(|| std::env::var(&input.api_key_env).ok())
        .flatten()
        .filter(|v| !v.is_empty());
    let from_keychain = if input.keychain && typed.is_empty() && from_env.is_none() {
        let account = input.old_name.as_deref().unwrap_or(&input.name);
        keys.get(account)
            .map_err(|e| e.to_string())?
            .filter(|k| !k.is_empty())
    } else {
        None
    };
    let key = (!typed.is_empty())
        .then(|| typed.to_string())
        .or(from_env)
        .or(from_keychain);
    let base = input.base_url.trim().to_string();
    let header = input.api_key_header.trim();
    kirpitch_models::list_models(
        &kirpitch_models::ListRequest {
            provider,
            base_url: &base,
            key: key.as_deref(),
            key_header: (!header.is_empty()).then_some(header),
        },
        transport,
    )
    .map_err(|e| e.to_string())
}
