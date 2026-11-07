// SPDX-License-Identifier: GPL-3.0-or-later

//! Models of template packages.

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{
    DualRowGullWing, DualRowThroughHole, GridArray, QuadFlatGullWing, QuadFlatNoLead, Template,
    dual_row_gull_wing, dual_row_through_hole, grid_array, quad_flat_gull_wing, quad_flat_no_lead,
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

fn lqfp64() -> PartSpec {
    part(
        quad_flat_gull_wing(
            &QuadFlatGullWing {
                family: "LQFP".into(),
                pins_x: 16,
                pins_y: 16,
                pitch: 0.5,
                body_x: Range::new(9.9, 10.1),
                body_y: Range::new(9.9, 10.1),
                span_x: Range::new(11.8, 12.2),
                span_y: Range::new(11.8, 12.2),
                height_max: 1.6,
                lead_length: Range::new(0.45, 0.75),
                lead_width: Range::new(0.17, 0.27),
                exposed_pad: None,
            },
            Density::Nominal,
        )
        .unwrap(),
    )
}

fn bga64() -> PartSpec {
    part(
        grid_array(
            &GridArray {
                family: "BGA".into(),
                rows: 8,
                cols: 8,
                pitch: 1.0,
                body_x: Range::new(8.9, 9.1),
                body_y: Range::new(8.9, 9.1),
                height_max: 1.4,
                ball_diameter: Range::new(0.45, 0.55),
                standoff: Some(0.3),
            },
            Density::Nominal,
        )
        .unwrap(),
    )
}

#[test]
fn a_quad_gull_wing_package_has_legs_on_all_four_sides_and_passes_the_fit_check() {
    let spec = lqfp64();
    let m = build(&spec).unwrap();
    assert!(
        check_fit(&spec, &m).is_empty(),
        "{:?}",
        check_fit(&spec, &m)
    );
    let (lo, hi) = m.bounds();
    // The legs reach out to the lead span (12 mm) on x and y.
    assert!(
        hi[0] > 5.5 && lo[0] < -5.5 && hi[1] > 5.5 && lo[1] < -5.5,
        "{lo:?} {hi:?}"
    );
    let metal = m
        .boxes
        .iter()
        .filter(|b| b.material == Material::Metal)
        .count();
    assert_eq!(metal, 64 * 3, "three boxes for each gull-wing leg");
}

#[test]
fn the_dimensions_of_the_drawing_shape_the_model() {
    // The lead thickness c and the body thickness A2 of the drawing are used.
    let mut spec = lqfp64();
    let plain = build(&spec).unwrap();
    for (k, v) in [("c", 0.12), ("A2", 1.4), ("A1", 0.1)] {
        let m = kirpitch_core::units::Mm::new(v).unwrap();
        spec.package.dimensions.insert(
            k.into(),
            kirpitch_core::source::Sourced::unsourced(kirpitch_core::units::Tolerance {
                min: None,
                nom: m,
                max: None,
            }),
        );
    }
    let shaped = build(&spec).unwrap();
    // A thinner lead: the foot box is 0.12 mm thick instead of 0.2 mm.
    let foot = |m: &kirpitch_gen_3d::Model| {
        m.boxes
            .iter()
            .filter(|b| b.material == Material::Metal)
            .map(|b| b.max[2] - b.min[2])
            .fold(f64::INFINITY, f64::min)
    };
    assert!((foot(&plain) - 0.2).abs() < 1e-9 && (foot(&shaped) - 0.12).abs() < 1e-9);
    // The body is A2 = 1.4 mm thick above the 0.1 mm standoff: 1.5 mm in all.
    assert!(
        (shaped.bounds().1[2] - 1.5).abs() < 1e-9,
        "{:?}",
        shaped.bounds()
    );
    assert!(check_fit(&spec, &shaped).is_empty());
}

#[test]
fn a_ball_grid_has_a_ball_under_every_pad_and_none_sticking_out() {
    let spec = bga64();
    let m = build(&spec).unwrap();
    assert!(
        check_fit(&spec, &m).is_empty(),
        "{:?}",
        check_fit(&spec, &m)
    );
    let balls: Vec<_> = m
        .boxes
        .iter()
        .filter(|b| b.material == Material::Metal)
        .collect();
    assert_eq!(balls.len(), 64);
    let (lo, hi) = m.bounds();
    // Nothing reaches past the 9 mm body, and the balls are 0.3 mm high.
    assert!(hi[0] <= 4.5 + 1e-9 && lo[0] >= -4.5 - 1e-9);
    assert!(balls.iter().all(|b| (b.max[2] - 0.3).abs() < 1e-9));
    assert!(
        balls
            .iter()
            .all(|b| (b.max[0] - b.min[0] - 0.5).abs() < 1e-9)
    );
    assert!(hi[2] <= 1.4 + 1e-9);
}

#[test]
fn the_engineering_drawing_has_three_views_and_the_overall_size() {
    use kirpitch_gen_3d::drawing_svg;
    let m = build(&qfn32()).unwrap();
    let svg = drawing_svg(&m);
    assert!(svg.starts_with("<svg ") && svg.trim_end().ends_with("</svg>"));
    // The sheet, and every box once in each of the three views.
    assert_eq!(svg.matches("<rect").count(), 1 + 3 * m.boxes.len());
    for label in [
        "TOP",
        "FRONT",
        "LEFT",
        "THIRD-ANGLE PROJECTION",
        "all sizes in mm",
    ] {
        assert!(svg.contains(label), "{label}");
    }
    // The overall size is written next to its three dimension lines.
    let (lo, hi) = m.bounds();
    for v in [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] {
        let text = kirpitch_core::format_number((v * 1000.0).round() / 1000.0);
        assert!(svg.contains(&format!(">{text}</text>")), "{text}");
    }
    // The same model always gives the same drawing.
    assert_eq!(svg, drawing_svg(&build(&qfn32()).unwrap()));
}
