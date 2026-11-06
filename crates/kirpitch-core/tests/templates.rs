// SPDX-License-Identifier: GPL-3.0-or-later

//! A filled template plus pins must be a valid PartSpec.

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{DualRowGullWing, dual_row_gull_wing};
use kirpitch_core::{FabProfile, validate};

fn part(pin_count: u32, family: &str, template: DualRowGullWing) -> PartSpec {
    let t = dual_row_gull_wing(&template, Density::Nominal).unwrap();
    PartSpec {
        schema_version: SCHEMA_VERSION,
        origin: Origin::Hand,
        metadata: Metadata {
            mpn: format!("TEST-{family}-{pin_count}"),
            manufacturer: "Test".into(),
            description: "Part made from a template".into(),
            keywords: vec![],
            datasheet_url: None,
            datasheet_sha256: None,
        },
        package: t.package,
        pins: (1..=pin_count)
            .map(|n| Pin {
                number: n.to_string(),
                name: format!("IO{n}"),
                electrical_type: ElectricalType::Passive,
                group: None,
                alt_functions: vec![],
                source: None,
            })
            .collect(),
        land_pattern: t.land_pattern,
        approvals: Approvals::default(),
        overrides: vec![],
    }
}

fn soic(pins: u32, length: f64) -> DualRowGullWing {
    DualRowGullWing {
        family: "SOIC".into(),
        pin_count: pins,
        pitch: 1.27,
        body_length: length,
        body_width: 3.9,
        height_max: 1.75,
        lead_span: Range::new(5.8, 6.2),
        lead_length: Range::new(0.4, 1.27),
        lead_width: Range::new(0.31, 0.51),
    }
}

#[test]
fn soic_templates_make_valid_partspecs() {
    for (pins, length) in [(8, 4.9), (14, 8.7), (16, 9.9)] {
        let spec = part(pins, "SOIC", soic(pins, length));
        let findings = validate(&spec, &FabProfile::generic());
        assert!(findings.is_empty(), "SOIC-{pins}: {findings:?}");
    }
}

#[test]
fn tssop_template_makes_a_valid_partspec() {
    let spec = part(
        14,
        "TSSOP",
        DualRowGullWing {
            family: "TSSOP".into(),
            pin_count: 14,
            pitch: 0.65,
            body_length: 5.0,
            body_width: 4.4,
            height_max: 1.2,
            lead_span: Range::exact(6.4),
            lead_length: Range::new(0.45, 0.75),
            lead_width: Range::new(0.17, 0.30),
        },
    );
    let findings = validate(&spec, &FabProfile::generic());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_body_that_is_too_short_for_the_pins_is_caught() {
    // 16 pins at 1.27 mm need 8.9 mm, the body is 5 mm long.
    let spec = part(16, "SOIC", soic(16, 5.0));
    let findings = validate(&spec, &FabProfile::generic());
    assert!(findings.iter().any(|f| f.check.id() == "pitch_fit"));
}
