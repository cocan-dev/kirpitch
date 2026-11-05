// SPDX-License-Identifier: GPL-3.0-or-later

//! Loading and validating the SOIC-8 example PartSpec.

use kirpitch_core::partspec::{PadRole, PartSpec};
use kirpitch_core::{Check, FabProfile, LengthUnit, Level, LoadError, Mm, validate};

const FIXTURE: &str = include_str!("fixtures/soic8_example.json");

fn load() -> PartSpec {
    PartSpec::from_json_str(FIXTURE).expect("fixture loads")
}

fn findings(spec: &PartSpec) -> Vec<(Check, Level, String)> {
    validate(spec, &FabProfile::generic())
        .into_iter()
        .map(|f| (f.check, f.level, f.path))
        .collect()
}

fn mm(v: f64) -> Mm {
    Mm::new(v).unwrap()
}

#[test]
fn fixture_is_valid_and_has_no_findings() {
    let spec = load();
    assert_eq!(spec.pins.len(), 8);
    assert_eq!(spec.land_pattern.pads.len(), 8);
    assert_eq!(findings(&spec), vec![]);
}

#[test]
fn json_round_trip_is_stable() {
    let spec = load();
    let text = spec.to_json_string();
    assert!(text.ends_with("}\n"));
    let again = PartSpec::from_json_str(&text).unwrap();
    assert_eq!(again, spec);
    assert_eq!(again.to_json_string(), text);
}

#[test]
fn unknown_field_error_names_the_path() {
    let text = FIXTURE.replace("\"pitch\": 1.27", "\"pitch\": 1.27, \"pich\": 1");
    let err = PartSpec::from_json_str(&text).unwrap_err();
    match err {
        LoadError::Invalid { path, message } => {
            assert_eq!(path, "package.pich");
            assert!(message.contains("pich"), "{message}");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn wrong_type_error_names_the_path() {
    let text = FIXTURE.replacen(
        "\"electrical_type\": \"output\"",
        "\"electrical_type\": \"outpt\"",
        1,
    );
    let err = PartSpec::from_json_str(&text).unwrap_err();
    match err {
        LoadError::Invalid { path, .. } => assert_eq!(path, "pins[0].electrical_type"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unsupported_version_is_rejected() {
    let text = FIXTURE.replace("\"schema_version\": 1", "\"schema_version\": 2");
    assert_eq!(
        PartSpec::from_json_str(&text).unwrap_err(),
        LoadError::UnsupportedVersion {
            found: 2,
            supported: 1
        }
    );
}

#[test]
fn trailing_garbage_is_rejected() {
    let text = format!("{FIXTURE} x");
    assert!(PartSpec::from_json_str(&text).is_err());
}

#[test]
fn pin_count_mismatch_is_an_error() {
    let mut spec = load();
    spec.package.pin_count = 9;
    assert_eq!(
        findings(&spec),
        vec![(Check::PinCount, Level::Error, "pins".into())]
    );
}

#[test]
fn duplicate_and_empty_pin_numbers() {
    let mut spec = load();
    spec.pins[1].number = "1".into();
    spec.pins[2].number = " ".into();
    let f = findings(&spec);
    assert!(f.contains(&(Check::PinNumber, Level::Error, "pins[1].number".into())));
    assert!(f.contains(&(Check::PinNumber, Level::Error, "pins[2].number".into())));
}

#[test]
fn repeated_names_only_for_power_and_no_connect() {
    let mut spec = load();
    spec.pins[2].name = "-IN1".into();
    let f = findings(&spec);
    assert!(f.contains(&(Check::PinName, Level::Error, "pins[2].name".into())));

    // Two ground pins with the same name are fine.
    let mut spec = load();
    spec.pins[7].name = "GND".into();
    assert!(!findings(&spec).iter().any(|f| f.0 == Check::PinName));
}

#[test]
fn pin_without_pad_and_pad_without_pin() {
    let mut spec = load();
    spec.land_pattern.pads[0].number = "9".into();
    let f = findings(&spec);
    assert!(f.contains(&(Check::PinPadMatch, Level::Error, "pins[0].number".into())));
    assert!(f.contains(&(
        Check::PinPadMatch,
        Level::Error,
        "land_pattern.pads[0].number".into()
    )));

    // A mechanical pad needs no pin.
    spec.land_pattern.pads[0].role = PadRole::Mechanical;
    let f = findings(&spec);
    assert!(!f.contains(&(
        Check::PinPadMatch,
        Level::Error,
        "land_pattern.pads[0].number".into()
    )));
}

#[test]
fn tolerance_order_is_checked() {
    let mut spec = load();
    spec.package.dimensions.get_mut("A1").unwrap().value.min = Some(mm(0.5));
    assert_eq!(
        findings(&spec),
        vec![(
            Check::ToleranceOrder,
            Level::Error,
            "package.dimensions.A1.value".into()
        )]
    );
}

#[test]
fn converted_units_give_a_warning_that_can_be_overridden() {
    let mut spec = load();
    spec.package.dimensions.get_mut("b").unwrap().converted_from = Some(LengthUnit::Mil);
    let found = validate(&spec, &FabProfile::generic());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].level, Level::Warning);
    assert!(!found[0].blocks());
    assert_eq!(found[0].overridden_by, None);

    spec.overrides.push(kirpitch_core::partspec::Override {
        check: "units".into(),
        path: String::new(),
        note: "checked against the drawing".into(),
    });
    let found = validate(&spec, &FabProfile::generic());
    assert_eq!(
        found[0].overridden_by.as_deref(),
        Some("checked against the drawing")
    );
}

#[test]
fn pad_clearance_uses_the_fab_profile() {
    let mut spec = load();
    // Pads of pins 1 and 2 are 1.27 mm apart and 0.6 mm tall: gap 0.67 mm.
    assert_eq!(findings(&spec), vec![]);
    spec.land_pattern.pads[0].size_y = mm(1.6); // gap shrinks to 0.17 mm
    let f = findings(&spec);
    assert_eq!(
        f,
        vec![(
            Check::PadClearance,
            Level::Error,
            "land_pattern.pads[1]".into()
        )]
    );
    // A stricter or looser profile changes the result.
    let loose = FabProfile {
        min_copper_gap: mm(0.05),
        ..FabProfile::generic()
    };
    assert!(validate(&spec, &loose).is_empty());
}

#[test]
fn pads_with_the_same_number_may_touch() {
    let mut spec = load();
    let mut copy = spec.land_pattern.pads[0].clone();
    copy.paste = false;
    spec.land_pattern.pads.push(copy);
    assert_eq!(findings(&spec), vec![]);
}

#[test]
fn zero_sizes_and_bad_sources_are_errors() {
    let mut spec = load();
    spec.package.pitch = mm(0.0);
    spec.land_pattern.pads[3].size_x = mm(-1.0);
    spec.pins[0].source = Some(kirpitch_core::Source {
        page: 0,
        bbox: Some(kirpitch_core::BoundingBox([0.5, 0.5, 0.1, 0.1])),
        confidence: None,
    });
    let f = findings(&spec);
    assert!(f.contains(&(Check::PositiveValue, Level::Error, "package.pitch".into())));
    assert!(f.contains(&(
        Check::PositiveValue,
        Level::Error,
        "land_pattern.pads[3].size_x".into()
    )));
    assert!(f.contains(&(
        Check::SourceValid,
        Level::Error,
        "pins[0].source.page".into()
    )));
    assert!(f.contains(&(
        Check::SourceValid,
        Level::Error,
        "pins[0].source.bbox".into()
    )));
}
