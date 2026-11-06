// SPDX-License-Identifier: GPL-3.0-or-later

//! Symbol generation for the SOIC-8 example PartSpec.

use std::path::Path;
use std::time::Instant;

use kirpitch_core::PartSpec;
use kirpitch_core::partspec::{Pad, Pin};
use kirpitch_gen_symbol::{GenerateError, Options, file_name, generate};
use kirpitch_kicad::KicadTarget;
use kirpitch_kicad::sexpr::check_round_trip;
use kirpitch_kicad::testing::{assert_snapshot, assert_symbol_loads, kicad_cli_or_skip};

const SPEC: &str = include_str!("../../kirpitch-core/tests/fixtures/soic8_example.json");

fn spec() -> PartSpec {
    PartSpec::from_json_str(SPEC).expect("fixture loads")
}

fn output() -> String {
    generate(&spec(), &Options::default()).expect("generates")
}

#[test]
fn matches_the_snapshot() {
    assert_snapshot(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/soic8_example.kicad_sym"),
        &output(),
    );
}

#[test]
fn is_deterministic_and_uses_unix_line_endings() {
    let a = output();
    assert_eq!(a, output());
    assert!(!a.contains('\r'));
}

#[test]
fn is_fast() {
    let start = Instant::now();
    let _ = output();
    assert!(start.elapsed().as_secs_f64() < 1.0);
}

#[test]
fn parses_back_to_the_same_text_with_the_target_versions() {
    let text = output();
    check_round_trip(&text).expect("canonical layout");
    assert!(text.contains(&format!(
        "(version {})",
        KicadTarget::V10.symbol_format_version()
    )));
}

#[test]
fn file_name_uses_the_part_number() {
    assert_eq!(file_name(&spec()), "EXAMPLE-DUAL-OPAMP.kicad_sym");
}

/// `(pin <type> line (at x y angle)` with the pin number, for assertions.
fn pin_line(text: &str, number: &str) -> String {
    let marker = format!("(number \"{number}\"");
    let end = text.find(&marker).expect("pin number present");
    let start = text[..end].rfind("(pin ").expect("pin start");
    let block = &text[start..end];
    let at = block.find("(at ").unwrap();
    let close = block[at..].find(')').unwrap();
    let kind = block["(pin ".len()..].split_whitespace().next().unwrap();
    format!("{kind} {}", &block[at..at + close + 1])
}

#[test]
fn pins_are_placed_by_type_on_the_grid() {
    let text = output();
    // Left: inputs, in PartSpec order, with a gap between the two amplifiers.
    assert_eq!(pin_line(&text, "2"), "input (at -7.62 5.08 0)");
    assert_eq!(pin_line(&text, "3"), "input (at -7.62 2.54 0)");
    assert_eq!(pin_line(&text, "5"), "input (at -7.62 -2.54 0)");
    assert_eq!(pin_line(&text, "6"), "input (at -7.62 -5.08 0)");
    // Right: outputs.
    assert_eq!(pin_line(&text, "1"), "output (at 7.62 5.08 180)");
    assert_eq!(pin_line(&text, "7"), "output (at 7.62 0 180)");
    // Top: positive supply, bottom: ground.
    assert_eq!(pin_line(&text, "8"), "power_in (at 0 10.16 270)");
    assert_eq!(pin_line(&text, "4"), "power_in (at 0 -10.16 90)");
    // The body is on the grid.
    assert!(text.contains("(start -5.08 7.62)"));
    assert!(text.contains("(end 5.08 -7.62)"));
}

#[test]
fn every_pin_is_on_the_2_54_grid() {
    let text = output();
    for number in ["1", "2", "3", "4", "5", "6", "7", "8"] {
        let line = pin_line(&text, number);
        let nums: Vec<f64> = line
            .trim_start_matches(|c: char| c != '(')
            .trim_matches(|c| c == '(' || c == ')')
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse().unwrap())
            .collect();
        for v in &nums[..2] {
            let steps = v / 2.54;
            assert!((steps - steps.round()).abs() < 1e-6, "{line} is off grid");
        }
    }
}

#[test]
fn fields_carry_the_metadata() {
    let text = output();
    assert!(text.contains("(symbol \"EXAMPLE-DUAL-OPAMP\""));
    assert!(text.contains("(property \"Value\" \"EXAMPLE-DUAL-OPAMP\""));
    assert!(text.contains("(property \"MPN\" \"EXAMPLE-DUAL-OPAMP\""));
    assert!(text.contains("(property \"Manufacturer\" \"Example Manufacturer\""));
    assert!(text.contains("(property \"Datasheet\" \"\""));
    assert!(text.contains("(property \"ki_keywords\" \"opamp example\""));
    assert!(text.contains("(property \"ki_fp_filters\" \"SOIC*\""));
}

/// The example with a second GND pin (number 9).
fn stacked_spec() -> PartSpec {
    let mut s = spec();
    let mut second_ground: Pin = s.pins[3].clone();
    second_ground.number = "9".into();
    s.pins.push(second_ground);
    let mut pad: Pad = s.land_pattern.pads[3].clone();
    pad.number = "9".into();
    pad.x = kirpitch_core::Mm::new(0.0).unwrap();
    pad.y = kirpitch_core::Mm::new(0.0).unwrap();
    s.land_pattern.pads.push(pad);
    s.package.pin_count = 9;
    s
}

#[test]
fn equal_named_power_pins_are_stacked_and_all_but_the_first_hidden() {
    let s = stacked_spec();
    let text = generate(&s, &Options::default()).unwrap();
    assert_eq!(pin_line(&text, "4"), "power_in (at 0 -10.16 90)");
    assert_eq!(pin_line(&text, "9"), "power_in (at 0 -10.16 90)");
    assert_eq!(text.matches("(hide yes)").count(), 7 + 1); // seven hidden fields + one pin
    check_round_trip(&text).expect("canonical layout");
}

#[test]
fn long_names_make_the_body_wider() {
    let mut s = spec();
    s.pins[1].name = "VERY_LONG_INVERTING_INPUT".into();
    let text = generate(&s, &Options::default()).unwrap();
    // The body width is a multiple of 5.08 mm, here 30.48 mm.
    assert!(text.contains("(start -15.24 7.62)"));
}

#[test]
fn invalid_partspec_is_reported_by_field() {
    let mut s = spec();
    s.pins[1].number = "1".into();
    match generate(&s, &Options::default()).unwrap_err() {
        GenerateError::Invalid(findings) => {
            assert!(findings.iter().any(|f| f.path == "pins[1].number"));
        }
    }
}

#[test]
fn kicad_loads_the_symbol() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    check_with_kicad(&cli, "gen-sym", &output(), &file_name(&spec()));
}

#[test]
fn kicad_loads_the_symbol_with_stacked_pins() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let s = stacked_spec();
    let text = generate(&s, &Options::default()).unwrap();
    check_with_kicad(&cli, "gen-sym-stacked", &text, &file_name(&s));
}

fn check_with_kicad(cli: &kirpitch_kicad::KicadCli, label: &str, text: &str, file_name: &str) {
    assert_symbol_loads(cli, label, text, file_name);
}
