// SPDX-License-Identifier: GPL-3.0-or-later

//! Footprints for parts made from package templates (no datasheet needed).

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{DualRowGullWing, Template, dual_row_gull_wing, sot23};
use kirpitch_gen_footprint::{Options, file_name, generate};
use kirpitch_kicad::KicadTarget;
use kirpitch_kicad::testing::{assert_footprint_loads, kicad_cli_or_skip};

fn part(t: Template, pins: u32) -> PartSpec {
    PartSpec {
        schema_version: SCHEMA_VERSION,
        origin: Origin::Hand,
        metadata: Metadata {
            mpn: "TEST".into(),
            manufacturer: "Test".into(),
            description: "Part made from a template".into(),
            keywords: vec![],
            datasheet_url: None,
            datasheet_sha256: None,
        },
        package: t.package,
        pins: (1..=pins)
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

fn sot(pins: u32) -> DualRowGullWing {
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

fn tssop(pins: u32, length: f64) -> DualRowGullWing {
    DualRowGullWing {
        family: "TSSOP".into(),
        pin_count: pins,
        pitch: 0.65,
        body_length: length,
        body_width: 4.4,
        height_max: 1.2,
        lead_span: Range::exact(6.4),
        lead_length: Range::new(0.45, 0.75),
        lead_width: Range::new(0.17, 0.30),
    }
}

fn specs() -> Vec<PartSpec> {
    let mut out = Vec::new();
    for pins in [3u32, 5, 6] {
        out.push(part(sot23(&sot(pins), Density::Nominal).unwrap(), pins));
    }
    out.push(part(
        dual_row_gull_wing(&tssop(14, 5.0), Density::Nominal).unwrap(),
        14,
    ));
    out.push(part(
        dual_row_gull_wing(&tssop(20, 6.5), Density::Nominal).unwrap(),
        20,
    ));
    out
}

#[test]
fn template_footprints_generate_for_every_target() {
    for spec in specs() {
        for &target in KicadTarget::ALL {
            let options = Options {
                target,
                ..Default::default()
            };
            let text =
                generate(&spec, &options).unwrap_or_else(|e| panic!("{}: {e}", spec.package.name));
            assert!(text.contains(&format!("(footprint \"{}\"", spec.package.name)));
        }
    }
}

#[test]
fn small_leads_keep_the_silkscreen_clear_of_the_pads() {
    // SOT-23-5: the pads are close to the body, so only the top and bottom edge
    // of the silkscreen are drawn between them.
    let spec = &specs()[1];
    let text = generate(spec, &Options::default()).unwrap();
    assert!(text.contains("F.SilkS"));
    assert!(text.contains("F.CrtYd"));
}

#[test]
fn kicad_loads_the_template_footprints() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    for spec in specs() {
        for &target in KicadTarget::ALL {
            let options = Options {
                target,
                ..Default::default()
            };
            let text = generate(&spec, &options).unwrap();
            assert_footprint_loads(&cli, target, "family", &text, &file_name(&spec));
        }
    }
}
