// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated symbols of several shapes follow the KiCad library conventions.

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{
    DualRowGullWing, DualRowThroughHole, QuadFlatNoLead, Template, dual_row_gull_wing,
    dual_row_through_hole, quad_flat_no_lead,
};
use kirpitch_core::{FabProfile, Mm};
use kirpitch_gen_symbol::{Options, generate};
use kirpitch_kicad::KicadTarget;
use kirpitch_kicad::klc_symbol::check_symbol;

/// A part whose pins cycle through several types, with two ground pins of the
/// same name when there are enough pins.
fn part(t: Template) -> PartSpec {
    let n = t.package.pin_count;
    let types = [
        ElectricalType::Input,
        ElectricalType::Output,
        ElectricalType::Bidirectional,
        ElectricalType::Passive,
        ElectricalType::OpenCollector,
    ];
    let pins = (1..=n)
        .map(|i| {
            let (name, electrical_type) = match i {
                1 => ("VCC".to_string(), ElectricalType::PowerIn),
                2 => ("GND".to_string(), ElectricalType::PowerIn),
                3 if n > 8 => ("GND".to_string(), ElectricalType::PowerIn),
                _ => (format!("IO{i}"), types[(i as usize) % types.len()]),
            };
            Pin {
                number: i.to_string(),
                name,
                electrical_type,
                group: None,
                alt_functions: vec![],
                source: None,
            }
        })
        .collect();
    PartSpec {
        schema_version: SCHEMA_VERSION,
        origin: Origin::Hand,
        metadata: Metadata {
            mpn: "TEST-PART".into(),
            manufacturer: "Test".into(),
            description: "Part made from a template".into(),
            keywords: vec![],
            datasheet_url: None,
            datasheet_sha256: None,
        },
        package: t.package,
        pins,
        land_pattern: t.land_pattern,
        approvals: Approvals::default(),
        overrides: vec![],
    }
}

fn parts() -> Vec<(&'static str, PartSpec)> {
    let soic = |pins: u32| {
        dual_row_gull_wing(
            &DualRowGullWing {
                family: "SOIC".into(),
                pin_count: pins,
                pitch: 1.27,
                body_length: if pins == 8 { 4.9 } else { 9.9 },
                body_width: 3.9,
                height_max: 1.75,
                lead_span: Range::new(5.8, 6.2),
                lead_length: Range::new(0.4, 1.27),
                lead_width: Range::new(0.31, 0.51),
            },
            Density::Nominal,
        )
        .unwrap()
    };
    let dip = dual_row_through_hole(&DualRowThroughHole {
        family: "DIP".into(),
        pin_count: 16,
        pitch: 2.54,
        row_spacing: 7.62,
        body_length: 19.0,
        body_width: 6.4,
        height_max: 5.0,
        pad_diameter: 1.6,
        drill: 0.8,
    })
    .unwrap();
    let qfn = quad_flat_no_lead(
        &QuadFlatNoLead {
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
        },
        Density::Nominal,
    )
    .unwrap();
    vec![
        ("soic8", part(soic(8))),
        ("soic16", part(soic(16))),
        ("dip16", part(dip)),
        ("qfn33", part(qfn)),
    ]
}

#[test]
fn generated_symbols_follow_the_conventions_in_every_kicad_version() {
    let options = |target| Options {
        target,
        fab: FabProfile {
            min_copper_gap: Mm::new(0.05).unwrap(),
            min_mask_web: Mm::new(0.05).unwrap(),
            ..FabProfile::generic()
        },
    };
    for (label, spec) in parts() {
        for &target in KicadTarget::ALL {
            let text = generate(&spec, &options(target)).unwrap();
            let found = check_symbol(&text).unwrap();
            assert!(
                found.is_empty(),
                "{label} for KiCad {}: {found:?}",
                target.major()
            );
        }
    }
}
