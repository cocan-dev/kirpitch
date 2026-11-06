// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a datasheet one step at a time, with confirmed values.

use std::sync::{Arc, Mutex};

use kirpitch_core::{FabProfile, has_errors, validate};
use kirpitch_models::config::Config;
use kirpitch_models::steps::{
    Step, assemble_confirmed, check_step, pin_types, preview_footprint, run_step,
};
use kirpitch_models::{BuildOptions, Cache, MemoryKeyStore, MockTransport, PageLines, TextLine};
use serde_json::{Value, json};

fn config() -> Config {
    Config::from_toml_str(
        "[models.m]\nprovider = \"openai-compatible\"\nbase_url = \"http://localhost:1234/v1\"\nmodel = \"test\"\n",
    )
    .unwrap()
}

fn reply(content: &Value) -> String {
    json!({
        "choices": [{"message": {"content": content.to_string()}}],
        "usage": {"prompt_tokens": 100, "completion_tokens": 20}
    })
    .to_string()
}

fn page(n: u32, lines: &[&str]) -> PageLines {
    PageLines {
        page: n,
        lines: lines
            .iter()
            .enumerate()
            .map(|(i, t)| TextLine {
                text: (*t).to_string(),
                y: 0.1 + 0.05 * i as f64,
                left: 0.1,
                right: 0.9,
            })
            .collect(),
    }
}

fn pin_text() -> Vec<PageLines> {
    vec![page(
        48,
        &["1 VBAT S", "2 PC13 I/O", "3 PC14 I/O", "4 PC15 I/O"],
    )]
}

fn pins_value() -> Value {
    json!({"pins": [
        {"number": "1", "name": "VBAT", "type": "power_in"},
        {"number": "2", "name": "PC13", "type": "bidirectional"},
        {"number": "3", "name": "PC14", "type": "bidirectional"},
        {"number": "4", "name": "PC15", "type": "bidirectional"},
    ]})
}

fn lqfp_value() -> Value {
    json!({"family": "lqfp", "pin_count": 64, "e": 0.5, "A": 1.6, "A1": 0.1, "A2": 1.4, "c": 0.15,
           "D": {"min": 11.8, "max": 12.2}, "E": {"min": 11.8, "max": 12.2},
           "D1": {"min": 9.9, "max": 10.1}, "E1": {"min": 9.9, "max": 10.1},
           "L": {"min": 0.45, "max": 0.75}, "b": {"min": 0.17, "max": 0.27}, "page": 5})
}

#[test]
fn a_step_is_read_from_the_text_given_and_keeps_the_answer_for_confirmation() {
    let m = Arc::new(MockTransport::new());
    m.push_ok(&reply(
        &json!({"mpn": "STM32F405RGT6", "manufacturer": "STMicroelectronics", "description": "MCU"}),
    ));
    let calls = Mutex::new(Vec::new());
    let run = run_step(
        &config(),
        m.clone(),
        &Cache::none(),
        &MemoryKeyStore::default(),
        Step::Metadata,
        &[page(1, &["STM32F405xx", "Arm Cortex-M4"])],
        true,
        None,
        false,
        &|p| {
            if let Some(c) = &p.call {
                calls.lock().unwrap().push(c.phase.clone());
            }
        },
    )
    .unwrap();
    assert_eq!(run.value["mpn"], "STM32F405RGT6");
    // Asked once, of the text given, and the call was reported.
    assert_eq!(m.requests().len(), 1);
    assert!(m.requests()[0].body.contains("Arm Cortex-M4"));
    assert_eq!(*calls.lock().unwrap(), ["sent", "received"]);
}

#[test]
fn pins_are_read_with_their_place_and_a_wrapped_answer_is_kept_unwrapped() {
    let m = Arc::new(MockTransport::new());
    m.push_ok(&reply(&json!({"answer": pins_value()})));
    let run = run_step(
        &config(),
        m,
        &Cache::none(),
        &MemoryKeyStore::default(),
        Step::Pins,
        &pin_text(),
        true,
        None,
        false,
        &|_| {},
    )
    .unwrap();
    let pins = run.value["pins"].as_array().unwrap();
    assert_eq!(pins.len(), 4);
    assert_eq!(pins[0]["grounded"], true);
    assert_eq!(pins[0]["page"], 48);
    assert!(pins[0]["bbox"].is_array());
}

#[test]
fn a_step_that_cannot_be_read_is_an_error_the_person_can_answer_by_hand() {
    let m = Arc::new(MockTransport::new());
    for _ in 0..3 {
        m.push_ok(&reply(&json!({"nonsense": true})));
    }
    let e = run_step(
        &config(),
        m,
        &Cache::none(),
        &MemoryKeyStore::default(),
        Step::Package,
        &[page(5, &["Package outline"])],
        true,
        None,
        false,
        &|_| {},
    )
    .unwrap_err();
    assert!(e.contains("no usable answer"), "{e}");
}

#[test]
fn typed_in_values_go_through_the_same_checks_with_clear_messages() {
    // Metadata.
    assert!(
        check_step(
            Step::Metadata,
            &json!({"mpn": "X", "manufacturer": "", "description": "d"}),
            &[]
        )
        .unwrap_err()
        .contains("manufacturer")
    );
    // Pins: a duplicate, a bad type, an empty name.
    let dup = json!({"pins": [{"number": "1", "name": "A", "type": "input"}, {"number": "1", "name": "B", "type": "input"}]});
    assert!(
        check_step(Step::Pins, &dup, &[])
            .unwrap_err()
            .contains("twice")
    );
    let bad = json!({"pins": [{"number": "1", "name": "A", "type": "loud"}]});
    assert!(
        check_step(Step::Pins, &bad, &[])
            .unwrap_err()
            .contains("not one of")
    );
    let empty = json!({"pins": [{"number": "1", "name": "  ", "type": "input"}]});
    assert!(check_step(Step::Pins, &empty, &[]).is_err());
    // A pin that is not in the text is flagged, one that is gets its place.
    let mixed = json!({"pins": [
        {"number": "1", "name": "VBAT", "type": "power_in"},
        {"number": "9", "name": "ZZZ", "type": "input"}]});
    let r = check_step(Step::Pins, &mixed, &pin_text()).unwrap();
    assert_eq!(r.value["pins"][0]["grounded"], true);
    assert_eq!(r.value["pins"][1]["grounded"], false);
    assert!(r.notes.iter().any(|n| n.contains("ZZZ")), "{:?}", r.notes);
    // The person confirmed them: full confidence.
    assert_eq!(r.value["pins"][1]["confidence"], 1.0);
    // Package and footprint.
    assert!(check_step(Step::Package, &json!({"family": "lqfp", "A": 1.6}), &[]).is_err());
    assert!(check_step(Step::Package, &lqfp_value(), &[]).is_ok());
    assert!(check_step(Step::Footprint, &json!({"pad_length": 1.5}), &[]).is_err());
    assert!(
        check_step(
            Step::Footprint,
            &json!({"pad_length": 1.5, "pad_width": 0.3}),
            &[]
        )
        .is_ok()
    );
    assert!(Step::from_name("nope").is_err());
    assert!(pin_types().contains(&"power_in") && pin_types().contains(&"bidirectional"));
}

#[test]
fn the_footprint_can_be_looked_at_before_it_is_confirmed() {
    let calculated = preview_footprint(&lqfp_value(), None).unwrap();
    assert_eq!(calculated.name, "LQFP-64_10x10mm_P0.5mm");
    assert_eq!(calculated.pads.len(), 64);
    assert!(calculated.notes.iter().any(|n| n.contains("calculated")));
    let fp = json!({"pad_length": 1.6, "pad_width": 0.3, "center_x": 11.3, "center_y": 11.3});
    let drawn = preview_footprint(&lqfp_value(), Some(&fp)).unwrap();
    assert_eq!(drawn.pads.len(), 64);
    let p = &drawn.pads[0];
    assert!(
        (p.size_x - 1.6).abs() < 1e-9 && (p.x.abs() - 5.65).abs() < 1e-9,
        "{p:?}"
    );
    assert!(
        drawn
            .notes
            .iter()
            .any(|n| n.contains("recommended footprint"))
    );
    // A package with no template and no pads cannot be shown.
    let custom = json!({"family": "custom", "package_name": "SOT-223", "A": 1.8,
        "D": {"min": 6.3, "max": 6.7}, "E1": {"min": 3.3, "max": 3.7}});
    assert!(preview_footprint(&custom, None).is_err());
}

#[test]
fn the_part_is_assembled_from_confirmed_values_with_no_call_to_a_model() {
    let names: Vec<Value> = (1..=64)
        .map(|i| json!({"number": i.to_string(), "name": format!("P{i}"), "type": "passive"}))
        .collect();
    let spec = assemble_confirmed(
        &json!({"mpn": "STM32F405RGT6", "manufacturer": "ST", "description": "MCU"}),
        &json!({ "pins": names }),
        &lqfp_value(),
        Some(
            &json!({"pad_length": 1.6, "pad_width": 0.3, "center_x": 11.3, "center_y": 11.3, "page": 50}),
        ),
        &[],
        &BuildOptions::default(),
    );
    let (spec, notes) = spec.unwrap();
    assert_eq!(spec.package.name, "LQFP-64_10x10mm_P0.5mm");
    assert_eq!(spec.pins.len(), 64);
    assert!(notes.iter().any(|n| n.contains("recommended footprint")));
    assert!(!has_errors(&validate(&spec, &FabProfile::generic())));
    // Pieces that do not fit are refused with the reason.
    let few: Vec<Value> = names_head(10);
    let e = assemble_confirmed(
        &json!({"mpn": "X", "manufacturer": "ST", "description": "MCU"}),
        &json!({ "pins": few }),
        &lqfp_value(),
        None,
        &[],
        &BuildOptions::default(),
    )
    .unwrap_err();
    assert!(e.contains("10 pins") && e.contains("64"), "{e}");
}

fn names_head(n: u32) -> Vec<Value> {
    (1..=n)
        .map(|i| json!({"number": i.to_string(), "name": format!("P{i}"), "type": "passive"}))
        .collect()
}
