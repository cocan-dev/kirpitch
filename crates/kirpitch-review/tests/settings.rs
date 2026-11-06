// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings screen: models, keys, tasks and folders.

use std::fs;
use std::sync::Arc;

use kirpitch_kicad::testing::TempDir;
use kirpitch_models::{KeyStore, MemoryKeyStore, MockTransport};
use kirpitch_review::{
    FindInput, ModelInput, SettingsInput, delete_api_key, find_models, save_settings, set_api_key,
    settings_view, test_model,
};
use serde_json::json;

fn model(name: &str, provider: &str) -> ModelInput {
    serde_json::from_value(json!({
        "name": name, "provider": provider, "model": "some-model",
    }))
    .unwrap()
}

fn input(models: Vec<ModelInput>) -> SettingsInput {
    serde_json::from_value(json!({ "models": [] }))
        .map(|mut s: SettingsInput| {
            s.models = models;
            s
        })
        .unwrap()
}

#[test]
fn a_missing_file_is_an_empty_configuration_with_the_providers() {
    let tmp = TempDir::new("settings-empty");
    let v = settings_view(&tmp.path().join("kirpitch.toml"), &MemoryKeyStore::new());
    assert!(!v.exists && v.models.is_empty() && v.config_error.is_none());
    let ids: Vec<&str> = v.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["anthropic", "openai", "openai-compatible", "ollama"]);
    assert_eq!(v.providers[3].default_url, "http://localhost:11434");
}

#[test]
fn models_tasks_and_folders_are_saved_and_read_back() {
    let tmp = TempDir::new("settings-save");
    let path = tmp.path().join("nested/kirpitch.toml");
    let keys = MemoryKeyStore::new();
    let mut local = model("local", "ollama");
    local.think = Some(false);
    let mut cloud = model("cloud", "anthropic");
    cloud.keychain = true;
    cloud.price_in = Some(3.0);
    let mut s = input(vec![local, cloud]);
    s.pin_tables = "local".into();
    s.drawings = "cloud".into();
    s.local_only = false;
    s.parts = "C:/parts".into();
    s.kicad_target = "9".into();
    let v = save_settings(&path, &s, &keys).unwrap();
    assert!(v.exists);
    assert_eq!(v.models.len(), 2);
    let cloud = v.models.iter().find(|m| m.name == "cloud").unwrap();
    assert!(cloud.keychain && !cloud.has_key && !cloud.local);
    assert_eq!(cloud.effective_url, "https://api.anthropic.com");
    let local = v.models.iter().find(|m| m.name == "local").unwrap();
    assert!(local.local && local.think == Some(false));
    assert_eq!(
        (v.pin_tables.as_str(), v.drawings.as_str()),
        ("local", "cloud")
    );
    assert_eq!(
        (v.parts.as_str(), v.kicad_target.as_str()),
        ("C:/parts", "9")
    );
    // The command line reads the same file.
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("[models.cloud]") && text.contains("keychain = true"));
}

#[test]
fn keys_live_in_the_keychain_and_never_in_the_file() {
    let tmp = TempDir::new("settings-keys");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::new();
    let mut cloud = model("cloud", "openai");
    cloud.keychain = true;
    save_settings(&path, &input(vec![cloud]), &keys).unwrap();
    let v = set_api_key(&path, "cloud", "sk-very-secret", &keys).unwrap();
    assert!(v.models[0].has_key);
    assert_eq!(
        keys.get("cloud").unwrap().as_deref(),
        Some("sk-very-secret")
    );
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains("sk-very-secret")
    );
    // A blank key, an unknown model and a model without a keychain are refused.
    assert!(set_api_key(&path, "cloud", "  ", &keys).is_err());
    assert!(
        set_api_key(&path, "nope", "k", &keys)
            .unwrap_err()
            .contains("save the model")
    );
    save_settings(&path, &input(vec![model("plain", "ollama")]), &keys).unwrap();
    assert!(
        set_api_key(&path, "plain", "k", &keys)
            .unwrap_err()
            .contains("not set to keep")
    );
    // Removing a model removes its key.
    assert_eq!(keys.get("cloud").unwrap(), None);
}

#[test]
fn deleting_a_key_and_renaming_a_model_keep_the_keychain_tidy() {
    let tmp = TempDir::new("settings-rename");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::new();
    let mut m = model("first", "openai");
    m.keychain = true;
    save_settings(&path, &input(vec![m.clone()]), &keys).unwrap();
    set_api_key(&path, "first", "k1", &keys).unwrap();
    // Rename: the key follows.
    let mut renamed = m.clone();
    renamed.old_name = Some("first".into());
    renamed.name = "second".into();
    save_settings(&path, &input(vec![renamed]), &keys).unwrap();
    assert_eq!(keys.get("second").unwrap().as_deref(), Some("k1"));
    assert_eq!(keys.get("first").unwrap(), None);
    // Delete the key only.
    let v = delete_api_key(&path, "second", &keys).unwrap();
    assert!(!v.models[0].has_key);
}

#[test]
fn the_form_is_checked_before_anything_is_written() {
    let tmp = TempDir::new("settings-bad");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::new();
    let err = |s: SettingsInput| save_settings(&path, &s, &keys).unwrap_err();
    assert!(err(input(vec![model("bad name", "ollama")])).contains("not a good model name"));
    assert!(err(input(vec![model("a", "ollama"), model("a", "openai")])).contains("two models"));
    assert!(err(input(vec![model("a", "carrier-pigeon")])).contains("unknown kind"));
    let mut no_model = model("a", "ollama");
    no_model.model = " ".into();
    assert!(err(input(vec![no_model])).contains("needs a model name"));
    let mut url = model("a", "openai-compatible");
    url.base_url = "ftp://x".into();
    assert!(err(input(vec![url])).contains("http://"));
    let mut compat = model("a", "openai-compatible");
    compat.base_url = String::new();
    assert!(err(input(vec![compat])).contains("base_url"));
    let mut task = input(vec![model("a", "ollama")]);
    task.pin_tables = "missing".into();
    assert!(err(task).contains("missing"));
    let mut target = input(vec![]);
    target.kicad_target = "7".into();
    assert!(err(target).contains("8, 9 or 10"));
    assert!(!path.exists());
}

#[test]
fn a_broken_file_is_reported_and_can_be_replaced() {
    let tmp = TempDir::new("settings-broken");
    let path = tmp.path().join("kirpitch.toml");
    fs::write(&path, "this is not toml [[[").unwrap();
    let keys = MemoryKeyStore::new();
    let v = settings_view(&path, &keys);
    assert!(v.config_error.is_some() && v.models.is_empty());
    let v = save_settings(&path, &input(vec![model("a", "ollama")]), &keys).unwrap();
    assert!(v.config_error.is_none() && v.models.len() == 1);
}

fn reply(v: &serde_json::Value) -> String {
    json!({"message": {"content": v.to_string()}, "prompt_eval_count": 5, "eval_count": 2})
        .to_string()
}

#[test]
fn a_saved_model_can_be_tested_and_a_cloud_model_is_refused_in_local_only_mode() {
    let tmp = TempDir::new("settings-test");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::new();
    save_settings(&path, &input(vec![model("local", "ollama")]), &keys).unwrap();
    let t = Arc::new(MockTransport::new());
    t.push_ok(&reply(&json!({"answer": 5})));
    t.push_ok(&reply(&json!({"colour": "red"})));
    t.push_ok(&reply(&json!({"code": "KESTREL-4471"})));
    let r = test_model(&path, "local", t, &keys).unwrap();
    assert!(r.reachable && r.structured_json);
    assert_eq!((r.vision, r.context_8k), (Some(true), Some(true)));
    assert!(test_model(&path, "nope", Arc::new(MockTransport::new()), &keys).is_err());

    let mut s = input(vec![model("cloud", "openai")]);
    s.local_only = true;
    save_settings(&path, &s, &keys).unwrap();
    let t = Arc::new(MockTransport::new());
    let err = test_model(&path, "cloud", t.clone(), &keys).unwrap_err();
    assert!(err.contains("local_only"), "{err}");
    assert!(t.requests().is_empty());
}

fn find(extra: serde_json::Value) -> FindInput {
    let mut base = json!({ "provider": "ollama", "base_url": "https://ollama.com" });
    base.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(base).unwrap()
}

#[test]
fn the_list_uses_the_typed_key_before_the_keychain() {
    let keys = MemoryKeyStore::default();
    keys.set("cloud", "saved-key").unwrap();
    let transport = MockTransport::new();
    transport.push_ok(r#"{"models":[{"name":"gpt-oss:120b-cloud"}]}"#);
    transport.push_ok(r#"{"models":[]}"#);

    let typed = find(
        json!({ "name": "cloud", "old_name": "cloud", "keychain": true, "new_key": "typed-key" }),
    );
    let found = find_models(&typed, &transport, &keys).unwrap();
    assert_eq!(found[0].id, "gpt-oss:120b-cloud");
    assert_eq!(transport.requests()[0].headers[0].1, "Bearer typed-key");

    let saved = find(json!({ "name": "renamed", "old_name": "cloud", "keychain": true }));
    find_models(&saved, &transport, &keys).unwrap();
    assert_eq!(transport.requests()[1].headers[0].1, "Bearer saved-key");
}

#[test]
fn the_list_refuses_an_unknown_kind_of_server() {
    let e = find_models(
        &find(json!({ "provider": "nope" })),
        &MockTransport::new(),
        &MemoryKeyStore::default(),
    )
    .unwrap_err();
    assert!(e.contains("unknown kind"), "{e}");
}

#[test]
fn changed_prompts_are_saved_shown_and_reset() {
    let tmp = TempDir::new("prompts");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::default();
    let default = settings_view(&path, &keys);
    assert_eq!(default.prompts.len(), 6);
    assert!(default.prompts.iter().all(|p| p.text == p.default));
    let before = default.prompts_fingerprint.clone();

    let mut s = input(vec![]);
    s.prompts
        .insert("pins".into(), "Only list the pins.\n{pages}".into());
    // A prompt sent as its own default is not stored as a change.
    let package_default = default
        .prompts
        .iter()
        .find(|p| p.key == "package")
        .unwrap()
        .default
        .clone();
    s.prompts.insert("package".into(), package_default);
    let view = save_settings(&path, &s, &keys).unwrap();
    let pins = view.prompts.iter().find(|p| p.key == "pins").unwrap();
    assert_eq!(pins.text, "Only list the pins.\n{pages}");
    assert_ne!(pins.text, pins.default);
    assert_ne!(view.prompts_fingerprint, before);
    let file = fs::read_to_string(&path).unwrap();
    assert!(
        file.contains("[prompts]") && file.contains("pins"),
        "{file}"
    );
    assert!(!file.contains("package ="), "{file}");

    // Sending the default text (or nothing) resets it.
    let mut again = input(vec![]);
    again.prompts.insert("pins".into(), String::new());
    let reset = save_settings(&path, &again, &keys).unwrap();
    assert_eq!(reset.prompts_fingerprint, before);
    assert!(!fs::read_to_string(&path).unwrap().contains("[prompts]"));

    // A mistake is named and nothing is written.
    let mut bad = input(vec![]);
    bad.prompts.insert("nope".into(), "x".into());
    assert!(
        save_settings(&path, &bad, &keys)
            .unwrap_err()
            .contains("unknown prompt")
    );
}

#[test]
fn extraction_options_are_saved_shown_and_checked() {
    let tmp = TempDir::new("extraction");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::default();
    let view = settings_view(&path, &keys);
    assert_eq!(view.extraction, view.extraction_defaults);
    assert_eq!(view.extraction.attempts, 3);

    let mut changed = view.extraction.clone();
    changed.attempts = 2;
    changed.timeout_secs = 900;
    changed.cache = false;
    let mut s = input(vec![]);
    s.extraction = Some(changed);
    let saved = save_settings(&path, &s, &keys).unwrap();
    assert_eq!(saved.extraction.attempts, 2);
    assert_eq!(saved.extraction.timeout_secs, 900);
    assert!(!saved.extraction.cache);
    assert_eq!(saved.extraction.max_tokens_pins, 8000);
    let file = fs::read_to_string(&path).unwrap();
    assert!(
        file.contains("[extraction]") && file.contains("attempts = 2"),
        "{file}"
    );
    assert!(!file.contains("max_tokens_pins"), "{file}");

    // Sending the defaults leaves nothing in the file; a bad value is refused.
    let mut back = input(vec![]);
    back.extraction = Some(view.extraction_defaults.clone());
    save_settings(&path, &back, &keys).unwrap();
    assert!(!fs::read_to_string(&path).unwrap().contains("[extraction]"));
    let mut bad = view.extraction_defaults.clone();
    bad.attempts = 99;
    let mut s = input(vec![]);
    s.extraction = Some(bad);
    assert!(
        save_settings(&path, &s, &keys)
            .unwrap_err()
            .contains("attempts")
    );
}

#[test]
fn how_a_model_runs_is_saved_shown_and_checked() {
    let tmp = TempDir::new("model-options");
    let path = tmp.path().join("kirpitch.toml");
    let keys = MemoryKeyStore::default();
    let mut m = model("cloud", "ollama");
    m.effort = "high".into();
    m.context_length = Some(32768);
    m.temperature = Some(0.2);
    m.extra_body = "{\"top_k\": 40}".into();
    let view = save_settings(&path, &input(vec![m]), &keys).unwrap();
    let v = &view.models[0];
    assert_eq!(v.effort, "high");
    assert_eq!(v.context_length, Some(32768));
    assert_eq!(v.temperature, Some(0.2));
    assert_eq!(v.extra_body, "{\"top_k\": 40}");
    let file = fs::read_to_string(&path).unwrap();
    assert!(
        file.contains("effort = \"high\"") && file.contains("context_length = 32768"),
        "{file}"
    );

    // Mistakes are named and nothing is written over the good file.
    let mut bad = model("cloud", "ollama");
    bad.extra_body = "{\"model\": \"x\"}".into();
    let e = save_settings(&path, &input(vec![bad]), &keys).unwrap_err();
    assert!(e.contains("cannot set"), "{e}");
    let mut bad = model("cloud", "ollama");
    bad.effort = "extreme".into();
    assert!(
        save_settings(&path, &input(vec![bad]), &keys)
            .unwrap_err()
            .contains("effort")
    );
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("effort = \"high\"")
    );
}
