// SPDX-License-Identifier: GPL-3.0-or-later

//! The logic behind the window, tested without a window.

use std::fs;
use std::path::{Path, PathBuf};

use kirpitch_app::{approve_stage, export_files, footprint_svg, load_spec, now_rfc3339, open_spec};
use kirpitch_kicad::testing::TempDir;

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden")
        .join(name)
        .join("partspec.json")
}

/// A copy of a golden PartSpec that a test may change.
fn copy_of(name: &str, tmp: &TempDir) -> String {
    let target = tmp.path().join("partspec.json");
    fs::copy(golden(name), &target).unwrap();
    target.to_string_lossy().into_owned()
}

#[test]
fn a_golden_part_is_shown_with_pins_pads_and_gates() {
    let view = open_spec(&golden("lm358-soic8").to_string_lossy()).unwrap();
    assert_eq!(view.mpn, "LM358DR");
    assert_eq!(view.package_name, "SOIC-8_3.9x4.9mm_P1.27mm");
    assert_eq!(view.pins.len(), 8);
    assert_eq!(view.pins[0].name, "OUT1");
    assert_eq!(view.pins[0].source, "p.3");
    assert_eq!(view.pads.len(), 8);
    assert_eq!(view.pads[0].x, "-2.7");
    let names: Vec<&str> = view.gates.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["pinout", "footprint"]);
    assert!(
        view.gates
            .iter()
            .all(|g| g.status == "open" && g.errors == 0)
    );
    assert!(view.findings.is_empty());
}

#[test]
fn the_footprint_drawing_has_every_pad_and_a_viewbox() {
    let spec = load_spec(&golden("ne555-soic8")).unwrap();
    let svg = footprint_svg(&spec);
    assert!(svg.starts_with("<svg "));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("viewBox=\""));
    assert_eq!(svg.matches("class=\"pad").count(), 8);
    assert_eq!(svg.matches("class=\"pad pin1\"").count(), 1);
    assert_eq!(svg.matches("class=\"label\"").count(), 8);
    assert!(svg.contains("class=\"body\""));
    // No scripts or event handlers can come out of a PartSpec.
    assert!(!svg.contains("<script") && !svg.contains("onload"));
}

#[test]
fn text_from_the_partspec_is_escaped_in_the_drawing() {
    let mut spec = load_spec(&golden("ne555-soic8")).unwrap();
    spec.package.name = "A<script>&\"".into();
    spec.land_pattern.pads[0].number = "<b>".into();
    let svg = footprint_svg(&spec);
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<b>"));
    assert!(svg.contains("&lt;b&gt;"));
}

#[test]
fn through_hole_pads_show_their_holes() {
    use kirpitch_core::ipc::Density;
    let _ = Density::Nominal;
    let spec = load_spec(&golden("ne555-soic8")).unwrap();
    assert!(!footprint_svg(&spec).contains("class=\"hole\""));
}

#[test]
fn stages_are_approved_in_order_and_saved() {
    let tmp = TempDir::new("app-approve");
    let path = copy_of("lm358-soic8", &tmp);

    let err = approve_stage(&path, "footprint", None, now_rfc3339()).unwrap_err();
    assert_eq!(err, "approve the pinout stage first");

    let view = approve_stage(
        &path,
        "pinout",
        Some("checked".into()),
        "2026-11-06T12:00:00Z".into(),
    )
    .unwrap();
    assert_eq!(view.gates[0].status, "approved");
    assert_eq!(view.gates[0].note.as_deref(), Some("checked"));
    assert_eq!(
        view.gates[0].approved_at.as_deref(),
        Some("2026-11-06T12:00:00Z")
    );
    assert_eq!(view.gates[1].status, "open");

    let view = approve_stage(&path, "footprint", Some("  ".into()), now_rfc3339()).unwrap();
    assert!(view.gates.iter().all(|g| g.status == "approved"));
    assert_eq!(view.gates[1].note, None, "a blank note is not saved");

    // The approvals are in the file, so a fresh open sees them.
    let again = open_spec(&path).unwrap();
    assert!(again.gates.iter().all(|g| g.status == "approved"));
}

#[test]
fn a_changed_stage_is_shown_as_stale() {
    let tmp = TempDir::new("app-stale");
    let path = copy_of("lm358-soic8", &tmp);
    approve_stage(&path, "pinout", None, now_rfc3339()).unwrap();
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("\"OUT1\"", "\"OUTPUT1\"");
    fs::write(&path, text).unwrap();
    let view = open_spec(&path).unwrap();
    assert_eq!(view.gates[0].status, "stale");
}

#[test]
fn errors_block_the_approval_with_the_field_to_fix() {
    let tmp = TempDir::new("app-errors");
    let path = copy_of("lm358-soic8", &tmp);
    let text =
        fs::read_to_string(&path)
            .unwrap()
            .replacen("\"number\": \"2\"", "\"number\": \"1\"", 1);
    fs::write(&path, text).unwrap();
    let view = open_spec(&path).unwrap();
    assert!(view.gates[0].errors >= 1);
    assert!(
        view.findings
            .iter()
            .any(|f| f.check == "pin_number" && f.gate == "pinout")
    );
    let err = approve_stage(&path, "pinout", None, now_rfc3339()).unwrap_err();
    assert!(err.contains("pins[1].number"), "{err}");
    assert_eq!(open_spec(&path).unwrap().gates[0].status, "open");
}

#[test]
fn bad_files_and_stages_give_readable_errors() {
    assert!(
        open_spec("definitely/not/here.json")
            .unwrap_err()
            .contains("not/here.json")
    );
    let tmp = TempDir::new("app-bad");
    let path = tmp.path().join("bad.json");
    fs::write(&path, "{ \"schema_version\": 1 }").unwrap();
    assert!(
        open_spec(&path.to_string_lossy())
            .unwrap_err()
            .contains("invalid PartSpec")
    );
    let ok = copy_of("lm358-soic8", &tmp);
    assert_eq!(
        approve_stage(&ok, "3d", None, now_rfc3339()).unwrap_err(),
        "there is no stage called `3d`"
    );
}

#[test]
fn files_are_written_for_the_chosen_kicad_version() {
    let tmp = TempDir::new("app-export");
    let path = copy_of("ne555-soic8", &tmp);
    let out = tmp.path().join("out");
    let written = export_files(&path, &out.to_string_lossy(), Some(9)).unwrap();
    assert_eq!(written.len(), 2);
    let symbol = fs::read_to_string(out.join("NE555DR.kicad_sym")).unwrap();
    assert!(symbol.contains("(version 20241209)"));
    assert!(
        out.join("kirpitch.pretty/SOIC-8_3.9x4.9mm_P1.27mm.kicad_mod")
            .is_file()
    );
    assert!(
        export_files(&path, &out.to_string_lossy(), Some(7))
            .unwrap_err()
            .contains("KiCad 7")
    );
}

#[test]
fn the_time_looks_like_a_utc_timestamp() {
    let t = now_rfc3339();
    assert_eq!(t.len(), 20, "{t}");
    assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
}

#[test]
fn base64_matches_the_standard_vectors() {
    use kirpitch_app::base64;
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foob"), "Zm9vYg==");
    assert_eq!(base64(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn rows_know_their_datasheet_page() {
    let view = open_spec(&golden("lm358-soic8").to_string_lossy()).unwrap();
    assert_eq!(view.pins[0].page, Some(3));
    assert_eq!(view.pads[0].page, Some(64));
    assert_eq!(view.pins[0].region, None);
}

#[test]
fn a_datasheet_page_is_rendered_when_pdfium_is_available() {
    if std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR").is_none() {
        eprintln!("SKIPPED: set KIRPITCH_PDFIUM_LIB_DIR to run this test");
        return;
    }
    let tmp = TempDir::new("app-page");
    let pdf = tmp.path().join("d.pdf");
    let pages = vec![
        vec![(72.0, 700.0, 14.0, "Page one")],
        vec![(72.0, 700.0, 14.0, "Page two")],
    ];
    fs::write(&pdf, kirpitch_extract::testing::make_pdf(&pages)).unwrap();
    let path = pdf.to_string_lossy().into_owned();
    let image = kirpitch_app::render_datasheet_page(&path, 2, 400).unwrap();
    assert_eq!((image.width, image.height, image.page_count), (400, 518, 2));
    assert_eq!(
        image.rgba_base64.len(),
        (image.width as usize * image.height as usize * 4).div_ceil(3) * 4
    );
    // A page after the last one is clamped, a missing file is an error.
    assert_eq!(
        kirpitch_app::render_datasheet_page(&path, 9, 400)
            .unwrap()
            .page_count,
        2
    );
    assert!(kirpitch_app::render_datasheet_page("nope.pdf", 1, 400).is_err());
}

#[test]
fn the_symbol_drawing_has_every_visible_pin_and_selection_marks() {
    let view = open_spec(&golden("lm358-soic8").to_string_lossy()).unwrap();
    let svg = &view.symbol_svg;
    assert!(svg.starts_with("<svg "), "{svg}");
    assert_eq!(svg.matches("class=\"spin\"").count(), 8);
    assert!(svg.contains("class=\"sbody\""));
    for n in 1..=8 {
        assert!(svg.contains(&format!("data-pin=\"{n}\"")), "pin {n}");
    }
    // Names are drawn, with their text escaped.
    assert!(svg.contains(">OUT1</text>"));
    // The footprint pads use the same pin numbers.
    for n in 1..=8 {
        assert!(
            view.footprint_svg.contains(&format!("data-pin=\"{n}\"")),
            "pad {n}"
        );
    }
}

#[test]
fn stacked_power_pins_are_drawn_once() {
    let mut spec = load_spec(&golden("lm358-soic8")).unwrap();
    let mut second = spec.pins[3].clone();
    second.number = "9".into();
    spec.pins.push(second);
    let mut pad = spec.land_pattern.pads[3].clone();
    pad.number = "9".into();
    pad.x = kirpitch_core::Mm::new(0.0).unwrap();
    pad.y = kirpitch_core::Mm::new(0.0).unwrap();
    spec.land_pattern.pads.push(pad);
    spec.package.pin_count = 9;
    let svg = kirpitch_app::symbol_svg(&spec).unwrap();
    assert_eq!(svg.matches("class=\"spin\"").count(), 8);
    assert!(!svg.contains("data-pin=\"9\""));
}

#[test]
fn an_invalid_partspec_has_no_symbol_drawing() {
    let tmp = TempDir::new("app-nosymbol");
    let path = copy_of("lm358-soic8", &tmp);
    let text =
        fs::read_to_string(&path)
            .unwrap()
            .replacen("\"number\": \"2\"", "\"number\": \"1\"", 1);
    fs::write(&path, text).unwrap();
    let view = open_spec(&path).unwrap();
    assert_eq!(view.symbol_svg, "");
    assert!(!view.footprint_svg.is_empty());
}

/// A datasheet page with an 8-pin table, built for the test.
fn pin_page() -> Vec<(f32, f32, f32, &'static str)> {
    let rows = [
        ("1", "OUT1", "Output"),
        ("2", "IN1-", "Input"),
        ("3", "IN1+", "Input"),
        ("4", "V-", "Power"),
        ("5", "IN2+", "Input"),
        ("6", "IN2-", "Input"),
        ("7", "OUT2", "Output"),
        ("8", "V+", "Power"),
    ];
    let mut items = vec![
        (72.0, 740.0, 14.0, "Pin Functions"),
        (72.0, 710.0, 12.0, "PIN"),
        (150.0, 710.0, 12.0, "NAME"),
        (260.0, 710.0, 12.0, "TYPE"),
    ];
    for (i, (n, name, ty)) in rows.iter().enumerate() {
        let y = 690.0 - 20.0 * i as f32;
        items.push((72.0, y, 12.0, n));
        items.push((150.0, y, 12.0, name));
        items.push((260.0, y, 12.0, ty));
    }
    items
}

#[test]
fn a_region_of_the_page_becomes_the_pin_table() {
    if std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR").is_none() {
        eprintln!("SKIPPED: set KIRPITCH_PDFIUM_LIB_DIR to run this test");
        return;
    }
    let tmp = TempDir::new("app-region");
    let pdf = tmp.path().join("d.pdf");
    fs::write(&pdf, kirpitch_extract::testing::make_pdf(&[pin_page()])).unwrap();
    let pdf = pdf.to_string_lossy().into_owned();
    // Under the heading: from y = 725 down to 515 pt of 792.
    let region = [0.0, 1.0 - 725.0 / 792.0, 1.0, 1.0 - 515.0 / 792.0];

    let table = kirpitch_app::read_region(&pdf, 1, region).unwrap();
    assert_eq!(table.rows.len(), 9);
    assert_eq!(table.rows[0], ["PIN", "NAME", "TYPE"]);
    assert_eq!(table.rows[8], ["8", "V+", "Power"]);

    // The user may fix a cell in the text before using it.
    let edited = table.tsv.replace("V+\tPower", "V+\tPower supply");
    let tmp2 = TempDir::new("app-region-spec");
    let spec = copy_of("lm358-soic8", &tmp2);
    let out = kirpitch_app::import_pins(&spec, &edited, 1, Some(region)).unwrap();
    assert_eq!(out.view.pins.len(), 8);
    assert_eq!(out.view.pins[7].name, "V+");
    assert_eq!(out.view.pins[7].electrical_type, "power_in");
    assert_eq!(out.view.pins[0].page, Some(1));
    assert_eq!(out.view.pins[0].region, Some(region));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    // The file was saved.
    assert_eq!(open_spec(&spec).unwrap().pins[0].name, "OUT1");
}

#[test]
fn importing_pins_makes_an_approved_pinout_stale() {
    let tmp = TempDir::new("app-import-stale");
    let spec = copy_of("lm358-soic8", &tmp);
    approve_stage(&spec, "pinout", None, now_rfc3339()).unwrap();
    let out = kirpitch_app::import_pins(
        &spec,
        "1 OUT1 output\n2 IN1- input\n3 IN1+ input\n4 V- power\n5 IN2+ input\n6 IN2- input\n7 OUT2 output\n8 VCC power\n",
        3,
        None,
    )
    .unwrap();
    assert_eq!(out.view.gates[0].status, "stale");
}

#[test]
fn bad_regions_and_tables_give_readable_errors() {
    let tmp = TempDir::new("app-import-bad");
    let spec = copy_of("lm358-soic8", &tmp);
    for bad in [
        [0.5, 0.5, 0.1, 0.9],
        [0.1, 0.1, 0.1, 0.1],
        [0.0, 0.0, 1.5, 1.0],
    ] {
        assert!(
            kirpitch_app::read_region("x.pdf", 1, bad)
                .unwrap_err()
                .contains("empty or outside"),
            "{bad:?}"
        );
        assert!(kirpitch_app::import_pins(&spec, "1 A", 1, Some(bad)).is_err());
    }
    assert!(
        kirpitch_app::import_pins(&spec, "", 1, None)
            .unwrap_err()
            .contains("empty")
    );
    assert!(
        kirpitch_app::import_pins(&spec, "1 A\n,B\n", 1, None)
            .unwrap_err()
            .contains("line 2")
    );
    // A failed import leaves the file as it was.
    assert_eq!(open_spec(&spec).unwrap().pins.len(), 8);
}
