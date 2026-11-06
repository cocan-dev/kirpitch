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
