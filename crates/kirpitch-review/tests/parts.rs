// SPDX-License-Identifier: GPL-3.0-or-later

//! The starting points of the window: examples, a new part, copies and a build
//! from a datasheet.

use std::fs;
use std::sync::Arc;

use kirpitch_kicad::testing::TempDir;
use kirpitch_models::{MockTransport, StageState};
use kirpitch_review::{
    DatasheetInput, NewPartInput, approve_stage, build_with_transport,
    build_with_transport_progress, list_examples, new_part, now_rfc3339, open_example, save_copy,
};
use serde_json::{Value, json};
use std::sync::Mutex;

fn input(dir: &std::path::Path, kind: &str) -> NewPartInput {
    let mut v = json!({
        "path": dir.join(format!("{kind}.json")).to_string_lossy(),
        "mpn": "TEST-1", "manufacturer": "Test", "description": "A test part",
        "kind": kind, "pitch": 1.27, "height": 1.75,
    });
    match kind {
        "dual-row" | "sot23" => {
            v["pin_count"] = json!(if kind == "sot23" { 5 } else { 8 });
            v["pitch"] = json!(if kind == "sot23" { 0.95 } else { 1.27 });
            v["body_length"] = json!(if kind == "sot23" { 2.9 } else { 4.9 });
            v["body_width"] = json!(if kind == "sot23" { 1.6 } else { 3.9 });
            v["height"] = json!(if kind == "sot23" { 1.45 } else { 1.75 });
            v["lead_span"] = json!(if kind == "sot23" {
                [2.6, 3.0]
            } else {
                [5.8, 6.2]
            });
            v["lead_length"] = json!([0.4, 0.6]);
            v["lead_width"] = json!([0.3, 0.5]);
        }
        "dip" => {
            v["pin_count"] = json!(8);
            v["pitch"] = json!(2.54);
            v["row_spacing"] = json!(7.62);
            v["body_length"] = json!(9.3);
            v["body_width"] = json!(6.4);
            v["height"] = json!(5.0);
        }
        _ => {
            v["pins_x"] = json!(4);
            v["pins_y"] = json!(4);
            v["pitch"] = json!(0.5);
            v["body_x"] = json!([3.95, 4.05]);
            v["body_y"] = json!([3.95, 4.05]);
            v["height"] = json!(1.0);
            v["lead_length"] = json!([0.35, 0.45]);
            v["lead_width"] = json!([0.2, 0.3]);
            v["exposed_pad"] = json!([2.6, 2.6]);
        }
    }
    serde_json::from_value(v).unwrap()
}

#[test]
fn the_golden_parts_are_listed_as_examples() {
    let examples = list_examples();
    let names: Vec<&str> = examples.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["24lc256-soic8", "lm358-soic8", "ne555-soic8"]);
    let lm = &examples[1];
    assert_eq!(lm.mpn, "LM358DR");
    assert_eq!(lm.package, "SOIC-8_3.9x4.9mm_P1.27mm");
}

#[test]
fn an_example_opens_as_a_copy_that_keeps_its_approvals() {
    let name = "ne555-soic8";
    let view = open_example(name, true).unwrap();
    assert_eq!(view.mpn, "NE555DR");
    assert!(view.path.contains("kirpitch-examples"));
    // Approve in the copy; opening again keeps the approval, `fresh` drops it.
    approve_stage(&view.path, "pinout", None, now_rfc3339()).unwrap();
    assert_eq!(
        open_example(name, false).unwrap().gates[0].status,
        "approved"
    );
    assert_eq!(open_example(name, true).unwrap().gates[0].status, "open");
    // The original in testdata is untouched.
    let original = list_examples();
    assert!(original.iter().any(|e| e.name == name));
    assert!(open_example("../x", false).is_err());
    assert!(open_example("nope", false).is_err());
}

#[test]
fn a_new_part_is_made_from_each_package_form() {
    let tmp = TempDir::new("new-part");
    for (kind, pins, package) in [
        ("dual-row", 8, "SOIC-8_3.9x4.9mm_P1.27mm"),
        ("sot23", 5, "SOT-23-5"),
        ("dip", 8, "DIP-8_W7.62mm"),
        ("qfn", 17, "QFN-16-1EP_4x4mm_P0.5mm_EP2.6x2.6mm"),
    ] {
        let view = new_part(&input(tmp.path(), kind)).unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(view.pins.len(), pins, "{kind}");
        assert_eq!(view.pads.len(), pins, "{kind}");
        assert!(
            view.package_name.starts_with(&package[..5]),
            "{} {kind}",
            view.package_name
        );
        assert_eq!(view.pins[0].name, "PIN1");
        assert_eq!(view.model_views.len(), 3);
    }
    // The exposed pad of the QFN is its own passive pin.
    let qfn = kirpitch_review::open_spec(&tmp.path().join("qfn.json").to_string_lossy()).unwrap();
    assert_eq!(qfn.pins[16].name, "EP");
    assert_eq!(qfn.pins[16].electrical_type, "passive");
}

#[test]
fn a_new_part_never_replaces_a_file_and_names_what_is_missing() {
    let tmp = TempDir::new("new-part-errors");
    let good = input(tmp.path(), "dual-row");
    new_part(&good).unwrap();
    assert!(new_part(&good).unwrap_err().contains("already exists"));

    let mut no_mpn = input(tmp.path(), "dip");
    no_mpn.mpn = " ".into();
    assert!(new_part(&no_mpn).unwrap_err().contains("part number"));
    let mut no_pins = input(tmp.path(), "dip");
    no_pins.pin_count = None;
    assert!(new_part(&no_pins).unwrap_err().contains("number of pins"));
    let mut bad = input(tmp.path(), "qfn");
    bad.kind = "bga".into();
    assert!(new_part(&bad).unwrap_err().contains("unknown package kind"));
}

#[test]
fn a_copy_keeps_the_content_and_the_original_stays() {
    let tmp = TempDir::new("copy");
    let part = new_part(&input(tmp.path(), "dual-row")).unwrap();
    let dest = tmp.path().join("sub/copy.json");
    let copy = save_copy(&part.path, &dest.to_string_lossy()).unwrap();
    assert_eq!(copy.path, dest.to_string_lossy());
    assert_eq!(copy.pins, part.pins);
    assert!(fs::metadata(&part.path).is_ok());
}

fn pdf() -> Vec<u8> {
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
    let mut pins = vec![(72.0, 740.0, 14.0, "Pin Configuration and Functions")];
    for (i, r) in rows.iter().enumerate() {
        pins.push((72.0, 700.0 - 20.0 * i as f32, 12.0, *r));
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

#[test]
fn a_datasheet_is_built_into_a_part_with_recorded_answers() {
    if kirpitch_review::find_pdfium_dir().is_none() {
        eprintln!("SKIPPED: no pdfium library (cargo xtask fetch-pdfium)");
        return;
    }
    let tmp = TempDir::new("from-datasheet");
    let pdf_path = tmp.path().join("d.pdf");
    fs::write(&pdf_path, pdf()).unwrap();
    let config = tmp.path().join("kirpitch.toml");
    fs::write(
        &config,
        "[models.m]\nprovider = \"openai-compatible\"\nbase_url = \"http://localhost:1/v1\"\nmodel = \"t\"\n",
    )
    .unwrap();
    let pins: Vec<Value> = [
        ("1", "OUT1", "output"),
        ("2", "IN1-", "input"),
        ("3", "IN1+", "input"),
        ("4", "GND", "power_in"),
        ("5", "IN2+", "input"),
        ("6", "IN2-", "input"),
        ("7", "OUT2", "output"),
        ("8", "VCC", "power_in"),
    ]
    .iter()
    .map(|(n, name, t)| json!({"number": n, "name": name, "type": t, "page": 2, "confidence": 0.9}))
    .collect();
    let transport = Arc::new(MockTransport::new());
    transport.push_ok(&reply(
        &json!({"mpn": "LM358DR", "manufacturer": "TI", "description": "Op amp"}),
    ));
    transport.push_ok(&reply(&json!({ "pins": pins })));
    transport.push_ok(&reply(&json!({
        "family": "soic", "pin_count": 8, "e": 1.27, "A": 1.75,
        "D": {"min": 4.81, "max": 5.0}, "E1": {"min": 3.81, "max": 3.98},
        "E": {"min": 5.8, "max": 6.19}, "L": {"min": 0.41, "max": 1.27},
        "b": {"min": 0.31, "max": 0.51}, "page": 3, "confidence": 0.9
    })));
    let out = tmp.path().join("lm358.json");
    let events = Mutex::new(Vec::new());
    let outcome = build_with_transport_progress(
        &DatasheetInput {
            pdf: pdf_path.to_string_lossy().into_owned(),
            out: out.to_string_lossy().into_owned(),
            hint: "SOIC-8".into(),
            second_read: false,
            config: config.to_string_lossy().into_owned(),
            cache_dir: tmp.path().join("cache").to_string_lossy().into_owned(),
            ..Default::default()
        },
        transport,
        &|p| events.lock().unwrap().push((p.stage, p.state)),
    )
    .unwrap();
    // Every stage reports when it starts and when it ends, in order. The calls
    // to the models are reported in between: asked, then answered.
    use StageState::{Call, Done, Started};
    let all = events.lock().unwrap().clone();
    for stage in ["metadata", "pins", "package"] {
        let calls = all.iter().filter(|e| e.0 == stage && e.1 == Call).count();
        assert_eq!(calls, 2, "{stage}: one question and one answer");
    }
    let steps: Vec<_> = all.iter().filter(|e| e.1 != Call).cloned().collect();
    assert_eq!(
        steps,
        [
            ("read", Started),
            ("read", Done),
            ("metadata", Started),
            ("metadata", Done),
            ("pins", Started),
            ("pins", Done),
            ("package", Started),
            ("package", Done),
            ("assemble", Started),
            ("assemble", Done),
            ("save", Started),
            ("save", Done),
        ]
    );
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    let view = outcome.view.unwrap();
    assert_eq!(view.mpn, "LM358DR");
    assert_eq!(view.pins.len(), 8);
    assert!(
        outcome
            .models
            .iter()
            .all(|m| m.contains("stays on this machine"))
    );
    assert!(
        outcome.usage.contains("3 model call(s)"),
        "{}",
        outcome.usage
    );
    // A second build into the same file is refused.
    let again = build_with_transport(
        &DatasheetInput {
            pdf: pdf_path.to_string_lossy().into_owned(),
            out: out.to_string_lossy().into_owned(),
            hint: String::new(),
            second_read: false,
            config: config.to_string_lossy().into_owned(),
            cache_dir: tmp.path().join("cache").to_string_lossy().into_owned(),
            ..Default::default()
        },
        Arc::new(MockTransport::new()),
    );
    assert!(again.unwrap_err().contains("already exists"));
}

#[test]
fn a_build_without_a_configured_model_explains_what_to_do() {
    let tmp = TempDir::new("from-datasheet-none");
    let err = build_with_transport(
        &DatasheetInput {
            pdf: "x.pdf".into(),
            out: tmp.path().join("o.json").to_string_lossy().into_owned(),
            hint: String::new(),
            second_read: false,
            config: tmp
                .path()
                .join("missing.toml")
                .to_string_lossy()
                .into_owned(),
            cache_dir: String::new(),
            ..Default::default()
        },
        Arc::new(MockTransport::new()),
    )
    .unwrap_err();
    assert!(err.contains("missing.toml"), "{err}");
}

#[test]
fn a_guided_build_sends_only_the_text_of_the_places_pointed_at() {
    if kirpitch_review::find_pdfium_dir().is_none() {
        eprintln!("SKIPPED: no pdfium library (cargo xtask fetch-pdfium)");
        return;
    }
    use kirpitch_review::{GuideInput, RegionInput, preview_selection, suggest_pages_for};
    let tmp = TempDir::new("guided");
    let pdf_path = tmp.path().join("d.pdf");
    fs::write(&pdf_path, pdf()).unwrap();
    let pdf_text = pdf_path.to_string_lossy().into_owned();
    let config = tmp.path().join("kirpitch.toml");
    fs::write(
        &config,
        "[models.m]\nprovider = \"openai-compatible\"\nbase_url = \"http://localhost:1/v1\"\nmodel = \"t\"\n",
    )
    .unwrap();

    // Suggestions come from the same keywords as the search.
    let s = suggest_pages_for(&pdf_text, "").unwrap();
    assert_eq!(s.page_count, 3);
    assert_eq!(s.pins.first().map(|p| p.page), Some(2), "{:?}", s.pins);

    // The pin table is on page 2, but only its first rows are pointed at.
    let guide = GuideInput {
        pins: vec![RegionInput {
            page: 2,
            bbox: [0.0, 0.10, 1.0, 0.20],
        }],
        ..Default::default()
    };
    let preview = preview_selection(&pdf_text, &guide, "").unwrap();
    assert!(preview.pins.chosen > 0 && preview.pins.chosen < preview.pins.automatic);
    assert_eq!(
        preview.metadata.chosen, 0,
        "a step with no place sends nothing of its own"
    );

    let pins: Vec<Value> = [("1", "OUT1", "output"), ("2", "IN1-", "input")]
        .iter()
        .map(|(n, name, t)| json!({"number": n, "name": name, "type": t, "page": 2, "confidence": 0.9}))
        .collect();
    let transport = Arc::new(MockTransport::new());
    transport.push_ok(&reply(
        &json!({"mpn": "LM358DR", "manufacturer": "TI", "description": "Op amp"}),
    ));
    transport.push_ok(&reply(&json!({ "pins": pins })));
    transport.push_ok(&reply(&json!({
        "family": "soic", "pin_count": 8, "e": 1.27, "A": 1.75,
        "D": {"min": 4.81, "max": 5.0}, "E1": {"min": 3.81, "max": 3.98},
        "E": {"min": 5.8, "max": 6.19}, "L": {"min": 0.41, "max": 1.27},
        "b": {"min": 0.31, "max": 0.51}, "page": 3, "confidence": 0.9
    })));
    build_with_transport(
        &DatasheetInput {
            pdf: pdf_text.clone(),
            out: tmp.path().join("p.json").to_string_lossy().into_owned(),
            config: config.to_string_lossy().into_owned(),
            cache_dir: tmp.path().join("cache").to_string_lossy().into_owned(),
            guide: Some(guide),
            ..Default::default()
        },
        transport.clone(),
    )
    .unwrap();
    let user = |i: usize| {
        let b: Value = serde_json::from_str(&transport.requests()[i].body).unwrap();
        b["messages"][1]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // The pin step saw the rows inside the rectangle and nothing else of the page.
    let pins_prompt = user(1);
    assert!(pins_prompt.contains("OUT1"), "{pins_prompt}");
    assert!(!pins_prompt.contains("OUT2"), "{pins_prompt}");
    assert!(!pins_prompt.contains("Pin Configuration"), "{pins_prompt}");
    assert!(pins_prompt.contains("[page 2]"));
    // The steps with no place were searched for as before: the first pages.
    assert!(user(0).contains("LM358DR Texas Instruments"));
}

#[test]
fn a_region_with_no_text_or_no_size_is_refused_with_the_page_named() {
    if kirpitch_review::find_pdfium_dir().is_none() {
        eprintln!("SKIPPED: no pdfium library (cargo xtask fetch-pdfium)");
        return;
    }
    use kirpitch_review::{RegionInput, text_of_regions};
    let bytes = pdf();
    let empty = text_of_regions(
        &bytes,
        &[RegionInput {
            page: 2,
            bbox: [0.0, 0.95, 1.0, 1.0],
        }],
    );
    assert!(empty.unwrap_err().contains("page 2"));
    let flat = text_of_regions(
        &bytes,
        &[RegionInput {
            page: 1,
            bbox: [0.5, 0.5, 0.5, 0.5],
        }],
    );
    assert!(flat.unwrap_err().contains("empty"));
    assert!(text_of_regions(&bytes, &[]).unwrap().is_none());
}
