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

#[test]
fn hand_written_values_may_omit_sources() {
    let spec = load();
    assert_eq!(spec.origin, kirpitch_core::partspec::Origin::Hand);
    assert!(!findings(&spec).iter().any(|f| f.0 == Check::SourceRequired));
    // The default origin is not written to files.
    assert!(!spec.to_json_string().contains("\"origin\""));
}

#[test]
fn extracted_values_need_page_and_region() {
    use kirpitch_core::partspec::Origin;
    let mut spec = load();
    spec.origin = Origin::Region;
    let f = findings(&spec);
    // The fixture has no sources on pins and pads.
    assert!(f.contains(&(Check::SourceRequired, Level::Error, "pins[0].source".into())));
    assert!(f.contains(&(
        Check::SourceRequired,
        Level::Error,
        "land_pattern.pads[7].source".into()
    )));
    // Sources without a region are not enough either.
    let src = kirpitch_core::Source {
        page: 1,
        bbox: None,
        confidence: None,
    };
    spec.pins[0].source = Some(src);
    let f = findings(&spec);
    assert!(f.contains(&(
        Check::SourceRequired,
        Level::Error,
        "pins[0].source.bbox".into()
    )));
    assert!(!f.contains(&(Check::SourceRequired, Level::Error, "pins[0].source".into())));
}

#[test]
fn model_readings_also_need_a_confidence() {
    use kirpitch_core::partspec::Origin;
    use kirpitch_core::{BoundingBox, Confidence, Source};
    let mut spec = load();
    spec.origin = Origin::Llm;
    let mut src = Source {
        page: 1,
        bbox: Some(BoundingBox([0.1, 0.1, 0.2, 0.2])),
        confidence: None,
    };
    spec.pins[0].source = Some(src);
    let f = findings(&spec);
    assert!(f.contains(&(
        Check::SourceRequired,
        Level::Error,
        "pins[0].source.confidence".into()
    )));
    src.confidence = Some(Confidence::new(0.9).unwrap());
    spec.pins[0].source = Some(src);
    assert!(!findings(&spec).contains(&(
        Check::SourceRequired,
        Level::Error,
        "pins[0].source.confidence".into()
    )));
}

#[test]
fn origin_round_trips_in_json() {
    use kirpitch_core::partspec::Origin;
    let mut spec = load();
    spec.origin = Origin::Llm;
    let text = spec.to_json_string();
    assert!(text.contains("\"origin\": \"llm\""));
    assert_eq!(PartSpec::from_json_str(&text).unwrap().origin, Origin::Llm);
}

#[test]
fn supply_pins_need_power_types_and_there_must_be_a_ground() {
    let mut spec = load();
    // Pin 8 is VCC.
    spec.pins[7].electrical_type = kirpitch_core::ElectricalType::Input;
    let f = findings(&spec);
    assert!(f.contains(&(
        Check::PowerPins,
        Level::Warning,
        "pins[7].electrical_type".into()
    )));

    let mut spec = load();
    spec.pins[3].name = "X".into(); // the only ground pin
    let f = findings(&spec);
    assert!(f.contains(&(Check::PowerPins, Level::Warning, "pins".into())));

    // A part without any power pin is not asked for a ground.
    let mut spec = load();
    spec.pins[3].electrical_type = kirpitch_core::ElectricalType::Passive;
    spec.pins[7].electrical_type = kirpitch_core::ElectricalType::Passive;
    assert!(!findings(&spec).contains(&(Check::PowerPins, Level::Warning, "pins".into())));
}

#[test]
fn pins_of_a_row_must_fit_in_the_body() {
    let mut spec = load();
    // 4 pins per side at 1.27 mm need 3.81 mm; the body is 4.9 mm long.
    assert_eq!(findings(&spec), vec![]);
    spec.package.body.length = mm(3.5);
    assert_eq!(
        findings(&spec),
        vec![(Check::PitchFit, Level::Error, "package.pitch".into())]
    );
}

#[test]
fn a_large_exposed_pad_with_paste_gets_a_warning() {
    let mut spec = load();
    let mut ep = spec.land_pattern.pads[0].clone();
    ep.number = "EP".into();
    ep.x = mm(0.0);
    ep.y = mm(0.0);
    ep.size_x = mm(2.2);
    ep.size_y = mm(2.2);
    spec.land_pattern.pads.push(ep);
    let mut pin = spec.pins[3].clone();
    pin.number = "EP".into();
    pin.name = "EP".into();
    pin.electrical_type = kirpitch_core::ElectricalType::Passive;
    spec.pins.push(pin);
    spec.package.pin_count = 9;
    let f = findings(&spec);
    assert_eq!(
        f,
        vec![(
            Check::ExposedPad,
            Level::Warning,
            "land_pattern.pads[8].paste".into()
        )]
    );
    // The warning can be accepted with a note, or the pad given no paste.
    spec.land_pattern.pads[8].paste = false;
    assert_eq!(findings(&spec), vec![]);
}

#[test]
fn an_override_for_a_missing_check_is_reported() {
    let mut spec = load();
    spec.overrides.push(kirpitch_core::partspec::Override {
        check: "pin_cuont".into(),
        path: String::new(),
        note: "typo".into(),
    });
    assert_eq!(
        findings(&spec),
        vec![(
            Check::OverrideUnknown,
            Level::Warning,
            "overrides[0].check".into()
        )]
    );
}
