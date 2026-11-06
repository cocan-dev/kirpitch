// SPDX-License-Identifier: GPL-3.0-or-later

//! QFN templates compared with KiCad's official footprints.

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{QuadFlatNoLead, Template, quad_flat_no_lead};
use kirpitch_core::{FabProfile, validate};

fn qfn32() -> QuadFlatNoLead {
    // QFN-32-1EP_4x4mm_P0.4mm_EP2.65x2.65mm
    QuadFlatNoLead {
        family: "QFN".into(),
        pins_x: 8,
        pins_y: 8,
        pitch: 0.4,
        body_x: Range::new(3.95, 4.05),
        body_y: Range::new(3.95, 4.05),
        height_max: 1.0,
        lead_length: Range::new(0.35, 0.45),
        lead_width: Range::new(0.15, 0.25),
        exposed_pad: Some((2.65, 2.65)),
    }
}

fn pad(t: &Template, number: &str) -> (f64, f64, f64, f64) {
    let p = t
        .land_pattern
        .pads
        .iter()
        .find(|p| p.number == number)
        .unwrap();
    (
        p.x.rounded(),
        p.y.rounded(),
        p.size_x.rounded(),
        p.size_y.rounded(),
    )
}

#[test]
fn qfn32_matches_the_official_footprint() {
    let t = quad_flat_no_lead(&qfn32(), Density::Nominal).unwrap();
    assert_eq!(t.package.name, "QFN-32-1EP_4x4mm_P0.4mm_EP2.65x2.65mm");
    assert_eq!(t.package.pin_count, 33);
    assert_eq!(t.land_pattern.pads.len(), 33);
    // KiCad: pin 1 at (-1.95, -1.4), pin 2 at (-1.95, -1), 0.8 x 0.2.
    assert_eq!(pad(&t, "1"), (-1.95, -1.4, 0.8, 0.2));
    assert_eq!(pad(&t, "2"), (-1.95, -1.0, 0.8, 0.2));
    // Pin 9 starts the bottom side, pin 17 the right side, pin 25 the top side.
    assert_eq!(pad(&t, "9"), (-1.4, 1.95, 0.2, 0.8));
    assert_eq!(pad(&t, "17"), (1.95, 1.4, 0.8, 0.2));
    assert_eq!(pad(&t, "25"), (1.4, -1.95, 0.2, 0.8));
    assert_eq!(pad(&t, "33"), (0.0, 0.0, 2.65, 2.65));
}

#[test]
fn a_qfn_without_exposed_pad_has_no_ep_in_its_name() {
    let mut q = qfn32();
    q.exposed_pad = None;
    let t = quad_flat_no_lead(&q, Density::Nominal).unwrap();
    assert_eq!(t.package.name, "QFN-32_4x4mm_P0.4mm");
    assert_eq!(t.package.pin_count, 32);
}

#[test]
fn rectangular_packages_use_each_body_size_for_its_sides() {
    let q = QuadFlatNoLead {
        pins_x: 4,
        pins_y: 6,
        body_x: Range::new(2.95, 3.05),
        body_y: Range::new(4.95, 5.05),
        exposed_pad: None,
        ..qfn32()
    };
    let t = quad_flat_no_lead(&q, Density::Nominal).unwrap();
    assert_eq!(t.package.pin_count, 20);
    let (left_x, ..) = pad(&t, "1");
    let (_, bottom_y, ..) = pad(&t, "7");
    assert!(left_x.abs() < bottom_y.abs(), "{left_x} {bottom_y}");
}

#[test]
fn a_qfn_template_plus_pins_validates() {
    let t = quad_flat_no_lead(&qfn32(), Density::Nominal).unwrap();
    let pins = (1..=33)
        .map(|n| Pin {
            number: n.to_string(),
            name: if n == 33 {
                "EP".into()
            } else {
                format!("IO{n}")
            },
            electrical_type: ElectricalType::Passive,
            group: None,
            alt_functions: vec![],
            source: None,
        })
        .collect();
    let spec = PartSpec {
        schema_version: SCHEMA_VERSION,
        origin: Origin::Hand,
        metadata: Metadata {
            mpn: "TEST-QFN".into(),
            manufacturer: "Test".into(),
            description: "QFN from a template".into(),
            keywords: vec![],
            datasheet_url: None,
            datasheet_sha256: None,
        },
        package: t.package,
        pins,
        land_pattern: t.land_pattern,
        approvals: Approvals::default(),
        overrides: vec![],
    };
    // At 0.4 mm pitch the corner pads of neighbouring sides are only 0.07 mm
    // apart, as in KiCad's own footprint: too close for the conservative generic
    // fab profile, fine for a maker with a finer process.
    let generic = validate(&spec, &FabProfile::generic());
    assert!(
        generic
            .iter()
            .all(|f| matches!(f.check.id(), "pad_clearance" | "exposed_pad")),
        "{generic:?}"
    );
    assert!(!generic.is_empty());
    let fine = FabProfile {
        min_copper_gap: kirpitch_core::Mm::new(0.05).unwrap(),
        ..FabProfile::generic()
    };
    // What is left is the warning about the paste of the large exposed pad.
    let findings = validate(&spec, &fine);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].check.id(), "exposed_pad");
}

#[test]
fn bad_qfn_input_is_rejected() {
    use kirpitch_core::templates::TemplateError;
    let mut q = qfn32();
    q.pins_x = 0;
    assert!(matches!(
        quad_flat_no_lead(&q, Density::Nominal),
        Err(TemplateError::PinCount(_))
    ));
    let mut q = qfn32();
    q.exposed_pad = Some((-1.0, 2.0));
    assert!(quad_flat_no_lead(&q, Density::Nominal).is_err());
}
