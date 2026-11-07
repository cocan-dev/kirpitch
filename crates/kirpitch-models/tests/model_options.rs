// SPDX-License-Identifier: GPL-3.0-or-later

//! The options of a model that shape its requests: how hard it thinks, the
//! context window, the temperature and extra fields.

use kirpitch_models::client::{ChatRequest, auto_context, build_request};
use kirpitch_models::config::{Config, ModelConfig};
use serde_json::{Value, json};

fn model(toml: &str) -> ModelConfig {
    let text = format!("[models.m]\n{toml}\n");
    Config::from_toml_str(&text)
        .unwrap()
        .models
        .get("m")
        .unwrap()
        .clone()
}

fn chat(user: &str, max_tokens: u32) -> ChatRequest {
    ChatRequest {
        system: "system".into(),
        user: user.into(),
        max_tokens,
        ..Default::default()
    }
}

fn body(m: &ModelConfig, c: &ChatRequest) -> Value {
    serde_json::from_str(&build_request(m, Some("k"), c).body).unwrap()
}

#[test]
fn an_openai_style_server_gets_the_effort_and_the_temperature_it_is_given() {
    let plain = model(
        "provider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"t\"",
    );
    let b = body(&plain, &chat("q", 100));
    assert_eq!(b["temperature"], json!(0));
    assert!(b.get("reasoning_effort").is_none());

    let reasoning = model(
        "provider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"t\"\neffort = \"high\"",
    );
    let b = body(&reasoning, &chat("q", 100));
    assert_eq!(b["reasoning_effort"], json!("high"));
    // A reasoning model refuses a temperature unless one was asked for.
    assert!(b.get("temperature").is_none());
    assert_eq!(b["max_tokens"], json!(100));

    let hot = model(
        "provider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"t\"\ntemperature = 0.7",
    );
    assert_eq!(body(&hot, &chat("q", 100))["temperature"], json!(0.7));
}

#[test]
fn openai_itself_counts_the_thinking_in_max_completion_tokens() {
    let m = model("provider = \"openai\"\nmodel = \"gpt-5\"\neffort = \"low\"");
    let b = body(&m, &chat("q", 500));
    assert_eq!(b["reasoning_effort"], json!("low"));
    assert_eq!(b["max_completion_tokens"], json!(500));
    assert!(b.get("max_tokens").is_none());
}

#[test]
fn ollama_gets_the_context_it_needs_and_the_thinking_it_is_told() {
    let m = model("provider = \"ollama\"\nmodel = \"qwen3\"");
    let small = body(&m, &chat("short", 1000));
    assert_eq!(small["options"]["num_ctx"], json!(4096));
    // A long question (page text) and a big answer limit need more room: the
    // default of the server would cut the text off.
    let long = "x".repeat(60_000);
    let big = body(&m, &chat(&long, 8000));
    assert_eq!(big["options"]["num_ctx"], json!(32768), "{big}");
    assert_eq!(auto_context(&chat(&long, 8000)), 32768);
    // The person's own value is used as it is.
    let set = model("provider = \"ollama\"\nmodel = \"qwen3\"\ncontext_length = 20000");
    assert_eq!(
        body(&set, &chat(&long, 8000))["options"]["num_ctx"],
        json!(20000)
    );

    // Thinking: not sent, the switch, or a level.
    assert!(small.get("think").is_none());
    let off = model("provider = \"ollama\"\nmodel = \"q\"\nthink = false");
    assert_eq!(body(&off, &chat("q", 100))["think"], json!(false));
    let high = model("provider = \"ollama\"\nmodel = \"q\"\neffort = \"high\"");
    assert_eq!(body(&high, &chat("q", 100))["think"], json!("high"));
    let none = model("provider = \"ollama\"\nmodel = \"q\"\neffort = \"none\"");
    assert_eq!(body(&none, &chat("q", 100))["think"], json!(false));
    let minimal = model("provider = \"ollama\"\nmodel = \"q\"\neffort = \"minimal\"");
    assert_eq!(body(&minimal, &chat("q", 100))["think"], json!("low"));
    // The retry after an empty answer turns thinking off whatever was set.
    let mut retry = chat("q", 100);
    retry.no_think = true;
    assert_eq!(body(&high, &retry)["think"], json!(false));
}

#[test]
fn anthropic_extended_thinking_has_a_budget_and_no_forced_tool() {
    let plain = model("provider = \"anthropic\"\nmodel = \"claude-sonnet\"");
    let mut with_schema = chat("q", 1000);
    with_schema.schema = Some(json!({"type": "object"}));
    let b = body(&plain, &with_schema);
    assert_eq!(b["temperature"].as_f64(), Some(0.0));
    assert_eq!(b["tool_choice"]["name"], json!("result"));
    assert!(b.get("thinking").is_none());

    let thinking =
        model("provider = \"anthropic\"\nmodel = \"claude-sonnet\"\neffort = \"medium\"");
    let b = body(&thinking, &with_schema);
    assert_eq!(
        b["thinking"],
        json!({"type": "enabled", "budget_tokens": 8192})
    );
    // The thinking counts in max_tokens, so the answer keeps its own room.
    assert_eq!(b["max_tokens"], json!(1000 + 8192));
    assert!(b.get("tool_choice").is_none() && b.get("tools").is_none());
    assert!(b.get("temperature").is_none());
    // No effort, "none": no thinking.
    let none = model("provider = \"anthropic\"\nmodel = \"claude-sonnet\"\neffort = \"none\"");
    assert!(body(&none, &with_schema).get("thinking").is_none());
}

#[test]
fn extra_fields_are_added_and_win_but_cannot_replace_the_question() {
    let m = model(
        "provider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"t\"\nextra_body = '{\"top_p\": 0.9, \"temperature\": 0.2}'",
    );
    let b = body(&m, &chat("q", 100));
    assert_eq!(b["top_p"], json!(0.9));
    assert_eq!(b["temperature"], json!(0.2), "the person's field wins");
    assert_eq!(b["messages"][1]["content"][0]["text"], json!("q"));
}

#[test]
fn options_that_make_no_sense_are_refused_with_the_model_named() {
    let bad = |extra: &str| {
        Config::from_toml_str(&format!(
            "[models.m]\nprovider = \"ollama\"\nmodel = \"t\"\n{extra}\n"
        ))
        .unwrap_err()
        .to_string()
    };
    assert!(bad("effort = \"extreme\"").contains("effort"));
    assert!(bad("effort = \"extreme\"").contains("`m`"));
    assert!(bad("context_length = 10").contains("context length"));
    assert!(bad("temperature = 5.0").contains("temperature"));
    assert!(bad("extra_body = 'not json'").contains("not JSON"));
    assert!(bad("extra_body = '[1]'").contains("JSON object"));
    assert!(bad("extra_body = '{\"model\": \"x\"}'").contains("cannot set"));
}

#[test]
fn the_options_are_saved_in_the_file_only_when_set() {
    let plain =
        Config::from_toml_str("[models.m]\nprovider = \"ollama\"\nmodel = \"t\"\n").unwrap();
    let text = plain.to_toml_string().unwrap();
    for key in ["effort", "context_length", "temperature", "extra_body"] {
        assert!(!text.contains(key), "{text}");
    }
    let set = Config::from_toml_str(
        "[models.m]\nprovider = \"ollama\"\nmodel = \"t\"\neffort = \"low\"\ncontext_length = 16384\ntemperature = 0.3\nextra_body = '{\"top_k\": 40}'\n",
    )
    .unwrap();
    let back = Config::from_toml_str(&set.to_toml_string().unwrap()).unwrap();
    assert_eq!(back, set);
}

#[test]
fn an_ollama_address_left_with_v1_still_reaches_the_ollama_api() {
    // The address of the OpenAI-compatible interface was kept when the kind was
    // changed to Ollama: the request must not go to .../v1/api/chat.
    let m = model(
        "provider = \"ollama\"
base_url = \"https://ollama.com/v1\"
model = \"t\"",
    );
    let r = build_request(&m, Some("k"), &chat("q", 100));
    assert_eq!(r.url, "https://ollama.com/api/chat");
    for url in [
        "https://ollama.com/v1/",
        "https://ollama.com/api",
        "https://ollama.com/",
    ] {
        let m = model(&format!(
            "provider = \"ollama\"
base_url = \"{url}\"
model = \"t\""
        ));
        assert_eq!(
            build_request(&m, None, &chat("q", 100)).url,
            "https://ollama.com/api/chat",
            "{url}"
        );
    }
    // Other kinds keep their address as it is.
    let o = model(
        "provider = \"openai-compatible\"
base_url = \"https://ollama.com/v1\"
model = \"t\"",
    );
    assert_eq!(
        build_request(&o, None, &chat("q", 100)).url,
        "https://ollama.com/v1/chat/completions"
    );
}

#[test]
fn the_model_list_and_details_of_an_ollama_kind_also_ignore_a_stray_v1() {
    use kirpitch_models::config::Provider;
    use kirpitch_models::{ListRequest, MockTransport, list_models, model_details};
    let req = ListRequest {
        provider: Provider::Ollama,
        base_url: "https://ollama.com/v1",
        key: Some("k"),
        key_header: None,
    };
    let t = MockTransport::new();
    t.push_ok(r#"{"models":[{"name":"m"}]}"#);
    list_models(&req, &t).unwrap();
    assert_eq!(t.requests()[0].url, "https://ollama.com/api/tags");
    let t = MockTransport::new();
    t.push_ok(r#"{"model_info":{"x.context_length":4096}}"#);
    model_details(&req, "m", &t).unwrap();
    assert_eq!(t.requests()[0].url, "https://ollama.com/api/show");
}
