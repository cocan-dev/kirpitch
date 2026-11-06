// SPDX-License-Identifier: GPL-3.0-or-later

//! One chat request to one model, for each kind of server.
//!
//! The request builders and answer readers are plain functions so they can be
//! tested with recorded answers.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};

use crate::config::{ModelConfig, Provider, base_url, resolve_model_name};
use crate::keys::KeyStore;
use crate::png::base64;
use crate::transport::{Request, Transport, TransportError};

/// Set this environment variable to print the raw answers of the servers.
pub const ENV_DEBUG: &str = "KIRPITCH_DEBUG_MODELS";

/// Seconds to wait for an answer.
pub const TIMEOUT_SECS: u64 = 300;

/// What to ask.
#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    /// Instructions.
    pub system: String,
    /// The question, with the page text.
    pub user: String,
    /// PNG images to look at.
    pub images: Vec<Vec<u8>>,
    /// JSON schema the answer must follow, when the answer is JSON. Servers
    /// that enforce schemas get it as a constraint.
    pub schema: Option<Value>,
    /// A made-up answer that shows the shape. Models follow an example far
    /// better than a schema, so the prompt carries this and not the schema.
    pub example: Option<Value>,
    /// Ask the model not to think before it answers (when the server knows
    /// how). Used after an empty answer: models that think can spend the
    /// whole answer budget on it.
    pub no_think: bool,
    /// Longest answer in tokens.
    pub max_tokens: u32,
    /// Seconds to wait for the answer; 0 means the usual `TIMEOUT_SECS`.
    pub timeout_secs: u64,
}

/// Tokens used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Tokens sent.
    pub input_tokens: u64,
    /// Tokens received.
    pub output_tokens: u64,
}

/// The answer.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    /// The text (for a JSON request, the JSON text).
    pub text: String,
    /// Tokens used.
    pub usage: Usage,
    /// Seconds the call took.
    pub seconds: f64,
    /// The model stopped because it reached the length limit, so the answer
    /// is cut off.
    pub truncated: bool,
    /// What the model wrote while thinking, when the server returns it.
    pub thinking: String,
    /// Why the model stopped, as the server says (`stop`, `length`, ...).
    pub finish: String,
}

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ModelError {
    /// The request did not get an answer.
    #[error("{0}")]
    Transport(#[from] TransportError),
    /// The server answered with an error.
    #[error("the server answered {status}: {message}")]
    Http {
        /// HTTP status.
        status: u16,
        /// What the server said.
        message: String,
    },
    /// The answer is not what the API documents.
    #[error("unexpected answer from the server: {0}")]
    Unexpected(String),
    /// The key variable is not set.
    #[error("the environment variable {0} is not set")]
    NoKey(String),
}

/// A connection to one model.
pub struct Client {
    model: ModelConfig,
    key: Option<String>,
    transport: Arc<dyn Transport>,
}

impl Client {
    /// Connects to `model`. The key is read from the environment variable the
    /// model names, if it names one.
    pub fn from_env(
        model: &ModelConfig,
        transport: Arc<dyn Transport>,
    ) -> Result<Client, ModelError> {
        let key = match &model.api_key_env {
            Some(var) => Some(
                std::env::var(var)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| ModelError::NoKey(var.clone()))?,
            ),
            None => None,
        };
        Ok(Client::new(model.clone(), key, transport))
    }

    /// Connects to the model called `name` in the configuration. The key comes
    /// from the environment variable the model names, when that is set, else
    /// from the keychain when the model keeps its key there. A model that names
    /// a variable that is not set and has no key in the keychain is an error.
    pub fn connect(
        name: &str,
        model: &ModelConfig,
        transport: Arc<dyn Transport>,
        keys: &dyn KeyStore,
    ) -> Result<Client, ModelError> {
        let from_env = model
            .api_key_env
            .as_ref()
            .and_then(|var| std::env::var(var).ok())
            .filter(|v| !v.is_empty());
        let key = match from_env {
            Some(k) => Some(k),
            None if model.keychain => keys
                .get(name)
                .map_err(|e| ModelError::Unexpected(e.to_string()))?
                .filter(|k| !k.is_empty()),
            None => None,
        };
        if key.is_none() {
            if let Some(var) = &model.api_key_env {
                return Err(ModelError::NoKey(var.clone()));
            }
            if model.keychain {
                return Err(ModelError::NoKey(format!("the keychain entry `{name}`")));
            }
        }
        Ok(Client::new(model.clone(), key, transport))
    }

    /// Connects with a key that is already known.
    pub fn new(model: ModelConfig, key: Option<String>, transport: Arc<dyn Transport>) -> Client {
        Client {
            model,
            key,
            transport,
        }
    }

    /// Asks the model.
    pub fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let http = build_request(&self.model, self.key.as_deref(), request);
        let start = Instant::now();
        let reply = self.transport.post(
            &http,
            if request.timeout_secs > 0 {
                request.timeout_secs
            } else {
                TIMEOUT_SECS
            },
        )?;
        if std::env::var_os(ENV_DEBUG).is_some() {
            eprintln!(
                "[kirpitch debug] {} answered {}: {}",
                http.url,
                reply.status,
                {
                    let n = reply.body.chars().count();
                    let head: String = reply.body.chars().take(1500).collect();
                    let tail: String = reply
                        .body
                        .chars()
                        .skip(n.saturating_sub(1500).max(1500))
                        .collect();
                    format!("{head} ... {tail}")
                }
            );
        }
        if !(200..300).contains(&reply.status) {
            return Err(ModelError::Http {
                status: reply.status,
                message: error_message(&reply.body),
            });
        }
        let mut response = parse_response(self.model.provider, &reply.body)?;
        response.seconds = start.elapsed().as_secs_f64();
        Ok(response)
    }

    /// The model this client talks to.
    pub fn model(&self) -> &ModelConfig {
        &self.model
    }
}

fn error_message(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed.as_ref().and_then(|v| {
        v.pointer("/error/message")
            .or_else(|| v.get("error"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    message.unwrap_or_else(|| body.chars().take(300).collect())
}

/// Builds the HTTP request for a chat request.
pub fn build_request(model: &ModelConfig, key: Option<&str>, chat: &ChatRequest) -> Request {
    let base = base_url(model);
    let name = resolve_model_name(&model.model);
    let mut headers: Vec<(String, String)> = Vec::new();
    let (url, body) = match model.provider {
        Provider::Anthropic => {
            if let Some(k) = key {
                headers.push((
                    model
                        .api_key_header
                        .clone()
                        .unwrap_or_else(|| "x-api-key".to_string()),
                    k.to_string(),
                ));
            }
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
            (
                format!("{base}/v1/messages"),
                anthropic_body(name, chat, model),
            )
        }
        Provider::Openai | Provider::OpenaiCompatible => {
            if let Some(k) = key {
                match &model.api_key_header {
                    Some(h) => headers.push((h.clone(), k.to_string())),
                    None => headers.push(("Authorization".into(), format!("Bearer {k}"))),
                }
            }
            let path = if base.ends_with("/v1") || base.contains("/openai") {
                "/chat/completions"
            } else {
                "/v1/chat/completions"
            };
            let mut body = openai_body(name, chat, model);
            // Ollama's OpenAI-compatible address understands this way of
            // asking a thinking model not to think.
            if chat.no_think && base.contains("ollama") && model.effort.is_none() {
                body["reasoning_effort"] = json!("none");
            }
            (format!("{base}{path}"), body)
        }
        Provider::Ollama => {
            if let Some(k) = key {
                headers.push(("Authorization".into(), format!("Bearer {k}")));
            }
            (format!("{base}/api/chat"), ollama_body(name, chat, model))
        }
    };
    let mut body = body;
    // Fields of the person's own win over everything above.
    if let Ok(extra) = model.extra_fields() {
        for (key, value) in extra {
            body[key] = value;
        }
    }
    Request {
        url,
        headers,
        body: body.to_string(),
    }
}

/// The context window to ask Ollama for: what the question and its answer need,
/// in steps so the model is not reloaded for every small change. Ollama's own
/// default is small and cuts the page text off without saying so.
pub fn auto_context(chat: &ChatRequest) -> u32 {
    let need = (chat.system.chars().count() + chat.user.chars().count()) / 3
        + chat.max_tokens as usize
        + 512;
    [4096usize, 8192, 16384, 32768, 65536, 131_072]
        .into_iter()
        .find(|c| *c >= need)
        .unwrap_or(131_072) as u32
}

/// The thinking budget of Anthropic for an effort level.
fn thinking_budget(effort: &str) -> Option<u32> {
    match effort {
        "minimal" => Some(1024),
        "low" => Some(2048),
        "medium" => Some(8192),
        "high" => Some(24_000),
        _ => None,
    }
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png))
}

fn openai_body(model: &str, chat: &ChatRequest, cfg: &ModelConfig) -> Value {
    let mut content = vec![json!({"type": "text", "text": chat.user})];
    for image in &chat.images {
        content.push(json!({"type": "image_url", "image_url": {"url": data_url(image)}}));
    }
    let mut body = json!({
        "model": model,
        "max_tokens": chat.max_tokens,
        "messages": [
            {"role": "system", "content": chat.system},
            {"role": "user", "content": content},
        ],
    });
    // Reasoning models refuse a temperature of their own: it is only sent when
    // there is no effort, or when the person set one.
    match (cfg.temperature, &cfg.effort) {
        (Some(t), _) => body["temperature"] = json!(t),
        (None, None) => body["temperature"] = json!(0),
        (None, Some(_)) => {}
    }
    if let Some(effort) = &cfg.effort {
        body["reasoning_effort"] = json!(effort);
        if cfg.provider == Provider::Openai {
            // OpenAI's reasoning models count the thinking in this field.
            body["max_completion_tokens"] = body["max_tokens"].take();
            body.as_object_mut().map(|o| o.remove("max_tokens"));
        }
    }
    if let Some(schema) = &chat.schema {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {"name": "result", "schema": schema},
        });
    }
    body
}

fn anthropic_body(model: &str, chat: &ChatRequest, cfg: &ModelConfig) -> Value {
    let mut content = Vec::new();
    for image in &chat.images {
        content.push(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": "image/png", "data": base64(image)},
        }));
    }
    content.push(json!({"type": "text", "text": chat.user}));
    let mut body = json!({
        "model": model,
        "max_tokens": chat.max_tokens,
        "system": chat.system,
        "messages": [{"role": "user", "content": content}],
    });
    let budget = cfg.effort.as_deref().and_then(thinking_budget);
    if let Some(budget) = budget {
        // Extended thinking: the thinking counts in max_tokens, a forced tool is
        // not allowed with it (the example in the prompt carries the shape), and
        // the temperature is the server's own.
        body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
        body["max_tokens"] = json!(chat.max_tokens + budget);
        if let Some(t) = cfg.temperature {
            body["temperature"] = json!(t);
        }
    } else {
        body["temperature"] = json!(cfg.temperature.unwrap_or(0.0));
        if let Some(schema) = &chat.schema {
            body["tools"] = json!([{
                "name": "result",
                "description": "Report the result.",
                "input_schema": schema,
            }]);
            body["tool_choice"] = json!({"type": "tool", "name": "result"});
        }
    }
    body
}

fn ollama_body(model: &str, chat: &ChatRequest, cfg: &ModelConfig) -> Value {
    let images: Vec<String> = chat.images.iter().map(|i| base64(i)).collect();
    let mut user = json!({"role": "user", "content": chat.user});
    if !images.is_empty() {
        user["images"] = json!(images);
    }
    let mut body = json!({
        "model": model,
        "stream": false,
        "options": {
            "temperature": cfg.temperature.unwrap_or(0.0),
            "num_predict": chat.max_tokens,
            "num_ctx": cfg.context_length.unwrap_or_else(|| auto_context(chat)),
        },
        "messages": [{"role": "system", "content": chat.system}, user],
    });
    if let Some(schema) = &chat.schema {
        body["format"] = schema.clone();
    }
    // Thinking: the effort says how much (a level for models that take one),
    // else the on/off switch.
    match cfg.effort.as_deref() {
        Some("none") => body["think"] = json!(false),
        Some("minimal") => body["think"] = json!("low"),
        Some(level) => body["think"] = json!(level),
        None => {
            if let Some(think) = cfg.think {
                body["think"] = json!(think);
            }
        }
    }
    if chat.no_think {
        body["think"] = json!(false);
    }
    body
}

/// Reads the answer of a server.
pub fn parse_response(provider: Provider, body: &str) -> Result<ChatResponse, ModelError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| ModelError::Unexpected(format!("not JSON ({e})")))?;
    let unexpected = |what: &str| ModelError::Unexpected(format!("no {what}"));
    let count = |p: &str| v.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    let truncated = match provider {
        Provider::Openai | Provider::OpenaiCompatible => {
            v.pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                == Some("length")
        }
        Provider::Anthropic => v.get("stop_reason").and_then(Value::as_str) == Some("max_tokens"),
        Provider::Ollama => v.get("done_reason").and_then(Value::as_str) == Some("length"),
    };
    let (text, usage) = match provider {
        Provider::Openai | Provider::OpenaiCompatible => {
            let text = v
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .ok_or_else(|| unexpected("choices[0].message.content"))?;
            (
                text.to_string(),
                Usage {
                    input_tokens: count("/usage/prompt_tokens"),
                    output_tokens: count("/usage/completion_tokens"),
                },
            )
        }
        Provider::Anthropic => {
            let blocks = v
                .get("content")
                .and_then(Value::as_array)
                .ok_or_else(|| unexpected("content"))?;
            let text = if let Some(tool) = blocks
                .iter()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            {
                tool.get("input")
                    .map(Value::to_string)
                    .ok_or_else(|| unexpected("tool input"))?
            } else {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            };
            (
                text,
                Usage {
                    input_tokens: count("/usage/input_tokens"),
                    output_tokens: count("/usage/output_tokens"),
                },
            )
        }
        Provider::Ollama => {
            let text = v
                .pointer("/message/content")
                .and_then(Value::as_str)
                .ok_or_else(|| unexpected("message.content"))?;
            (
                text.to_string(),
                Usage {
                    input_tokens: count("/prompt_eval_count"),
                    output_tokens: count("/eval_count"),
                },
            )
        }
    };
    let thinking = match provider {
        Provider::Openai | Provider::OpenaiCompatible => v
            .pointer("/choices/0/message/reasoning_content")
            .or_else(|| v.pointer("/choices/0/message/reasoning"))
            .and_then(Value::as_str),
        Provider::Ollama => v.pointer("/message/thinking").and_then(Value::as_str),
        Provider::Anthropic => None,
    }
    .unwrap_or("")
    .to_string();
    let finish = match provider {
        Provider::Openai | Provider::OpenaiCompatible => v.pointer("/choices/0/finish_reason"),
        Provider::Anthropic => v.get("stop_reason"),
        Provider::Ollama => v.get("done_reason"),
    }
    .and_then(Value::as_str)
    .unwrap_or("")
    .to_string();
    Ok(ChatResponse {
        text,
        usage,
        seconds: 0.0,
        truncated,
        thinking,
        finish,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    fn model(provider: Provider, base_url: Option<&str>) -> ModelConfig {
        ModelConfig {
            effort: None,
            context_length: None,
            temperature: None,
            extra_body: None,
            provider,
            model: "claude-sonnet".into(),
            base_url: base_url.map(str::to_string),
            api_key_env: None,
            api_key_header: None,
            vision: None,
            think: None,
            keychain: false,
            price_in: None,
            price_out: None,
        }
    }

    fn ask() -> ChatRequest {
        ChatRequest {
            system: "sys".into(),
            user: "question".into(),
            images: vec![vec![1, 2, 3]],
            schema: Some(json!({"type": "object"})),
            max_tokens: 100,
            ..Default::default()
        }
    }

    #[test]
    fn anthropic_requests_use_a_tool_for_json() {
        let r = build_request(&model(Provider::Anthropic, None), Some("k"), &ask());
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert!(r.headers.contains(&("x-api-key".into(), "k".into())));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["model"], "claude-sonnet-5-5");
        assert_eq!(b["tool_choice"]["name"], "result");
        assert_eq!(b["messages"][0]["content"][0]["type"], "image");
        assert_eq!(b["messages"][0]["content"][0]["source"]["data"], "AQID");
    }

    #[test]
    fn openai_requests_use_json_schema_and_data_urls() {
        let m = model(
            Provider::OpenaiCompatible,
            Some("http://localhost:1234/v1/"),
        );
        let r = build_request(&m, None, &ask());
        assert_eq!(r.url, "http://localhost:1234/v1/chat/completions");
        assert!(r.headers.is_empty());
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["response_format"]["type"], "json_schema");
        assert_eq!(
            b["messages"][1]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AQID"
        );
        let r = build_request(&model(Provider::Openai, None), Some("sk"), &ask());
        assert_eq!(r.url, "https://api.openai.com/v1/chat/completions");
        assert!(
            r.headers
                .contains(&("Authorization".into(), "Bearer sk".into()))
        );
        let mut azure = model(
            Provider::OpenaiCompatible,
            Some("https://x.openai.azure.com/openai/v1"),
        );
        azure.api_key_header = Some("api-key".into());
        let r = build_request(&azure, Some("k"), &ask());
        assert_eq!(
            r.url,
            "https://x.openai.azure.com/openai/v1/chat/completions"
        );
        assert!(r.headers.contains(&("api-key".into(), "k".into())));
    }

    #[test]
    fn ollama_requests_put_the_schema_in_format() {
        let r = build_request(&model(Provider::Ollama, None), None, &ask());
        assert_eq!(r.url, "http://localhost:11434/api/chat");
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["format"]["type"], "object");
        assert_eq!(b["stream"], false);
        assert_eq!(b["messages"][1]["images"][0], "AQID");
    }

    #[test]
    fn answers_of_all_servers_are_read() {
        let openai = r#"{"choices":[{"message":{"content":"{\"a\":1}"}}],"usage":{"prompt_tokens":11,"completion_tokens":5}}"#;
        let r = parse_response(Provider::Openai, openai).unwrap();
        assert_eq!(r.text, "{\"a\":1}");
        assert_eq!(
            r.usage,
            Usage {
                input_tokens: 11,
                output_tokens: 5
            }
        );

        let anthropic_tool = r#"{"content":[{"type":"tool_use","name":"result","input":{"a":1}}],"usage":{"input_tokens":7,"output_tokens":3}}"#;
        let r = parse_response(Provider::Anthropic, anthropic_tool).unwrap();
        assert_eq!(r.text, "{\"a\":1}");
        assert_eq!(r.usage.input_tokens, 7);
        let anthropic_text =
            r#"{"content":[{"type":"text","text":"he"},{"type":"text","text":"llo"}]}"#;
        assert_eq!(
            parse_response(Provider::Anthropic, anthropic_text)
                .unwrap()
                .text,
            "hello"
        );

        let ollama = r#"{"message":{"role":"assistant","content":"{}"},"prompt_eval_count":20,"eval_count":2}"#;
        let r = parse_response(Provider::Ollama, ollama).unwrap();
        assert_eq!(
            r.usage,
            Usage {
                input_tokens: 20,
                output_tokens: 2
            }
        );

        assert!(parse_response(Provider::Openai, "[]").is_err());
        assert!(parse_response(Provider::Openai, "not json").is_err());
    }

    #[test]
    fn http_errors_carry_the_server_message() {
        let t = Arc::new(MockTransport::new());
        t.push(Ok(crate::transport::Reply {
            status: 401,
            body: r#"{"error":{"message":"bad key"}}"#.into(),
        }));
        let c = Client::new(model(Provider::Openai, None), Some("k".into()), t);
        assert_eq!(
            c.chat(&ask()),
            Err(ModelError::Http {
                status: 401,
                message: "bad key".into()
            })
        );
    }

    #[test]
    fn the_key_comes_from_the_variable_then_the_keychain() {
        use crate::keys::{KeyStore, MemoryKeyStore};
        let keys = MemoryKeyStore::new();
        let t: Arc<dyn Transport> = Arc::new(MockTransport::new());
        // No key needed.
        let m = model(Provider::Ollama, None);
        assert!(Client::connect("m", &m, t.clone(), &keys).is_ok());
        // A keychain model with no key stored is an error that names the entry.
        let mut k = model(Provider::Openai, None);
        k.keychain = true;
        assert_eq!(
            Client::connect("work", &k, t.clone(), &keys).err(),
            Some(ModelError::NoKey("the keychain entry `work`".into()))
        );
        // With the key stored the request carries it.
        keys.set("work", "sk-secret").unwrap();
        let c = Client::connect("work", &k, t.clone(), &keys).unwrap();
        let req = build_request(c.model(), Some("sk-secret"), &ask());
        assert!(req.headers.iter().any(|(_, v)| v.contains("sk-secret")));
        // A variable that is not set does not stop the keychain from being used.
        let mut both = k.clone();
        both.api_key_env = Some("KIRPITCH_TEST_KEY_BOTH".into());
        assert!(Client::connect("work", &both, t, &keys).is_ok());
    }

    #[test]
    fn a_missing_key_variable_is_named() {
        let mut m = model(Provider::Openai, None);
        m.api_key_env = Some("KIRPITCH_TEST_KEY_THAT_IS_NOT_SET".into());
        let t = Arc::new(MockTransport::new());
        assert_eq!(
            Client::from_env(&m, t).err(),
            Some(ModelError::NoKey(
                "KIRPITCH_TEST_KEY_THAT_IS_NOT_SET".into()
            ))
        );
    }
}
