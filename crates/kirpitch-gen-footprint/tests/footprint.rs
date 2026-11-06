// SPDX-License-Identifier: GPL-3.0-or-later

//! Footprint generation for the SOIC-8 example PartSpec.

use std::fs;
use std::path::Path;
use std::time::Instant;

use kirpitch_core::PartSpec;
use kirpitch_gen_footprint::{GenerateError, Options, file_name, generate};
use kirpitch_kicad::KicadTarget;
use kirpitch_kicad::sexpr::{check_round_trip, parse};
use kirpitch_kicad::testing::{TempDir, assert_snapshot, kicad_cli_or_skip};
use kirpitch_kicad::{Node, UpgradeOutcome};

const SPEC: &str = include_str!("../../kirpitch-core/tests/fixtures/soic8_example.json");

fn spec() -> PartSpec {
    PartSpec::from_json_str(SPEC).expect("fixture loads")
}

fn output() -> String {
    generate(&spec(), &Options::default()).expect("generates")
}

#[test]
fn matches_the_snapshot() {
    assert_snapshot(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/soic8_example.kicad_mod"),
        &output(),
    );
}

#[test]
fn is_deterministic_and_uses_unix_line_endings() {
    let a = output();
    assert_eq!(a, output());
    assert!(!a.contains('\r'));
    assert!(a.ends_with(")\n"));
}

#[test]
fn is_fast() {
    let start = Instant::now();
    let _ = output();
    assert!(start.elapsed().as_secs_f64() < 1.0);
}

#[test]
fn parses_back_to_the_same_text_with_the_target_version() {
    let text = output();
    let tree = check_round_trip(&text).expect("canonical layout");
    assert_eq!(tree.name(), Some("footprint"));
    let version = find(&tree, "version").expect("version");
    assert_eq!(
        version,
        &KicadTarget::V10.footprint_format_version().to_string()
    );
}

fn find<'a>(list: &'a kirpitch_kicad::List, name: &str) -> Option<&'a String> {
    list.items.iter().find_map(|n| match n {
        Node::List(l) if l.name() == Some(name) => match l.items.get(1) {
            Some(Node::Atom(a)) | Some(Node::Str(a)) => Some(a),
            _ => None,
        },
        _ => None,
    })
}

#[test]
fn name_and_file_name_follow_kicad_naming() {
    assert_eq!(file_name(&spec()), "SOIC-8_3.9x4.9mm_P1.27mm.kicad_mod");
    assert!(output().starts_with("(footprint \"SOIC-8_3.9x4.9mm_P1.27mm\"\n"));
}

#[test]
fn geometry_matches_the_official_kicad_soic8_footprint() {
    // Values below were compared with KiCad's own SOIC-8_3.9x4.9mm_P1.27mm.
    let text = output();
    // Pads.
    assert!(text.contains("(pad \"1\" smd roundrect\n\t\t(at -2.475 -1.905)\n\t\t(size 1.95 0.6)"));
    assert!(text.contains("(pad \"5\" smd roundrect\n\t\t(at 2.475 1.905)"));
    assert!(text.contains("(roundrect_rratio 0.25)"));
    assert!(text.contains("(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")"));
    // Courtyard.
    assert!(text.contains("(start -3.7 -2.7)"));
    assert!(text.contains("(end 3.7 2.7)"));
    // Fab outline with the pin 1 chamfer.
    for pt in [
        "(xy -0.975 -2.45)",
        "(xy 1.95 -2.45)",
        "(xy 1.95 2.45)",
        "(xy -1.95 2.45)",
        "(xy -1.95 -1.475)",
    ] {
        assert!(text.contains(pt), "missing {pt}");
    }
    // Silkscreen: top and bottom edges plus the short corner stubs.
    assert!(text.contains("(start -2.06 -2.56)\n\t\t(end 2.06 -2.56)"));
    assert!(text.contains("(start -2.06 2.56)\n\t\t(end 2.06 2.56)"));
    assert!(text.contains("(start -2.06 -2.56)\n\t\t(end -2.06 -2.465)"));
    // Texts above and below the courtyard.
    assert!(text.contains("(property \"Reference\" \"REF**\"\n\t\t(at 0 -3.4 0)"));
    assert!(text.contains("(at 0 3.4 0)"));
}

#[test]
fn silkscreen_keeps_its_distance_from_pads() {
    let tree = parse(&output()).unwrap();
    let pads: Vec<(f64, f64, f64, f64)> = spec()
        .land_pattern
        .pads
        .iter()
        .map(|p| p.bounds())
        .collect();
    for node in &tree.items {
        let Node::List(l) = node else { continue };
        if l.name() != Some("fp_line") || !text_of(l).contains("F.SilkS") {
            continue;
        }
        let (a, b) = endpoints(l);
        for (x0, y0, x1, y1) in &pads {
            // Distance between the segment (axis aligned) and the pad box.
            let dx = (a.0.min(b.0) - x1).max(x0 - a.0.max(b.0)).max(0.0);
            let dy = (a.1.min(b.1) - y1).max(y0 - a.1.max(b.1)).max(0.0);
            let gap = dx.hypot(dy) - 0.06; // minus half the line width
            assert!(
                gap >= 0.2 - 1e-6,
                "silk {a:?}-{b:?} too close to a pad: {gap}"
            );
        }
    }
}

fn text_of(l: &kirpitch_kicad::List) -> String {
    l.write()
}

fn endpoints(l: &kirpitch_kicad::List) -> ((f64, f64), (f64, f64)) {
    let point = |name: &str| {
        let Some(Node::List(p)) = l
            .items
            .iter()
            .find(|n| matches!(n, Node::List(c) if c.name() == Some(name)))
        else {
            panic!("{name} missing");
        };
        let num = |i: usize| match &p.items[i] {
            Node::Atom(a) => a.parse::<f64>().unwrap(),
            _ => panic!("not a number"),
        };
        (num(1), num(2))
    };
    (point("start"), point("end"))
}

#[test]
fn invalid_partspec_is_reported_by_field() {
    let mut s = spec();
    s.package.pin_count = 9;
    let err = generate(&s, &Options::default()).unwrap_err();
    match err {
        GenerateError::Invalid(findings) => {
            assert!(findings.iter().any(|f| f.path == "pins"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn pad_rotation_and_other_shapes_are_written() {
    let mut s = spec();
    s.land_pattern.pads[0].rotation_deg = 90.0;
    s.land_pattern.pads[0].shape = kirpitch_core::partspec::PadShape::Oval;
    s.land_pattern.pads[0].size_x = kirpitch_core::Mm::new(0.6).unwrap();
    s.land_pattern.pads[0].size_y = kirpitch_core::Mm::new(1.95).unwrap();
    s.land_pattern.pads[1].paste = false;
    let text = generate(&s, &Options::default()).unwrap();
    assert!(text.contains("(pad \"1\" smd oval\n\t\t(at -2.475 -1.905 90)"));
    assert!(text.contains("(layers \"F.Cu\" \"F.Mask\")"));
    check_round_trip(&text).expect("canonical layout");
}

#[test]
fn explicit_courtyard_is_used() {
    let mut s = spec();
    let m = |v| kirpitch_core::Mm::new(v).unwrap();
    s.land_pattern.courtyard = Some(kirpitch_core::partspec::Rect {
        x_min: m(-4.0),
        y_min: m(-3.0),
        x_max: m(4.0),
        y_max: m(3.0),
    });
    let text = generate(&s, &Options::default()).unwrap();
    assert!(text.contains("(start -4 -3)"));
    assert!(text.contains("(at 0 -3.7 0)"));
}

#[test]
fn kicad_loads_the_footprint() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("gen-fp");
    let lib = tmp.path().join("lib.pretty");
    fs::create_dir_all(&lib).unwrap();
    let text = output();
    fs::write(lib.join(file_name(&spec())), &text).unwrap();
    let svgs = cli
        .export_footprint_svg(&lib, &tmp.path().join("svg"))
        .expect("kicad-cli exports the footprint");
    assert_eq!(svgs.len(), 1);
    check_round_trip(&text).expect("own parse and round trip");
    if cli.version().unwrap().major >= 10 {
        assert_eq!(
            cli.upgrade_footprints(&lib).unwrap(),
            UpgradeOutcome::Unchanged
        );
        let after = fs::read_to_string(lib.join(file_name(&spec()))).unwrap();
        assert_eq!(after.replace("\r\n", "\n"), text);
    }
}
