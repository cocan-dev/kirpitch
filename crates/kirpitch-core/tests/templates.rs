// SPDX-License-Identifier: GPL-3.0-or-later

//! A filled template plus pins must be a valid PartSpec.

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{DualRowGullWing, dual_row_gull_wing};
use kirpitch_core::{FabProfile, validate};

fn part_from(t: kirpitch_core::templates::Template, pin_count: u32, family: &str) -> PartSpec {
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

fn part(pin_count: u32, family: &str, template: DualRowGullWing) -> PartSpec {
    part_from(
        dual_row_gull_wing(&template, Density::Nominal).unwrap(),
        pin_count,
        family,
    )
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

fn sot(pins: u32) -> DualRowGullWing {
    // JEDEC MO-178 (SOT-23-5 and SOT-23-6).
    DualRowGullWing {
        family: "SOT-23".into(),
        pin_count: pins,
        pitch: 0.95,
        body_length: 2.9,
        body_width: 1.6,
        height_max: 1.45,
        lead_span: Range::exact(2.8),
        lead_length: Range::new(0.3, 0.6),
        lead_width: Range::new(0.3, 0.5),
    }
}

fn pad_at(t: &kirpitch_core::templates::Template, number: &str) -> (f64, f64) {
    let p = t
        .land_pattern
        .pads
        .iter()
        .find(|p| p.number == number)
        .unwrap();
    (p.x.rounded(), p.y.rounded())
}

#[test]
fn sot23_5_matches_the_official_footprint() {
    use kirpitch_core::templates::sot23;
    let t = sot23(&sot(5), Density::Nominal).unwrap();
    assert_eq!(t.package.name, "SOT-23-5");
    // KiCad: SOT-23-5 pads are 1.325 x 0.6 at x = +-1.1375.
    assert_eq!(pad_at(&t, "1"), (-1.1375, -0.95));
    assert_eq!(pad_at(&t, "2"), (-1.1375, 0.0));
    assert_eq!(pad_at(&t, "3"), (-1.1375, 0.95));
    assert_eq!(pad_at(&t, "4"), (1.1375, 0.95));
    assert_eq!(pad_at(&t, "5"), (1.1375, -0.95));
    let p = &t.land_pattern.pads[0];
    assert_eq!(
        (p.size_x.format().as_str(), p.size_y.format().as_str()),
        ("1.325", "0.6")
    );
}

#[test]
fn sot23_6_and_3_have_the_official_pin_positions() {
    use kirpitch_core::templates::sot23;
    let t6 = sot23(&sot(6), Density::Nominal).unwrap();
    assert_eq!(t6.package.name, "SOT-23-6");
    assert_eq!(pad_at(&t6, "5"), (1.1375, 0.0));
    assert_eq!(pad_at(&t6, "6"), (1.1375, -0.95));
    let t3 = sot23(&sot(3), Density::Nominal).unwrap();
    assert_eq!(t3.package.name, "SOT-23");
    // Pins 1 and 2 on the left, pin 3 alone in the middle of the right side.
    assert_eq!(pad_at(&t3, "1").1, -0.95);
    assert_eq!(pad_at(&t3, "2").1, 0.95);
    assert_eq!(pad_at(&t3, "3").1, 0.0);
    assert!(pad_at(&t3, "3").0 > 0.0);
}

#[test]
fn sot23_only_supports_three_five_and_six_leads() {
    use kirpitch_core::templates::{TemplateError, sot23};
    assert_eq!(
        sot23(&sot(4), Density::Nominal).unwrap_err(),
        TemplateError::PinCount(4)
    );
}

#[test]
fn sot23_templates_make_valid_partspecs() {
    use kirpitch_core::templates::sot23;
    for pins in [3u32, 5, 6] {
        let t = sot23(&sot(pins), Density::Nominal).unwrap();
        let spec = part_from(t, pins, "SOT-23");
        let findings = validate(&spec, &FabProfile::generic());
        assert!(findings.is_empty(), "SOT-23-{pins}: {findings:?}");
    }
}

fn dip(pins: u32, row: f64) -> kirpitch_core::templates::DualRowThroughHole {
    kirpitch_core::templates::DualRowThroughHole {
        family: "DIP".into(),
        pin_count: pins,
        pitch: 2.54,
        row_spacing: row,
        body_length: 9.27,
        body_width: 6.35,
        height_max: 5.08,
        pad_diameter: 1.6,
        drill: 0.8,
    }
}

#[test]
fn dip8_matches_the_official_footprint() {
    use kirpitch_core::partspec::{PadKind, PadShape};
    use kirpitch_core::templates::dual_row_through_hole;
    let t = dual_row_through_hole(&dip(8, 7.62)).unwrap();
    assert_eq!(t.package.name, "DIP-8_W7.62mm");
    // KiCad: pin 1 at the origin, pins 1 to 4 down, 5 to 8 up on the right.
    assert_eq!(pad_at(&t, "1"), (0.0, 0.0));
    assert_eq!(pad_at(&t, "4"), (0.0, 7.62));
    assert_eq!(pad_at(&t, "5"), (7.62, 7.62));
    assert_eq!(pad_at(&t, "8"), (7.62, 0.0));
    let p1 = &t.land_pattern.pads[0];
    assert_eq!(p1.shape, PadShape::Rect);
    assert_eq!(p1.kind, PadKind::ThruHole);
    assert_eq!(p1.drill.unwrap().format(), "0.8");
    assert_eq!(t.land_pattern.pads[1].shape, PadShape::Oval);
    let c = t.package.body.center.unwrap();
    assert_eq!(
        (c.x.format().as_str(), c.y.format().as_str()),
        ("3.81", "3.81")
    );
}

#[test]
fn wide_dips_have_their_row_spacing_in_the_name() {
    use kirpitch_core::templates::dual_row_through_hole;
    let t = dual_row_through_hole(&dip(16, 15.24)).unwrap();
    assert_eq!(t.package.name, "DIP-16_W15.24mm");
    assert_eq!(pad_at(&t, "16"), (15.24, 0.0));
    assert_eq!(pad_at(&t, "9"), (15.24, 17.78));
}

#[test]
fn dip_templates_make_valid_partspecs() {
    use kirpitch_core::templates::dual_row_through_hole;
    for (pins, row) in [(8, 7.62), (14, 7.62), (16, 7.62), (28, 15.24)] {
        let mut d = dip(pins, row);
        d.body_length = (pins / 2 - 1) as f64 * 2.54 + 2.54;
        let spec = part_from(dual_row_through_hole(&d).unwrap(), pins, "DIP");
        let findings = validate(&spec, &FabProfile::generic());
        assert!(findings.is_empty(), "DIP-{pins}: {findings:?}");
    }
}

#[test]
fn bad_dip_dimensions_are_rejected() {
    use kirpitch_core::templates::{TemplateError, dual_row_through_hole};
    let mut d = dip(8, 7.62);
    d.drill = 1.6;
    assert_eq!(
        dual_row_through_hole(&d).unwrap_err(),
        TemplateError::BadValue("drill")
    );
    let mut d = dip(8, 7.62);
    d.pad_diameter = 8.0;
    assert_eq!(
        dual_row_through_hole(&d).unwrap_err(),
        TemplateError::BadValue("pad_diameter")
    );
    assert_eq!(
        dual_row_through_hole(&dip(5, 7.62)).unwrap_err(),
        TemplateError::PinCount(5)
    );
}

#[test]
fn drills_are_checked() {
    use kirpitch_core::partspec::PadKind;
    use kirpitch_core::templates::dual_row_through_hole;
    let mut spec = part_from(dual_row_through_hole(&dip(8, 7.62)).unwrap(), 8, "DIP");
    let ok = validate(&spec, &FabProfile::generic());
    assert!(ok.is_empty(), "{ok:?}");

    spec.land_pattern.pads[0].drill = None;
    spec.land_pattern.pads[1].drill = Some(kirpitch_core::Mm::new(2.0).unwrap());
    spec.land_pattern.pads[2].kind = PadKind::Smd; // still has a drill
    let found = validate(&spec, &FabProfile::generic());
    let paths: Vec<&str> = found
        .iter()
        .filter(|f| f.check.id() == "drill")
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "land_pattern.pads[0].drill",
            "land_pattern.pads[1].drill",
            "land_pattern.pads[2].drill"
        ]
    );
}
