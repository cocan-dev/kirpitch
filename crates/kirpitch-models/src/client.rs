// SPDX-License-Identifier: GPL-3.0-or-later

//! One chat request to one model, for each kind of server.
//!
//! The request builders and answer readers are plain functions so they can be
//! tested with recorded answers.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};

use crate::config::{ModelConfig, Provider, base_url, resolve_model_name};
use crate::png::base64;
use crate::transport::{Request, Transport, TransportError};

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
    /// JSON schema the answer must follow, when the answer is JSON.
    pub schema: Option<Value>,
    /// Longest answer in tokens.
    pub max_tokens: u32,
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
        let reply = self.transport.post(&http, TIMEOUT_SECS)?;
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
            (format!("{base}/v1/messages"), anthropic_body(name, chat))
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
            (format!("{base}{path}"), openai_body(name, chat))
        }
        Provider::Ollama => {
            if let Some(k) = key {
                headers.push(("Authorization".into(), format!("Bearer {k}")));
            }
            (format!("{base}/api/chat"), ollama_body(name, chat))
        }
    };
    Request {
        url,
        headers,
        body: body.to_string(),
    }
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png))
}

fn openai_body(model: &str, chat: &ChatRequest) -> Value {
    let mut content = vec![json!({"type": "text", "text": chat.user})];
    for image in &chat.images {
        content.push(json!({"type": "image_url", "image_url": {"url": data_url(image)}}));
    }
    let mut body = json!({
        "model": model,
        "temperature": 0,
        "max_tokens": chat.max_tokens,
        "messages": [
            {"role": "system", "content": chat.system},
            {"role": "user", "content": content},
        ],
    });
    if let Some(schema) = &chat.schema {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {"name": "result", "schema": schema},
        });
    }
    body
}

fn anthropic_body(model: &str, chat: &ChatRequest) -> Value {
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
        "temperature": 0,
        "max_tokens": chat.max_tokens,
        "system": chat.system,
        "messages": [{"role": "user", "content": content}],
    });
    if let Some(schema) = &chat.schema {
        body["tools"] = json!([{
            "name": "result",
            "description": "Report the result.",
            "input_schema": schema,
        }]);
        body["tool_choice"] = json!({"type": "tool", "name": "result"});
    }
    body
}

fn ollama_body(model: &str, chat: &ChatRequest) -> Value {
    let images: Vec<String> = chat.images.iter().map(|i| base64(i)).collect();
    let mut user = json!({"role": "user", "content": chat.user});
    if !images.is_empty() {
        user["images"] = json!(images);
    }
    let mut body = json!({
        "model": model,
        "stream": false,
        "options": {"temperature": 0, "num_predict": chat.max_tokens},
        "messages": [{"role": "system", "content": chat.system}, user],
    });
    if let Some(schema) = &chat.schema {
        body["format"] = schema.clone();
    }
    body
}

/// Reads the answer of a server.
pub fn parse_response(provider: Provider, body: &str) -> Result<ChatResponse, ModelError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| ModelError::Unexpected(format!("not JSON ({e})")))?;
    let unexpected = |what: &str| ModelError::Unexpected(format!("no {what}"));
    let count = |p: &str| v.pointer(p).and_then(Value::as_u64).unwrap_or(0);
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
    Ok(ChatResponse {
        text,
        usage,
        seconds: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    fn model(provider: Provider, base_url: Option<&str>) -> ModelConfig {
        ModelConfig {
            provider,
            model: "claude-sonnet".into(),
            base_url: base_url.map(str::to_string),
            api_key_env: None,
            api_key_header: None,
            vision: None,
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
