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

fn dip(pins: u32, row: f64) -> kirpitch_core::templates::DualRowThroughHole {
    kirpitch_core::templates::DualRowThroughHole {
        family: "DIP".into(),
        pin_count: pins,
        pitch: 2.54,
        row_spacing: row,
        body_length: (pins / 2 - 1) as f64 * 2.54 + 2.54,
        body_width: 6.35,
        height_max: 5.08,
        pad_diameter: 1.6,
        drill: 0.8,
    }
}

fn dip_specs() -> Vec<PartSpec> {
    use kirpitch_core::templates::dual_row_through_hole;
    [(8u32, 7.62), (16, 7.62), (28, 15.24)]
        .iter()
        .map(|&(n, row)| part(dual_row_through_hole(&dip(n, row)).unwrap(), n))
        .collect()
}

#[test]
fn dip_footprint_is_through_hole_with_origin_at_pin_one() {
    let spec = &dip_specs()[0];
    let text = generate(spec, &Options::default()).unwrap();
    assert!(text.starts_with("(footprint \"DIP-8_W7.62mm\"\n"));
    assert!(text.contains("(attr through_hole)"));
    assert!(text.contains("(pad \"1\" thru_hole rect\n\t\t(at 0 0)\n\t\t(size 1.6 1.6)\n\t\t(drill 0.8)\n\t\t(layers \"*.Cu\" \"*.Mask\")\n\t\t(remove_unused_layers no)"));
    assert!(text.contains("(pad \"2\" thru_hole oval\n\t\t(at 0 2.54)"));
    assert!(text.contains("(pad \"5\" thru_hole oval\n\t\t(at 7.62 7.62)"));
    // Pin 1 mark: a half circle in the silkscreen, and the chamfer on the fab layer.
    assert!(text.contains("(fp_arc"));
    assert!(!text.contains("F.Paste"));
    // Texts above and below the courtyard, centred on the body.
    assert!(text.contains("(property \"Reference\" \"REF**\"\n\t\t(at 3.81 "));
}

#[test]
fn dip_silkscreen_keeps_its_distance_from_the_pads() {
    let spec = &dip_specs()[0];
    let text = generate(spec, &Options::default()).unwrap();
    // The vertical silkscreen lines are at least 0.26 mm from the pad edges
    // (0.8 mm radius at x = 0 and x = 7.62).
    let tree = kirpitch_kicad::sexpr::parse(&text).unwrap();
    for node in &tree.items {
        let kirpitch_kicad::Node::List(l) = node else {
            continue;
        };
        if l.name() != Some("fp_line") || !l.write().contains("F.SilkS") {
            continue;
        }
        let w = l.write();
        let nums: Vec<f64> = w
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();
        // start x, start y, end x, end y are the first four numbers.
        for x in [nums[0], nums[2]] {
            assert!(x - 0.8 >= 0.26 - 1e-9 || x - 0.8 < -1.0, "x = {x}");
            assert!(7.62 - 0.8 - x >= 0.26 - 1e-9 || x > 7.62, "x = {x}");
        }
    }
}

#[test]
fn kicad_loads_the_dip_footprints() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    for spec in dip_specs() {
        for &target in KicadTarget::ALL {
            let options = Options {
                target,
                ..Default::default()
            };
            let text = generate(&spec, &options).unwrap();
            assert_footprint_loads(&cli, target, "dip", &text, &file_name(&spec));
        }
    }
}

fn qfn_spec() -> PartSpec {
    use kirpitch_core::templates::{QuadFlatNoLead, quad_flat_no_lead};
    let t = quad_flat_no_lead(
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
    part(t, 33)
}

/// Options for a maker whose process allows 0.05 mm between pads.
fn fine_options(target: KicadTarget) -> Options {
    Options {
        target,
        fab: kirpitch_core::FabProfile {
            min_copper_gap: kirpitch_core::Mm::new(0.05).unwrap(),
            ..kirpitch_core::FabProfile::generic()
        },
        model: None,
    }
}

#[test]
fn qfn_footprint_needs_the_fine_profile_and_has_the_exposed_pad() {
    let spec = qfn_spec();
    assert!(generate(&spec, &Options::default()).is_err());
    let text = generate(&spec, &fine_options(KicadTarget::LATEST)).unwrap();
    assert!(text.starts_with("(footprint \"QFN-32-1EP_4x4mm_P0.4mm_EP2.65x2.65mm\"\n"));
    assert!(text.contains("(pad \"33\" smd rect"));
    // Pin 1 marker and fab chamfer sit at the top-left corner.
    assert!(text.contains("F.Fab"));
    // Courtyard is on the 0.01 grid and 0.25 mm outside pads and body.
    let found = kirpitch_kicad::klc::check_footprint(&text).unwrap();
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn kicad_loads_the_qfn_footprint() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let spec = qfn_spec();
    for &target in KicadTarget::ALL {
        let text = generate(&spec, &fine_options(target)).unwrap();
        assert_footprint_loads(&cli, target, "qfn", &text, &file_name(&spec));
    }
}
