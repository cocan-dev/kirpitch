// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding out which models a server offers, and what they cost.
//!
//! Every kind of server lists its models (`/v1/models`, `/api/tags`). Only some
//! of them say what a model costs or what it can read: OpenRouter publishes
//! prices, context sizes and image support, Mistral publishes image support
//! and context sizes. For the others (Anthropic, OpenAI, Gemini) the prices
//! are looked up, best effort, in OpenRouter's public list by model name; the
//! result says where each price came from so the screen can tell the user to
//! check it. Nothing here sends any datasheet content: it only asks for lists.

use serde::Serialize;
use serde_json::Value;

use crate::client::ModelError;
use crate::config::Provider;
use crate::transport::{Request, Transport};

/// Seconds to wait for a list.
const TIMEOUT_SECS: u64 = 20;

/// The public OpenRouter list, used for reference prices.
pub const OPENROUTER_LIST: &str = "https://openrouter.ai/api/v1/models";

/// One model a server offers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FoundModel {
    /// The name to put in the configuration.
    pub id: String,
    /// A friendlier name, when the server gives one.
    pub label: Option<String>,
    /// Price of one million input tokens, in US dollars.
    pub price_in: Option<f64>,
    /// Price of one million output tokens, in US dollars.
    pub price_out: Option<f64>,
    /// Where the prices come from: `"the server"`, `"OpenRouter list"`; empty
    /// when there is no price.
    pub price_source: Option<String>,
    /// Size of the context window in tokens.
    pub context: Option<u64>,
    /// Whether the model reads images, when the server says.
    pub vision: Option<bool>,
}

/// What a list of models needs to be asked for.
#[derive(Debug, Clone)]
pub struct ListRequest<'a> {
    /// The kind of server.
    pub provider: Provider,
    /// The address, without a trailing slash; the provider's usual one when
    /// empty.
    pub base_url: &'a str,
    /// The API key, if the server wants one.
    pub key: Option<&'a str>,
    /// The header that carries the key when it is not the usual one.
    pub key_header: Option<&'a str>,
}

fn listing_url(provider: Provider, base: &str) -> String {
    match provider {
        Provider::Anthropic => format!("{base}/v1/models?limit=1000"),
        Provider::Ollama => format!("{base}/api/tags"),
        Provider::Openai | Provider::OpenaiCompatible => {
            if base.ends_with("/v1") || base.contains("/openai") || base.contains("/v1beta") {
                format!("{base}/models")
            } else {
                format!("{base}/v1/models")
            }
        }
    }
}

fn headers(req: &ListRequest<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    match req.provider {
        Provider::Anthropic => {
            if let Some(k) = req.key {
                out.push((
                    req.key_header.unwrap_or("x-api-key").to_string(),
                    k.to_string(),
                ));
            }
            out.push(("anthropic-version".into(), "2023-06-01".into()));
        }
        _ => {
            if let Some(k) = req.key {
                match req.key_header {
                    Some(h) => out.push((h.to_string(), k.to_string())),
                    None => out.push(("Authorization".into(), format!("Bearer {k}"))),
                }
            }
        }
    }
    out
}

fn get(
    transport: &dyn Transport,
    url: String,
    headers: Vec<(String, String)>,
) -> Result<Value, ModelError> {
    let reply = transport.get(
        &Request {
            url,
            headers,
            body: String::new(),
        },
        TIMEOUT_SECS,
    )?;
    if !(200..300).contains(&reply.status) {
        let message = serde_json::from_str::<Value>(&reply.body)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or_else(|| v.get("error"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| reply.body.chars().take(200).collect());
        return Err(ModelError::Http {
            status: reply.status,
            message,
        });
    }
    serde_json::from_str(&reply.body)
        .map_err(|e| ModelError::Unexpected(format!("the list is not JSON: {e}")))
}

/// Asks the server which models it offers.
///
/// # Errors
///
/// The server cannot be reached, refuses the key, or answers something that is
/// not a list of models.
pub fn list_models(
    req: &ListRequest<'_>,
    transport: &dyn Transport,
) -> Result<Vec<FoundModel>, ModelError> {
    let base = if req.base_url.is_empty() {
        req.provider.default_base_url().unwrap_or("")
    } else {
        req.base_url
    }
    .trim_end_matches('/');
    if base.is_empty() {
        return Err(ModelError::Unexpected(
            "this kind of server needs an address first".into(),
        ));
    }
    let value = get(transport, listing_url(req.provider, base), headers(req))?;
    let mut found = match req.provider {
        Provider::Ollama => parse_ollama(&value)?,
        Provider::Anthropic => parse_anthropic(&value)?,
        Provider::Openai => parse_openai(&value, true)?,
        Provider::OpenaiCompatible => parse_openai(&value, false)?,
    };
    // Reference prices from OpenRouter for servers that do not publish any.
    // A failure here only means there are no prices.
    let wants_prices = !matches!(req.provider, Provider::Ollama)
        && !base.contains("openrouter.ai")
        && !is_local_like(base)
        && found.iter().any(|m| m.price_in.is_none());
    if wants_prices
        && let Ok(list) = get(transport, OPENROUTER_LIST.to_string(), Vec::new())
        && let Ok(reference) = parse_openai(&list, false)
    {
        fill_prices(&mut found, &reference);
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found.dedup_by(|a, b| a.id == b.id);
    Ok(found)
}

fn is_local_like(base: &str) -> bool {
    crate::config::is_local_url(base)
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, ModelError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| ModelError::Unexpected(format!("the answer has no `{key}` list")))
}

fn parse_anthropic(value: &Value) -> Result<Vec<FoundModel>, ModelError> {
    Ok(array(value, "data")?
        .iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            Some(FoundModel {
                label: m
                    .get("display_name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                id,
                ..FoundModel::default()
            })
        })
        .collect())
}

fn parse_ollama(value: &Value) -> Result<Vec<FoundModel>, ModelError> {
    Ok(array(value, "models")?
        .iter()
        .filter_map(|m| {
            let id = m
                .get("model")
                .or_else(|| m.get("name"))?
                .as_str()?
                .to_string();
            Some(FoundModel {
                id,
                ..FoundModel::default()
            })
        })
        .collect())
}

/// Ids of models that do not chat (OpenAI lists everything it sells).
fn not_for_chat(id: &str) -> bool {
    const SKIP: [&str; 14] = [
        "embedding",
        "whisper",
        "tts",
        "dall-e",
        "moderation",
        "davinci",
        "babbage",
        "audio",
        "realtime",
        "transcribe",
        "image",
        "search",
        "computer-use",
        "codex",
    ];
    SKIP.iter().any(|s| id.contains(s))
}

fn per_million(value: Option<&Value>) -> Option<f64> {
    let per_token = match value? {
        Value::String(s) => s.parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    // Negative prices mean "varies" on OpenRouter.
    (per_token >= 0.0).then(|| (per_token * 1e6 * 1e4).round() / 1e4)
}

fn parse_openai(value: &Value, filter_chat: bool) -> Result<Vec<FoundModel>, ModelError> {
    Ok(array(value, "data")?
        .iter()
        .filter_map(|m| {
            let raw = m.get("id")?.as_str()?;
            // Gemini lists `models/gemini-...`.
            let id = raw.strip_prefix("models/").unwrap_or(raw).to_string();
            if filter_chat && not_for_chat(&id) {
                return None;
            }
            let price_in = per_million(m.pointer("/pricing/prompt"));
            let price_out = per_million(m.pointer("/pricing/completion"));
            let context = m
                .get("context_length")
                .or_else(|| m.get("max_context_length"))
                .and_then(Value::as_u64);
            let vision = m
                .pointer("/architecture/input_modalities")
                .and_then(Value::as_array)
                .map(|a| a.iter().any(|v| v.as_str() == Some("image")))
                .or_else(|| m.pointer("/capabilities/vision").and_then(Value::as_bool));
            let priced = price_in.is_some() && price_out.is_some();
            Some(FoundModel {
                label: m.get("name").and_then(Value::as_str).map(str::to_string),
                id,
                price_in,
                price_out,
                price_source: priced.then(|| "the server".to_string()),
                context,
                vision,
            })
        })
        .collect())
}

/// Makes a name comparable: lower case, no vendor prefix, dots as dashes, no
/// date or `-latest` suffix.
fn normal(id: &str) -> String {
    let name = id.rsplit('/').next().unwrap_or(id).to_lowercase();
    let name = name.replace('.', "-");
    let name = name.strip_suffix("-latest").unwrap_or(&name).to_string();
    let parts: Vec<&str> = name.split('-').collect();
    // A trailing 8 digit date (20250929) or 4 digit month (2411).
    let keep = match parts.last() {
        Some(last)
            if parts.len() > 1
                && last.chars().all(|c| c.is_ascii_digit())
                && (last.len() == 8 || last.len() == 4) =>
        {
            parts.len() - 1
        }
        _ => parts.len(),
    };
    parts[..keep].join("-")
}

fn fill_prices(found: &mut [FoundModel], reference: &[FoundModel]) {
    for m in found.iter_mut().filter(|m| m.price_in.is_none()) {
        let key = normal(&m.id);
        if let Some(r) = reference
            .iter()
            .find(|r| normal(&r.id) == key && r.price_in.is_some() && r.price_out.is_some())
        {
            m.price_in = r.price_in;
            m.price_out = r.price_out;
            m.price_source = Some("OpenRouter list".to_string());
            m.context = m.context.or(r.context);
            m.vision = m.vision.or(r.vision);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    fn request(provider: Provider, base: &str) -> ListRequest<'_> {
        ListRequest {
            provider,
            base_url: base,
            key: Some("k"),
            key_header: None,
        }
    }

    #[test]
    fn an_ollama_list_is_read() {
        let m = MockTransport::new();
        m.push_ok(
            r#"{"models":[{"name":"qwen3:8b","model":"qwen3:8b"},{"name":"gpt-oss:120b-cloud"}]}"#,
        );
        let found = list_models(&request(Provider::Ollama, "https://ollama.com"), &m).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "gpt-oss:120b-cloud");
        let sent = &m.requests()[0];
        assert_eq!(sent.url, "https://ollama.com/api/tags");
        assert_eq!(sent.headers[0].1, "Bearer k");
        // No price lookups for Ollama.
        assert_eq!(m.requests().len(), 1);
    }

    #[test]
    fn openrouter_prices_are_per_million_tokens() {
        let m = MockTransport::new();
        m.push_ok(
            r#"{"data":[{"id":"anthropic/claude-sonnet-4.5","name":"Claude Sonnet 4.5",
            "context_length":200000,"pricing":{"prompt":"0.000003","completion":"0.000015"},
            "architecture":{"input_modalities":["text","image"]}},
            {"id":"x/free","pricing":{"prompt":"0","completion":"0"}},
            {"id":"x/varies","pricing":{"prompt":"-1","completion":"-1"}}]}"#,
        );
        let found = list_models(
            &request(Provider::OpenaiCompatible, "https://openrouter.ai/api/v1"),
            &m,
        )
        .unwrap();
        let s = found.iter().find(|f| f.id.contains("sonnet")).unwrap();
        assert_eq!(s.price_in, Some(3.0));
        assert_eq!(s.price_out, Some(15.0));
        assert_eq!(s.context, Some(200_000));
        assert_eq!(s.vision, Some(true));
        assert_eq!(s.price_source.as_deref(), Some("the server"));
        let free = found.iter().find(|f| f.id == "x/free").unwrap();
        assert_eq!(free.price_in, Some(0.0));
        let varies = found.iter().find(|f| f.id == "x/varies").unwrap();
        assert_eq!(varies.price_in, None);
        assert_eq!(m.requests()[0].url, "https://openrouter.ai/api/v1/models");
        // OpenRouter is its own price source: no second request.
        assert_eq!(m.requests().len(), 1);
    }

    #[test]
    fn anthropic_prices_come_from_the_openrouter_list_by_name() {
        let m = MockTransport::new();
        m.push_ok(
            r#"{"data":[{"id":"claude-sonnet-4-5-20250929","display_name":"Claude Sonnet 4.5"},
            {"id":"claude-unknown-9"}]}"#,
        );
        m.push_ok(
            r#"{"data":[{"id":"anthropic/claude-sonnet-4.5","pricing":{"prompt":"0.000003","completion":"0.000015"}}]}"#,
        );
        let found = list_models(&request(Provider::Anthropic, ""), &m).unwrap();
        let s = found
            .iter()
            .find(|f| f.id == "claude-sonnet-4-5-20250929")
            .unwrap();
        assert_eq!(s.label.as_deref(), Some("Claude Sonnet 4.5"));
        assert_eq!(s.price_in, Some(3.0));
        assert_eq!(s.price_source.as_deref(), Some("OpenRouter list"));
        assert_eq!(
            found
                .iter()
                .find(|f| f.id == "claude-unknown-9")
                .unwrap()
                .price_in,
            None
        );
        let reqs = m.requests();
        assert_eq!(
            reqs[0].url,
            "https://api.anthropic.com/v1/models?limit=1000"
        );
        assert!(
            reqs[0]
                .headers
                .iter()
                .any(|(k, v)| k == "x-api-key" && v == "k")
        );
        assert!(
            reqs[1].headers.is_empty(),
            "the key must not go to OpenRouter"
        );
    }

    #[test]
    fn a_failed_price_lookup_still_gives_the_models() {
        let m = MockTransport::new();
        m.push_ok(r#"{"data":[{"id":"gpt-4o"}]}"#);
        // No second answer: the lookup fails.
        let found = list_models(&request(Provider::Openai, ""), &m).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].price_in, None);
    }

    #[test]
    fn openai_lists_only_chat_models_and_gemini_loses_its_prefix() {
        let m = MockTransport::new();
        m.push_ok(
            r#"{"data":[{"id":"gpt-4o"},{"id":"text-embedding-3-small"},{"id":"whisper-1"},{"id":"tts-1"}]}"#,
        );
        m.push_ok(r#"{"data":[]}"#);
        let found = list_models(&request(Provider::Openai, ""), &m).unwrap();
        assert_eq!(
            found.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["gpt-4o"]
        );
        let m = MockTransport::new();
        m.push_ok(r#"{"data":[{"id":"models/gemini-2.5-pro"}]}"#);
        m.push_ok(r#"{"data":[]}"#);
        let found = list_models(
            &request(
                Provider::OpenaiCompatible,
                "https://generativelanguage.googleapis.com/v1beta/openai",
            ),
            &m,
        )
        .unwrap();
        assert_eq!(found[0].id, "gemini-2.5-pro");
        assert_eq!(
            m.requests()[0].url,
            "https://generativelanguage.googleapis.com/v1beta/openai/models"
        );
    }

    #[test]
    fn a_local_server_is_not_asked_for_prices() {
        let m = MockTransport::new();
        m.push_ok(r#"{"data":[{"id":"qwen2.5-vl"}]}"#);
        let found = list_models(
            &request(Provider::OpenaiCompatible, "http://localhost:1234/v1"),
            &m,
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(m.requests().len(), 1);
        assert_eq!(m.requests()[0].url, "http://localhost:1234/v1/models");
    }

    #[test]
    fn errors_say_what_the_server_said() {
        let m = MockTransport::new();
        m.push(Ok(crate::transport::Reply {
            status: 401,
            body: r#"{"error":{"message":"invalid x-api-key"}}"#.into(),
        }));
        let e = list_models(&request(Provider::Anthropic, ""), &m).unwrap_err();
        assert!(e.to_string().contains("401"), "{e}");
        assert!(e.to_string().contains("invalid x-api-key"), "{e}");
        let e = list_models(&request(Provider::OpenaiCompatible, ""), &m).unwrap_err();
        assert!(e.to_string().contains("address"), "{e}");
    }

    #[test]
    fn names_are_made_comparable() {
        assert_eq!(normal("anthropic/claude-sonnet-4.5"), "claude-sonnet-4-5");
        assert_eq!(normal("claude-sonnet-4-5-20250929"), "claude-sonnet-4-5");
        assert_eq!(normal("mistral-large-latest"), "mistral-large");
        assert_eq!(normal("mistral-large-2411"), "mistral-large");
        assert_eq!(normal("gpt-4"), "gpt-4");
    }
}
