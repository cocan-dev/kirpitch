// SPDX-License-Identifier: GPL-3.0-or-later

//! Extraction with recorded model answers (no network).

use std::sync::Arc;

use kirpitch_core::{FabProfile, PartSpec, has_errors, validate};
use kirpitch_kicad::testing::TempDir;
use kirpitch_models::config::Config;
use kirpitch_models::transport::Reply;
use kirpitch_models::{
    BuildOptions, Cache, Client, MockTransport, PageLines, TextLine, Transport, build, probe, score,
};
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("../../../testdata/golden/lm358-soic8/partspec.json");

fn golden() -> PartSpec {
    PartSpec::from_json_str(GOLDEN).unwrap()
}

fn line(text: &str, y: f64) -> TextLine {
    TextLine {
        text: text.to_string(),
        y,
        left: 0.1,
        right: 0.9,
    }
}

/// Pages of a datasheet: title page, pin table, package drawing.
fn pages() -> Vec<PageLines> {
    let g = golden();
    let mut pin_lines = vec![line("Pin Configuration and Functions", 0.1)];
    for (i, p) in g.pins.iter().enumerate() {
        pin_lines.push(line(
            &format!("{} {} {}", p.number, p.name, p.electrical_type.as_str()),
            0.2 + 0.05 * i as f64,
        ));
    }
    vec![
        PageLines {
            page: 1,
            lines: vec![line("LM358 Dual Operational Amplifier", 0.1)],
        },
        PageLines {
            page: 3,
            lines: pin_lines,
        },
        PageLines {
            page: 5,
            lines: vec![
                line("Package outline SOIC 8 pins pitch 1.27 mm dimensions", 0.1),
                line("D 4.9 E1 3.9 A 1.75", 0.2),
            ],
        },
    ]
}

fn reply(content: &Value) -> String {
    json!({
        "choices": [{"message": {"content": content.to_string()}}],
        "usage": {"prompt_tokens": 100, "completion_tokens": 20}
    })
    .to_string()
}

fn metadata_answer() -> Value {
    json!({"mpn": "LM358DR", "manufacturer": "Texas Instruments", "description": "Dual operational amplifier"})
}

fn pins_answer() -> Value {
    let pins: Vec<Value> = golden()
        .pins
        .iter()
        .map(|p| {
            json!({"number": p.number, "name": p.name, "type": p.electrical_type.as_str(), "page": 3, "confidence": 0.95})
        })
        .collect();
    json!({ "pins": pins })
}

fn package_answer() -> Value {
    json!({
        "family": "soic", "pin_count": 8, "e": 1.27, "A": 1.75,
        "D": {"min": 4.81, "max": 5.0}, "E1": {"min": 3.81, "max": 3.98},
        "E": {"min": 5.8, "max": 6.19},
        "L": {"min": 0.41, "max": 1.27},
        "b": {"min": 0.31, "max": 0.51},
        "page": 5, "confidence": 0.9
    })
}

fn config() -> Config {
    Config::from_toml_str(
        "[models.m]\nprovider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"test\"\n",
    )
    .unwrap()
}

fn mock(answers: &[Value]) -> Arc<MockTransport> {
    let m = Arc::new(MockTransport::new());
    for a in answers {
        m.push_ok(&reply(a));
    }
    m
}

fn run(m: &Arc<MockTransport>, options: &BuildOptions) -> kirpitch_models::BuildReport {
    build(&config(), m.clone(), &Cache::none(), &pages(), options)
}

#[test]
fn a_datasheet_becomes_a_valid_part_spec() {
    let m = mock(&[metadata_answer(), pins_answer(), package_answer()]);
    let report = run(&m, &BuildOptions::default());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let spec = report.spec.unwrap();
    assert_eq!(spec.metadata.mpn, "LM358DR");
    assert_eq!(spec.package.name, "SOIC-8_3.9x4.9mm_P1.27mm");
    assert_eq!(spec.origin, kirpitch_core::partspec::Origin::Llm);
    // Every value has a page, a region and a confidence: the validator
    // accepts an `llm` PartSpec only then.
    let findings = validate(&spec, &FabProfile::generic());
    assert!(!has_errors(&findings), "{findings:?}");
    assert_eq!(report.stats.calls, 3);
    assert_eq!(report.stats.usage.input_tokens, 300);
    let s = score(&spec, &golden());
    assert!((s.pinout() - 1.0).abs() < 1e-9);
    assert_eq!(s.pins_extra, 0);
    // The pin was found on its line of page 3.
    let p1 = &spec.pins[0];
    let src = p1.source.unwrap();
    assert_eq!(src.page, 3);
    assert!(src.bbox.unwrap().0[3] < 0.5);
    assert!(report.notes.is_empty(), "{:?}", report.notes);
}

#[test]
fn a_pin_the_text_does_not_show_gets_a_low_confidence_and_a_note() {
    let mut pins = pins_answer();
    pins["pins"][7]["name"] = json!("VDD");
    let m = mock(&[metadata_answer(), pins, package_answer()]);
    let report = run(&m, &BuildOptions::default());
    let spec = report.spec.unwrap();
    let c = spec.pins[7].source.unwrap().confidence.unwrap().get();
    assert!(c <= 0.3 + 1e-9, "{c}");
    assert!(
        report.notes.iter().any(|n| n.contains("VDD")),
        "{:?}",
        report.notes
    );
}

#[test]
fn the_second_reading_flags_disagreements() {
    let mut second = pins_answer();
    second["pins"][1]["name"] = json!("IN1_N");
    // metadata, pins (1st), pins (2nd), package
    let m = mock(&[metadata_answer(), pins_answer(), second, package_answer()]);
    let report = run(
        &m,
        &BuildOptions {
            second_read: true,
            ..Default::default()
        },
    );
    assert!(
        report.notes.iter().any(|n| n.contains("disagree on pin 2")),
        "{:?}",
        report.notes
    );
    let spec = report.spec.unwrap();
    assert!(spec.pins[1].source.unwrap().confidence.unwrap().get() <= 0.4 + 1e-9);
}

#[test]
fn an_unusable_answer_is_sent_back_once() {
    let mut bad = pins_answer();
    bad["pins"][0]["type"] = json!("loud");
    let m = mock(&[metadata_answer(), bad, pins_answer(), package_answer()]);
    let report = run(&m, &BuildOptions::default());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let requests = m.requests();
    assert_eq!(requests.len(), 4);
    assert!(requests[2].body.contains("previous answer was not usable"));
    assert!(requests[2].body.contains("loud"));
}

#[test]
fn a_failed_stage_does_not_stop_the_others() {
    let garbage = json!({"family": "soic"});
    // The package answer is wrong twice.
    let m = mock(&[metadata_answer(), pins_answer(), garbage.clone(), garbage]);
    let report = run(&m, &BuildOptions::default());
    assert!(report.spec.is_none());
    assert!(report.pins.is_some());
    assert_eq!(report.failures.len(), 1);
    assert!(
        report.failures[0].starts_with("package:"),
        "{:?}",
        report.failures
    );
}

#[test]
fn answers_are_cached() {
    let tmp = TempDir::new("models-cache");
    let cache = Cache::in_dir(tmp.path());
    let m = mock(&[metadata_answer(), pins_answer(), package_answer()]);
    let first = build(
        &config(),
        m.clone(),
        &cache,
        &pages(),
        &BuildOptions::default(),
    );
    assert_eq!(first.stats.calls, 3);
    let again = Arc::new(MockTransport::new());
    let second = build(
        &config(),
        again.clone(),
        &cache,
        &pages(),
        &BuildOptions::default(),
    );
    assert!(second.failures.is_empty(), "{:?}", second.failures);
    assert_eq!(second.stats.calls, 0);
    assert_eq!(second.stats.cached, 3);
    assert!(again.requests().is_empty());
    assert_eq!(first.spec, second.spec);
}

#[test]
fn a_server_without_json_schema_support_gets_the_schema_in_the_prompt() {
    let m = Arc::new(MockTransport::new());
    m.push(Ok(Reply {
        status: 400,
        body: r#"{"error":{"message":"response_format is not supported"}}"#.into(),
    }));
    m.push_ok(&reply(&metadata_answer()));
    let options = BuildOptions {
        mpn: None,
        ..Default::default()
    };
    // Only the metadata call is of interest: the other stages fail without
    // answers left.
    let report = run(&m, &options);
    assert!(report.stats.calls >= 1);
    let requests = m.requests();
    assert!(requests[0].body.contains("response_format"));
    assert!(!requests[1].body.contains("response_format"));
    assert!(requests[1].body.contains("shaped like this example"));
}

#[test]
fn local_only_refuses_a_cloud_model_before_anything_is_sent() {
    let config = Config::from_toml_str(
        "[models.cloud]\nprovider = \"openai\"\nmodel = \"x\"\n[privacy]\nlocal_only = true\n",
    )
    .unwrap();
    let m = Arc::new(MockTransport::new());
    let report = build(
        &config,
        m.clone(),
        &Cache::none(),
        &pages(),
        &BuildOptions::default(),
    );
    assert!(report.spec.is_none());
    assert!(m.requests().is_empty());
    assert!(
        report.failures.iter().all(|f| f.contains("local_only")),
        "{:?}",
        report.failures
    );
    assert_eq!(report.failures.len(), 3);
}

#[test]
fn the_probe_reports_json_vision_and_context() {
    let m = Arc::new(MockTransport::new());
    m.push_ok(&reply(&json!({"answer": 5})));
    m.push_ok(&reply(&json!({"colour": "Red"})));
    m.push_ok(&reply(&json!({"code": "KESTREL-4471"})));
    let transport: Arc<dyn Transport> = m;
    let client = Client::new(config().models["m"].clone(), None, transport);
    let r = probe("m", &client);
    assert!(r.reachable && r.structured_json);
    assert_eq!(r.vision, Some(true));
    assert_eq!(r.context_8k, Some(true));
    assert_eq!(r.usage.input_tokens, 300);
}

#[test]
fn a_text_model_is_reported_as_having_no_vision() {
    let m = Arc::new(MockTransport::new());
    m.push_ok(&reply(&json!({"answer": 5})));
    m.push(Ok(Reply {
        status: 400,
        body: r#"{"error":{"message":"images are not supported"}}"#.into(),
    }));
    m.push_ok(&reply(&json!({"code": "wrong"})));
    let transport: Arc<dyn Transport> = m;
    let client = Client::new(config().models["m"].clone(), None, transport);
    let r = probe("m", &client);
    assert_eq!(r.vision, Some(false));
    assert_eq!(r.context_8k, Some(false));
}

#[test]
fn the_prompt_shows_an_example_not_the_schema() {
    let m = mock(&[metadata_answer(), pins_answer(), package_answer()]);
    let report = run(&m, &BuildOptions::default());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    for r in m.requests() {
        let body: Value = serde_json::from_str(&r.body).unwrap();
        let user = body["messages"][1]["content"][0]["text"].as_str().unwrap();
        assert!(user.contains("shaped like this example"), "{user}");
        assert!(!user.contains("JSON schema"), "{user}");
        assert!(!user.contains("\"required\""), "{user}");
    }
}

#[test]
fn an_answer_inside_a_wrapper_is_still_read() {
    // What a model does when it has seen a schema: the answer goes under
    // `properties`, or under `answer`.
    let m = mock(&[
        json!({"properties": metadata_answer()}),
        json!({"answer": pins_answer()}),
        package_answer(),
    ]);
    let report = run(&m, &BuildOptions::default());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.spec.is_some());
    // No repair was needed: one call for each stage.
    assert_eq!(m.requests().len(), 3);
}

#[test]
fn an_empty_answer_is_asked_again_without_thinking() {
    let config = Config::from_toml_str(
        "[models.m]\nprovider = \"ollama\"\nbase_url = \"http://localhost:11434\"\nmodel = \"test\"\n",
    )
    .unwrap();
    let m = Arc::new(MockTransport::new());
    // A thinking model that used its whole budget: nothing comes back.
    m.push_ok(&json!({"message": {"content": ""}, "done_reason": "length"}).to_string());
    m.push_ok(&json!({"message": {"content": metadata_answer().to_string()}}).to_string());
    let options = BuildOptions {
        mpn: None,
        ..Default::default()
    };
    let report = build(&config, m.clone(), &Cache::none(), &pages(), &options);
    let requests = m.requests();
    assert!(requests.len() >= 2);
    let first: Value = serde_json::from_str(&requests[0].body).unwrap();
    let second: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert!(first.get("think").is_none());
    assert_eq!(second["think"], json!(false));
    assert!(second["options"]["num_predict"].as_u64() > first["options"]["num_predict"].as_u64());
    assert!(
        !report.failures.iter().any(|f| f.starts_with("metadata")),
        "{:?}",
        report.failures
    );
}

#[test]
fn a_changed_prompt_is_what_the_model_is_asked_and_the_defaults_are_unchanged() {
    // The default prompts, filled in, carry the instructions and the page text.
    let m = mock(&[metadata_answer(), pins_answer(), package_answer()]);
    run(&m, &BuildOptions::default());
    let first = m.requests();
    let user = |r: &kirpitch_models::Request| {
        let b: Value = serde_json::from_str(&r.body).unwrap();
        b["messages"][1]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(user(&first[1]).contains("List every pin of the package"));
    assert!(user(&first[1]).contains("input, output, bidirectional"));

    // A changed pin prompt (without a place for the pages) is used, and the
    // pages are added at its end.
    let mut config = config();
    config
        .prompts
        .set("pins", "My own words: list the pins. {types}")
        .unwrap();
    config.prompts.set("system", "My system rules.").unwrap();
    let m = mock(&[metadata_answer(), pins_answer(), package_answer()]);
    build(
        &config,
        m.clone(),
        &Cache::none(),
        &pages(),
        &BuildOptions::default(),
    );
    let second = m.requests();
    let pins = user(&second[1]);
    assert!(
        pins.starts_with("My own words: list the pins. input, output"),
        "{pins}"
    );
    assert!(
        pins.contains("Pin Configuration and Functions"),
        "the pages are added: {pins}"
    );
    let body: Value = serde_json::from_str(&second[1].body).unwrap();
    assert_eq!(body["messages"][0]["content"], "My system rules.");
    // The other tasks keep their defaults.
    assert!(user(&second[0]).contains("part number, the manufacturer"));
    assert_ne!(
        config.prompts.fingerprint(),
        Config::default().prompts.fingerprint()
    );
}

#[test]
fn prompts_are_saved_in_the_file_only_when_changed() {
    let mut config = Config::default();
    assert!(!config.to_toml_string().unwrap().contains("[prompts]"));
    config
        .prompts
        .set("package", "Read the package.\n{pages}")
        .unwrap();
    let text = config.to_toml_string().unwrap();
    assert!(text.contains("[prompts]"), "{text}");
    assert!(!text.contains("system"), "{text}");
    let back = Config::from_toml_str(&text).unwrap();
    assert_eq!(back.prompts, config.prompts);
}

fn ollama_config() -> Config {
    Config::from_toml_str(
        "[models.m]\nprovider = \"ollama\"\nbase_url = \"http://localhost:11434\"\nmodel = \"test\"\n",
    )
    .unwrap()
}

#[test]
fn every_call_is_reported_with_what_the_model_said_and_thought() {
    let m = Arc::new(MockTransport::new());
    // A thinking model: nothing in the answer, a lot of thinking, stopped at
    // the limit. Then a good answer.
    m.push_ok(
        &json!({"message": {"content": "", "thinking": "Let me think about this datasheet at length..."},
                "done_reason": "length", "prompt_eval_count": 100, "eval_count": 2000})
        .to_string(),
    );
    m.push_ok(
        &json!({"message": {"content": metadata_answer().to_string()}, "done_reason": "stop"})
            .to_string(),
    );
    let calls = std::sync::Mutex::new(Vec::new());
    let options = BuildOptions {
        mpn: None,
        ..Default::default()
    };
    kirpitch_models::build_with_progress(
        &ollama_config(),
        m,
        &Cache::none(),
        &pages(),
        &options,
        &kirpitch_models::MemoryKeyStore::default(),
        &|p| {
            if let Some(c) = &p.call {
                calls.lock().unwrap().push((p.stage, c.clone()));
            }
        },
    );
    let calls = calls.lock().unwrap();
    let metadata: Vec<_> = calls.iter().filter(|(s, _)| *s == "metadata").collect();
    // sent, received (empty), rejected, sent, received.
    let phases: Vec<&str> = metadata.iter().map(|(_, c)| c.phase.as_str()).collect();
    assert_eq!(phases, ["sent", "received", "rejected", "sent", "received"]);
    let first = &metadata[1].1;
    assert_eq!(first.text, "");
    assert!(first.thinking.contains("think about this datasheet"));
    assert_eq!(first.finish, "length");
    assert_eq!(first.tokens_out, 2000);
    assert_eq!(first.label, "metadata");
    assert_eq!(metadata[1].1.attempt, 1);
    assert_eq!(metadata[3].1.attempt, 2);
    assert!(metadata[2].1.problem.contains("no JSON object"));
    assert!(metadata[4].1.text.contains("LM358DR"));
    assert_eq!(first.model, "test");
}

#[test]
fn a_package_that_cannot_be_drawn_is_named_and_not_asked_again() {
    let m = mock(&[
        metadata_answer(),
        pins_answer(),
        json!({"family": "other", "package_name": "LQFP64", "pin_count": 64}),
    ]);
    let report = run(&m, &BuildOptions::default());
    // One call for the package: asking again would not change the answer.
    assert_eq!(m.requests().len(), 3);
    assert!(report.spec.is_none());
    assert!(report.pins.is_some(), "the pins are kept");
    let failure = report
        .failures
        .iter()
        .find(|f| f.starts_with("package"))
        .unwrap();
    assert!(failure.contains("LQFP64"), "{failure}");
    assert!(failure.contains("not one of the shapes"), "{failure}");
}

#[test]
fn the_options_set_the_tries_the_limits_and_the_time() {
    let mut config = config();
    config.extraction.attempts = Some(1);
    config.extraction.max_tokens_pins = Some(1234);
    config.extraction.timeout_secs = Some(77);
    let mut bad = pins_answer();
    bad["pins"][0]["type"] = json!("loud");
    let m = mock(&[metadata_answer(), bad, package_answer()]);
    let report = build(
        &config,
        m.clone(),
        &Cache::none(),
        &pages(),
        &BuildOptions::default(),
    );
    // With one try there is no repair: the pin step fails after one call.
    assert_eq!(m.requests().len(), 3);
    assert!(
        report.failures.iter().any(|f| f.starts_with("pins")),
        "{:?}",
        report.failures
    );
    let pins_body: Value = serde_json::from_str(&m.requests()[1].body).unwrap();
    assert_eq!(pins_body["max_tokens"], json!(1234));
}

#[test]
fn extraction_options_are_saved_only_when_changed_and_checked() {
    let mut config = Config::default();
    config.extraction.attempts = Some(3); // the default
    config.extraction.timeout_secs = Some(120);
    config.extraction = config.extraction.normalized();
    assert_eq!(config.extraction.attempts, None);
    let text = config.to_toml_string().unwrap();
    assert!(
        text.contains("[extraction]") && text.contains("timeout_secs = 120"),
        "{text}"
    );
    assert!(!text.contains("attempts"), "{text}");
    assert_eq!(
        Config::from_toml_str(&text)
            .unwrap()
            .extraction
            .timeout_secs(),
        120
    );
    assert!(Config::from_toml_str("[extraction]\nattempts = 9\n").is_err());
    assert!(Config::from_toml_str("[extraction]\ntimeout_secs = 5\n").is_err());
    assert!(Config::from_toml_str("[extraction]\nmax_tokens_pins = 10\n").is_err());
}
