// SPDX-License-Identifier: GPL-3.0-or-later

//! `kirpitch models test`: finds out what a model can do.

use serde_json::{Value, json};

use crate::client::{ChatRequest, Client, ModelError, Usage};
use crate::png::solid_png;
use crate::tasks::json_from_text;

/// What the probe found out about one model.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    /// The model's name in the configuration.
    pub name: String,
    /// The model could be reached at all.
    pub reachable: bool,
    /// It answered a request that has a JSON schema with correct JSON.
    pub structured_json: bool,
    /// It can read images: `Some(true)` when it named the colour of a test
    /// image, `Some(false)` when it could not, `None` when not tested.
    pub vision: Option<bool>,
    /// It found a code word in a text of about 8,000 tokens.
    pub context_8k: Option<bool>,
    /// Seconds for the first (small) request.
    pub seconds: f64,
    /// Tokens used by all probe requests.
    pub usage: Usage,
    /// Problems, one line each.
    pub notes: Vec<String>,
}

fn small(client: &Client, request: ChatRequest) -> Result<(Value, Usage, f64), ModelError> {
    let r = client.chat(&request)?;
    let value = json_from_text(&r.text).map_err(ModelError::Unexpected)?;
    Ok((value, r.usage, r.seconds))
}

fn add(total: &mut Usage, more: Usage) {
    total.input_tokens += more.input_tokens;
    total.output_tokens += more.output_tokens;
}

/// A text of about `words` words with a code word hidden in the middle.
fn needle_text(words: usize) -> String {
    let filler = "The quick brown fox jumps over the lazy dog near the old river bank. ";
    let mut text = String::new();
    for i in 0..words / 14 {
        if i == words / 28 {
            text.push_str("The secret code word is KESTREL-4471. ");
        }
        text.push_str(filler);
    }
    text
}

/// Tests one model.
pub fn probe(name: &str, client: &Client) -> ProbeResult {
    let mut result = ProbeResult {
        name: name.to_string(),
        reachable: false,
        structured_json: false,
        vision: client.model().vision,
        context_8k: None,
        seconds: 0.0,
        usage: Usage::default(),
        notes: vec![],
    };
    let schema = json!({
        "type": "object",
        "properties": {"answer": {"type": "integer"}},
        "required": ["answer"]
    });
    let ask = ChatRequest {
        system: "Answer only with JSON.".into(),
        user: "What is 2 + 3? Answer as {\"answer\": <number>}.".into(),
        images: vec![],
        schema: Some(schema),
        max_tokens: 100,
        ..Default::default()
    };
    match small(client, ask) {
        Ok((v, usage, seconds)) => {
            result.reachable = true;
            result.seconds = seconds;
            add(&mut result.usage, usage);
            result.structured_json = v.get("answer").and_then(Value::as_i64) == Some(5);
            if !result.structured_json {
                result.notes.push(format!("the JSON answer was wrong: {v}"));
            }
        }
        Err(e) => {
            result.notes.push(e.to_string());
            return result;
        }
    }
    if result.vision.is_none() {
        let colour = json!({
            "type": "object",
            "properties": {"colour": {"type": "string"}},
            "required": ["colour"]
        });
        let ask = ChatRequest {
            system: "Answer only with JSON.".into(),
            user: "What colour is the whole image? Answer as {\"colour\": \"<name>\"}.".into(),
            images: vec![solid_png(32, [255, 0, 0])],
            schema: Some(colour),
            max_tokens: 100,
            ..Default::default()
        };
        match small(client, ask) {
            Ok((v, usage, _)) => {
                add(&mut result.usage, usage);
                let said = v
                    .get("colour")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                result.vision = Some(said.contains("red"));
            }
            Err(ModelError::Http { .. }) => result.vision = Some(false),
            Err(e) => result.notes.push(format!("vision test: {e}")),
        }
    }
    let ask = ChatRequest {
        system: "Answer only with JSON.".into(),
        user: format!(
            "{}\n\nWhat is the secret code word in the text above? Answer as {{\"code\": \"<word>\"}}.",
            needle_text(5_500)
        ),
        images: vec![],
        schema: Some(json!({
            "type": "object",
            "properties": {"code": {"type": "string"}},
            "required": ["code"]
        })),
        max_tokens: 100,
        ..Default::default()
    };
    match small(client, ask) {
        Ok((v, usage, _)) => {
            add(&mut result.usage, usage);
            result.context_8k = Some(
                v.get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.contains("KESTREL-4471")),
            );
        }
        Err(ModelError::Http { .. }) => result.context_8k = Some(false),
        Err(e) => result.notes.push(format!("context test: {e}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_needle_text_is_long_and_has_the_code_once() {
        let t = needle_text(5_500);
        assert!(t.split_whitespace().count() > 5_000);
        assert_eq!(t.matches("KESTREL-4471").count(), 1);
    }
}
