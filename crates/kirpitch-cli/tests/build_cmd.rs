// SPDX-License-Identifier: GPL-3.0-or-later

//! `kirpitch build`, `models test` and `bench` with recorded model answers.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use kirpitch_kicad::testing::TempDir;
use kirpitch_models::MockTransport;
use serde_json::{Value, json};

/// The command line code keeps its settings in process-wide variables, so
/// these tests run one at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn run(args: &[&str], transport: Arc<MockTransport>) -> (u8, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut full = vec!["kirpitch"];
    full.extend_from_slice(args);
    let code = kirpitch_cli::run_with_transport(full, &mut out, &mut err, transport);
    let code = if code == std::process::ExitCode::SUCCESS {
        0
    } else if code == std::process::ExitCode::from(1) {
        1
    } else {
        3
    };
    (
        code,
        String::from_utf8_lossy(&out).replace("\r\n", "\n"),
        String::from_utf8_lossy(&err).replace("\r\n", "\n"),
    )
}

fn reply(content: &Value) -> String {
    json!({
        "choices": [{"message": {"content": content.to_string()}}],
        "usage": {"prompt_tokens": 50, "completion_tokens": 10}
    })
    .to_string()
}

fn config_file(dir: &Path) -> String {
    let path = dir.join("kirpitch.toml");
    fs::write(
        &path,
        "[models.m]\nprovider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"test\"\n",
    )
    .unwrap();
    path.to_string_lossy().into_owned()
}

const PINS: [(&str, &str, &str); 8] = [
    ("1", "OUT1", "output"),
    ("2", "IN1-", "input"),
    ("3", "IN1+", "input"),
    ("4", "GND", "power_in"),
    ("5", "IN2+", "input"),
    ("6", "IN2-", "input"),
    ("7", "OUT2", "output"),
    ("8", "VCC", "power_in"),
];

fn datasheet_pdf() -> Vec<u8> {
    let mut pin_page = vec![(72.0, 740.0, 14.0, "Pin Configuration and Functions")];
    let rows = [
        "1 OUT1 output",
        "2 IN1- input",
        "3 IN1+ input",
        "4 GND power",
        "5 IN2+ input",
        "6 IN2- input",
        "7 OUT2 output",
        "8 VCC power",
    ];
    for (i, row) in rows.iter().enumerate() {
        pin_page.push((72.0, 700.0 - 20.0 * i as f32, 12.0, *row));
    }
    kirpitch_extract::testing::make_pdf(&[
        vec![(
            72.0,
            740.0,
            16.0,
            "LM358DR Texas Instruments dual operational amplifier",
        )],
        pin_page,
        vec![
            (
                72.0,
                740.0,
                14.0,
                "Package outline SOIC pitch 1.27 mm dimensions",
            ),
            (72.0, 700.0, 12.0, "D 4.9 E1 3.9 A 1.75"),
        ],
    ])
}

fn answers() -> Vec<String> {
    let pins: Vec<Value> = PINS
        .iter()
        .map(|(n, name, t)| json!({"number": n, "name": name, "type": t, "page": 2, "confidence": 0.9}))
        .collect();
    vec![
        reply(
            &json!({"mpn": "LM358DR", "manufacturer": "Texas Instruments", "description": "Dual operational amplifier"}),
        ),
        reply(&json!({ "pins": pins })),
        reply(&json!({
            "family": "soic", "pin_count": 8, "e": 1.27, "A": 1.75,
        "D": {"min": 4.81, "max": 5.0}, "E1": {"min": 3.81, "max": 3.98},
        "E": {"min": 5.8, "max": 6.19},
        "L": {"min": 0.41, "max": 1.27},
        "b": {"min": 0.31, "max": 0.51},
            "page": 3, "confidence": 0.9
        })),
    ]
}

fn mock(replies: &[String]) -> Arc<MockTransport> {
    let m = Arc::new(MockTransport::new());
    for r in replies {
        m.push_ok(r);
    }
    m
}

#[test]
fn without_a_configuration_the_manual_flow_is_suggested() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new("build-noconfig");
    let pdf = tmp.path().join("d.pdf");
    fs::write(&pdf, datasheet_pdf()).unwrap();
    let spec = tmp.path().join("spec.json");
    // No --config, and the test folder has no kirpitch.toml.
    let (code, _, err) = run(
        &[
            "build",
            &pdf.to_string_lossy(),
            "-o",
            &spec.to_string_lossy(),
        ],
        mock(&[]),
    );
    assert_eq!(code, 1);
    assert!(err.contains("no model is configured"), "{err}");
    assert!(err.contains("kirpitch template"), "{err}");
    assert!(!spec.exists());
}

#[test]
fn build_writes_a_part_spec_that_goes_through_review() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    if std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR").is_none() {
        eprintln!("SKIPPED: set KIRPITCH_PDFIUM_LIB_DIR to run this test");
        return;
    }
    let tmp = TempDir::new("build-ok");
    let pdf = tmp.path().join("d.pdf");
    fs::write(&pdf, datasheet_pdf()).unwrap();
    let spec = tmp.path().join("spec.json");
    let m = mock(&answers());
    let (code, out, err) = run(
        &[
            "build",
            &pdf.to_string_lossy(),
            "-o",
            &spec.to_string_lossy(),
            "--config",
            &config_file(tmp.path()),
            "--no-cache",
        ],
        m.clone(),
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("read 3 pages"), "{out}");
    assert!(out.contains("3 model call(s)"), "{out}");
    assert!(out.contains("data stays on this machine"), "{out}");
    assert_eq!(m.requests().len(), 3);
    let text = fs::read_to_string(&spec).unwrap();
    assert!(text.contains("\"origin\": \"llm\""), "{text}");
    assert!(text.contains("LM358DR"));
    assert!(text.contains("datasheet_sha256"));
    // The result is an ordinary PartSpec: checks and review work on it.
    let (code, out, _) = run(&["check", &spec.to_string_lossy()], mock(&[]));
    assert_eq!(code, 0, "{out}");
    let (code, out, _) = run(&["status", &spec.to_string_lossy()], mock(&[]));
    assert_eq!(code, 0);
    assert!(out.contains("pinout"), "{out}");
    let (code, _, err) = run(
        &["approve", &spec.to_string_lossy(), "--stage", "pinout"],
        mock(&[]),
    );
    assert_eq!(code, 0, "{err}");
}

#[test]
fn a_failed_package_leaves_the_pins_for_the_manual_flow() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    if std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR").is_none() {
        eprintln!("SKIPPED: set KIRPITCH_PDFIUM_LIB_DIR to run this test");
        return;
    }
    let tmp = TempDir::new("build-partial");
    let pdf = tmp.path().join("d.pdf");
    fs::write(&pdf, datasheet_pdf()).unwrap();
    let spec = tmp.path().join("spec.json");
    let mut replies = answers();
    let bad = reply(&json!({"family": "soic"}));
    replies[2] = bad.clone();
    replies.push(bad);
    let (code, out, err) = run(
        &[
            "build",
            &pdf.to_string_lossy(),
            "-o",
            &spec.to_string_lossy(),
            "--config",
            &config_file(tmp.path()),
            "--no-cache",
        ],
        mock(&replies),
    );
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(err.contains("failed: package:"), "{err}");
    assert!(!spec.exists());
    let csv = fs::read_to_string(tmp.path().join("spec.pins.csv")).unwrap();
    assert!(csv.starts_with("Pin,Name,Type\n1,OUT1,output\n"), "{csv}");
    assert!(out.contains("Finish by hand"), "{out}");
}

#[test]
fn models_test_reports_what_each_model_can_do() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new("models-test");
    let m = mock(&[
        reply(&json!({"answer": 5})),
        reply(&json!({"colour": "red"})),
        reply(&json!({"code": "KESTREL-4471"})),
    ]);
    let (code, out, err) = run(&["models", "test", "--config", &config_file(tmp.path())], m);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("structured JSON: yes"), "{out}");
    assert!(out.contains("vision: yes"), "{out}");
    assert!(out.contains("8k context: yes"), "{out}");
    assert!(out.contains("task pin_tables"), "{out}");
}

#[test]
fn models_test_refuses_cloud_models_in_local_only_mode() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new("models-local");
    let path = tmp.path().join("kirpitch.toml");
    fs::write(
        &path,
        "[models.cloud]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet\"\n[privacy]\nlocal_only = true\n",
    )
    .unwrap();
    let m = mock(&[]);
    let (code, out, _) = run(
        &["models", "test", "--config", &path.to_string_lossy()],
        m.clone(),
    );
    assert_eq!(code, 1);
    assert!(out.contains("refused"), "{out}");
    assert!(m.requests().is_empty());
}

#[test]
fn bench_skips_parts_without_a_pdf() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new("bench-skip");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden");
    let (code, out, err) = run(
        &[
            "bench",
            "--config",
            &config_file(tmp.path()),
            "--golden",
            &golden.to_string_lossy(),
            "--pdfs",
            &tmp.path().to_string_lossy(),
        ],
        mock(&[]),
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("lm358-soic8"), "{out}");
    assert!(out.contains("skipped"), "{out}");
    assert!(out.contains("total"), "{out}");
}
