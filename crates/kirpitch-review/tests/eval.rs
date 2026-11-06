// SPDX-License-Identifier: GPL-3.0-or-later

//! Evaluating the models and prompts against cases with known answers.

use std::fs;
use std::sync::{Arc, Mutex};

use kirpitch_kicad::testing::TempDir;
use kirpitch_models::{Config, MemoryKeyStore, MockTransport};
use kirpitch_review::{
    EvalInput, EvalTools, add_case, delete_run, list_cases_in, list_runs, remove_case,
    resolve_cases, run_eval,
};
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("../../../testdata/golden/lm358-soic8/partspec.json");

fn golden_pins() -> Vec<(String, String, String)> {
    let v: Value = serde_json::from_str(GOLDEN).unwrap();
    v["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["number"].as_str().unwrap().to_string(),
                p["name"].as_str().unwrap().to_string(),
                p["electrical_type"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn pdf() -> Vec<u8> {
    // The page text lives as long as the test process: leaking is fine here.
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let mut pins = vec![(72.0, 740.0, 14.0, "Pin Configuration and Functions")];
    for (i, (n, name, t)) in golden_pins().into_iter().enumerate() {
        pins.push((
            72.0,
            700.0 - 20.0 * i as f32,
            12.0,
            leak(format!("{n} {name} {t}")),
        ));
    }
    kirpitch_extract::testing::make_pdf(&[
        vec![(
            72.0,
            740.0,
            16.0,
            "LM358DR Texas Instruments dual operational amplifier",
        )],
        pins,
        vec![(
            72.0,
            740.0,
            14.0,
            "Package outline SOIC pitch 1.27 mm dimensions",
        )],
    ])
}

fn reply(v: &Value) -> String {
    json!({"choices": [{"message": {"content": v.to_string()}}],
           "usage": {"prompt_tokens": 10, "completion_tokens": 5}})
    .to_string()
}

fn answers(m: &MockTransport, wrong_pin: bool) {
    let mut pins: Vec<Value> = golden_pins()
        .iter()
        .map(|(n, name, t)| json!({"number": n, "name": name, "type": t, "page": 2, "confidence": 0.9}))
        .collect();
    if wrong_pin {
        pins[1]["name"] = json!("IN1M");
    }
    m.push_ok(&reply(
        &json!({"mpn": "LM358DR", "manufacturer": "TI", "description": "Op amp"}),
    ));
    m.push_ok(&reply(&json!({ "pins": pins })));
    m.push_ok(&reply(&json!({
        "family": "soic", "pin_count": 8, "e": 1.27, "A": 1.75,
        "D": {"min": 4.81, "max": 5.0}, "E1": {"min": 3.81, "max": 3.98},
        "E": {"min": 5.8, "max": 6.19}, "L": {"min": 0.41, "max": 1.27},
        "b": {"min": 0.31, "max": 0.51}, "page": 3, "confidence": 0.9
    })));
}

#[test]
fn a_case_is_run_scored_saved_compared_and_deleted() {
    if kirpitch_review::find_pdfium_dir().is_none() {
        eprintln!("SKIPPED: no pdfium library (cargo xtask fetch-pdfium)");
        return;
    }
    let tmp = TempDir::new("eval");
    let dir = tmp.path().join("evals");
    let spec = tmp.path().join("expected.json");
    fs::write(&spec, GOLDEN).unwrap();
    let pdf_path = tmp.path().join("d.pdf");
    fs::write(&pdf_path, pdf()).unwrap();

    // Cases: the user's own part and datasheet.
    assert!(
        add_case(
            &dir,
            "",
            &spec.to_string_lossy(),
            &pdf_path.to_string_lossy(),
            ""
        )
        .is_err()
    );
    assert!(add_case(&dir, "mine", "nope.json", &pdf_path.to_string_lossy(), "").is_err());
    add_case(
        &dir,
        "mine",
        &spec.to_string_lossy(),
        &pdf_path.to_string_lossy(),
        "SOIC-8",
    )
    .unwrap();
    assert!(
        add_case(
            &dir,
            "mine",
            &spec.to_string_lossy(),
            &pdf_path.to_string_lossy(),
            ""
        )
        .is_err()
    );
    let cases = list_cases_in(&dir);
    let mine = cases.iter().find(|c| c.name == "mine").unwrap();
    assert!(mine.ready);
    assert_eq!(mine.mpn, "LM358DR");
    assert!(resolve_cases(&dir, &["nope".into()]).is_err());
    let resolved = resolve_cases(&dir, &["mine".into()]).unwrap();

    let config = Config::from_toml_str(
        "[models.m]\nprovider = \"openai-compatible\"\nbase_url = \"http://localhost:1/v1\"\nmodel = \"t\"\n",
    )
    .unwrap();
    let keys = MemoryKeyStore::default();
    let events = Mutex::new(Vec::new());
    let input = EvalInput {
        cases: vec!["mine".into()],
        label: "first try".into(),
        ..Default::default()
    };

    let transport = Arc::new(MockTransport::new());
    answers(&transport, false);
    let on = |e: &kirpitch_review::EvalEvent| {
        events
            .lock()
            .unwrap()
            .push((e.case.clone(), e.state.clone()))
    };
    let tools = EvalTools {
        transport: transport.clone(),
        keys: &keys,
        on: &on,
        stop: &|| false,
    };
    let run = run_eval(&dir, &config, &resolved, &input, &tools).unwrap();
    assert_eq!(run.cases.len(), 1);
    let c = &run.cases[0];
    assert_eq!(c.status, "scored", "{:?}", c.failures);
    assert_eq!(c.pins_correct, 8);
    assert!((run.totals.pinout - 1.0).abs() < 1e-9);
    assert!(run.totals.tokens > 0);
    assert_eq!(run.label, "first try");
    assert_eq!(run.prompts.len(), 5);
    assert_eq!(run.prompts_fingerprint, config.prompts.fingerprint());
    assert_eq!(run.models.len(), 3);
    let ev = events.lock().unwrap();
    assert_eq!(ev.first().unwrap().1, "started");
    assert_eq!(ev.last().unwrap().1, "finished");
    drop(ev);

    // A second run with a changed prompt and a wrong pin scores lower, and the
    // history lists both, newest first, each with its prompts.
    let mut changed = config.clone();
    changed
        .prompts
        .set("pins", "Just the pins.\n{pages}")
        .unwrap();
    let transport = Arc::new(MockTransport::new());
    answers(&transport, true);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let tools = EvalTools {
        transport,
        keys: &keys,
        on: &|_| {},
        stop: &|| false,
    };
    let second = run_eval(&dir, &changed, &resolved, &input, &tools).unwrap();
    assert_eq!(second.cases[0].pins_correct, 7);
    assert!(second.totals.pinout < run.totals.pinout);
    assert_ne!(second.prompts_fingerprint, run.prompts_fingerprint);
    assert_eq!(second.prompts["pins"], "Just the pins.\n{pages}");
    let runs = list_runs(&dir);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, second.id);
    assert_eq!(runs[1], run);

    // Stopping before a case keeps what is there.
    let tools = EvalTools {
        transport: Arc::new(MockTransport::new()),
        keys: &keys,
        on: &|_| {},
        stop: &|| true,
    };
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let stopped = run_eval(&dir, &config, &resolved, &input, &tools).unwrap();
    assert!(stopped.cases.is_empty());

    delete_run(&dir, &second.id).unwrap();
    assert!(delete_run(&dir, "../x").is_err());
    assert_eq!(list_runs(&dir).len(), 2);
    remove_case(&dir, "mine").unwrap();
    assert!(list_cases_in(&dir).iter().all(|c| c.name != "mine"));
}

#[test]
fn a_case_without_its_datasheet_is_skipped_not_failed() {
    let tmp = TempDir::new("eval-missing");
    let dir = tmp.path().join("evals");
    let spec = tmp.path().join("expected.json");
    fs::write(&spec, GOLDEN).unwrap();
    let pdf_path = tmp.path().join("d.pdf");
    fs::write(&pdf_path, b"x").unwrap();
    add_case(
        &dir,
        "gone",
        &spec.to_string_lossy(),
        &pdf_path.to_string_lossy(),
        "",
    )
    .unwrap();
    fs::remove_file(&pdf_path).unwrap();
    let resolved = resolve_cases(&dir, &["gone".into()]).unwrap();
    let keys = MemoryKeyStore::default();
    let tools = EvalTools {
        transport: Arc::new(MockTransport::new()),
        keys: &keys,
        on: &|_| {},
        stop: &|| false,
    };
    let run = run_eval(
        &dir,
        &Config::default(),
        &resolved,
        &EvalInput {
            cases: vec!["gone".into()],
            ..Default::default()
        },
        &tools,
    )
    .unwrap();
    assert_eq!(run.cases[0].status, "skipped");
    assert_eq!(run.totals.cases, 0);
    assert!(run.cases[0].failures[0].contains("not there"));
}
