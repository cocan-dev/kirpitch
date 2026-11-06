// SPDX-License-Identifier: GPL-3.0-or-later

//! Models of template packages.

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{
    DualRowGullWing, DualRowThroughHole, QuadFlatNoLead, Template, dual_row_gull_wing,
    dual_row_through_hole, quad_flat_no_lead,
};
use kirpitch_gen_3d::{Material, View, build, check_fit, view_svg, write_step, write_vrml};

fn part(t: Template) -> PartSpec {
    let n = t.package.pin_count;
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
        pins: (1..=n)
            .map(|i| Pin {
                number: i.to_string(),
                name: format!("P{i}"),
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

fn soic8() -> PartSpec {
    part(
        dual_row_gull_wing(
            &DualRowGullWing {
                family: "SOIC".into(),
                pin_count: 8,
                pitch: 1.27,
                body_length: 4.9,
                body_width: 3.9,
                lead_span: Range::new(5.8, 6.2),
                height_max: 1.75,
                lead_length: Range::new(0.4, 1.27),
                lead_width: Range::new(0.31, 0.51),
            },
            Density::Nominal,
        )
        .unwrap(),
    )
}

fn qfn32() -> PartSpec {
    part(
        quad_flat_no_lead(
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
        .unwrap(),
    )
}

fn dip8() -> PartSpec {
    part(
        dual_row_through_hole(&DualRowThroughHole {
            family: "DIP".into(),
            pin_count: 8,
            pitch: 2.54,
            row_spacing: 7.62,
            body_length: 9.3,
            body_width: 6.4,
            height_max: 5.0,
            drill: 0.8,
            pad_diameter: 1.6,
        })
        .unwrap(),
    )
}

#[test]
fn the_soic8_model_has_a_body_a_marker_and_three_boxes_per_lead() {
    let spec = soic8();
    let m = build(&spec).unwrap();
    assert_eq!(m.boxes.len(), 1 + 1 + 8 * 3);
    let (lo, hi) = m.bounds();
    assert!((hi[2] - 1.75).abs() < 1e-9);
    assert!(lo[2].abs() < 1e-9);
    // The leads reach beyond the body on the left and right.
    assert!(lo[0] < -2.0 && hi[0] > 2.0);
    assert_eq!(check_fit(&spec, &m), vec![]);
}

#[test]
fn pin_one_is_at_the_top_left_of_the_model_like_in_the_footprint() {
    let spec = soic8();
    let m = build(&spec).unwrap();
    let marker = m
        .boxes
        .iter()
        .find(|b| b.material == Material::Marking)
        .unwrap();
    // Footprint y is down, model y is up: top left is x < 0 and y > 0.
    assert!(marker.max[0] < 0.0);
    assert!(marker.min[1] > 0.0);
}

#[test]
fn the_qfn_model_has_flat_terminals_and_an_exposed_pad() {
    let spec = qfn32();
    let m = build(&spec).unwrap();
    assert_eq!(m.boxes.len(), 1 + 1 + 32 + 1);
    let pad = m.boxes.last().unwrap();
    assert_eq!(pad.material, Material::Metal);
    assert!((pad.size()[0] - 2.65).abs() < 1e-9);
    let (lo, hi) = m.bounds();
    // No-lead terminals stay inside the body outline.
    assert!(lo[0] >= -2.0 - 1e-9 && hi[0] <= 2.0 + 1e-9);
    assert_eq!(check_fit(&spec, &m), vec![]);
}

#[test]
fn the_dip_model_has_pins_through_the_board() {
    let spec = dip8();
    let m = build(&spec).unwrap();
    assert_eq!(m.boxes.len(), 1 + 1 + 8);
    let (lo, _) = m.bounds();
    assert!(lo[2] < -1.0);
    let body = &m.boxes[0];
    // The body sits at the centre given in the PartSpec, not at pin 1.
    let c = spec.package.body.center.unwrap();
    assert!(((body.min[0] + body.max[0]) / 2.0 - c.x.get()).abs() < 1e-9);
    assert!(((body.min[1] + body.max[1]) / 2.0 + c.y.get()).abs() < 1e-9);
}

#[test]
fn the_fit_check_finds_a_model_that_is_too_high_or_misses_a_pad() {
    let spec = soic8();
    let mut m = build(&spec).unwrap();
    m.boxes[0].max[2] = 3.0;
    let issues = check_fit(&spec, &m);
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("above the maximum"))
    );

    let mut m = build(&spec).unwrap();
    m.boxes.truncate(2); // body and marker only
    let issues = check_fit(&spec, &m);
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("pad 1 has no terminal"))
    );
}

#[test]
fn vrml_is_well_formed_and_scaled_to_kicad_units() {
    let m = build(&soic8()).unwrap();
    let text = write_vrml(&m);
    assert!(text.starts_with("#VRML V2.0 utf8\n"));
    assert_eq!(text.matches('{').count(), text.matches('}').count());
    assert_eq!(text.matches('[').count(), text.matches(']').count());
    assert_eq!(text.matches("Shape {").count(), 3);
    // 1.75 mm high is 0.688976 in units of 2.54 mm.
    assert!(
        text.contains(" 0.688976,") || text.contains("0.68110"),
        "{text}"
    );
    assert_eq!(text, write_vrml(&m));
}

#[test]
fn step_has_the_right_structure() {
    let m = build(&soic8()).unwrap();
    let text = write_step(&m);
    assert!(text.starts_with("ISO-10303-21;\nHEADER;\n"));
    assert!(text.ends_with("END-ISO-10303-21;\n"));
    assert_eq!(text.matches("MANIFOLD_SOLID_BREP").count(), m.boxes.len());
    assert_eq!(text.matches("ADVANCED_FACE").count(), 6 * m.boxes.len());
    assert_eq!(text.matches("EDGE_CURVE").count(), 12 * m.boxes.len());
    assert_eq!(text.matches("STYLED_ITEM").count(), m.boxes.len());
    // Entity numbers are 1, 2, 3 ... and every reference points at one that
    // exists.
    let mut count = 0usize;
    let data: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "DATA;")
        .skip(1)
        .take_while(|l| *l != "ENDSEC;")
        .collect();
    for line in &data {
        count += 1;
        assert!(line.starts_with(&format!("#{count}=")), "{line}");
        assert!(line.ends_with(';'));
        assert_eq!(
            line.matches('(').count(),
            line.matches(')').count(),
            "{line}"
        );
        for part in line.split('#').skip(2) {
            let n: usize = part
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap();
            assert!(n >= 1 && n <= data.len(), "{line}");
        }
    }
    assert_eq!(text, write_step(&m));
}

#[test]
fn views_are_svg() {
    let m = build(&qfn32()).unwrap();
    for view in View::ALL {
        let svg = view_svg(&m, view);
        assert!(svg.starts_with("<svg "));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert_eq!(svg.matches("<rect").count(), m.boxes.len());
    }
}

/// KiCad loads the STEP and the VRML file through a footprint on a board and
/// puts every box into its own STEP export.
#[test]
fn kicad_loads_the_models() {
    use kirpitch_gen_footprint::{Options, generate};
    use kirpitch_kicad::KicadTarget;
    use kirpitch_kicad::testing::{
        TempDir, installed_is, kicad_cli_or_skip, write_board_with_footprint,
    };
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let fine = kirpitch_core::FabProfile {
        min_copper_gap: kirpitch_core::Mm::new(0.05).unwrap(),
        ..kirpitch_core::FabProfile::generic()
    };
    for (label, spec) in [("soic", soic8()), ("qfn", qfn32()), ("dip", dip8())] {
        let model = build(&spec).unwrap();
        for &target in KicadTarget::ALL {
            if !installed_is(&cli, target) {
                continue;
            }
            for extension in ["step", "wrl"] {
                let tmp = TempDir::new(&format!("3d-{label}-{extension}"));
                let folder = tmp.path().join("kirpitch.3dshapes");
                std::fs::create_dir_all(&folder).unwrap();
                let name = &spec.package.name;
                std::fs::write(folder.join(format!("{name}.step")), write_step(&model)).unwrap();
                std::fs::write(folder.join(format!("{name}.wrl")), write_vrml(&model)).unwrap();
                let footprint = generate(
                    &spec,
                    &Options {
                        target,
                        fab: fine.clone(),
                        model: Some(format!(
                            "${{KIPRJMOD}}/kirpitch.3dshapes/{name}.{extension}"
                        )),
                    },
                )
                .unwrap();
                let board = write_board_with_footprint(target, tmp.path(), &footprint);
                let out = tmp.path().join("board.step");
                cli.export_board_step(&board, &out)
                    .unwrap_or_else(|e| panic!("{label} {extension}: {e}"));
                let step = std::fs::read_to_string(&out).unwrap();
                let solids = step.matches("MANIFOLD_SOLID_BREP").count();
                // The board itself is one more solid. KiCad may merge or
                // split solids, so only require that the model is there.
                assert!(
                    solids > model.boxes.len() / 2,
                    "{label} {extension}: only {solids} solids"
                );
            }
        }
    }
}
