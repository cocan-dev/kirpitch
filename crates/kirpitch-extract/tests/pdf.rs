// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading synthetic PDFs created in the test (ADR 0020: no manufacturer PDFs).
//!
//! These tests need the pdfium library. Run `cargo xtask fetch-pdfium` and set
//! `KIRPITCH_PDFIUM_LIB_DIR` to the folder it prints; without it the tests are
//! skipped with a message, unless `KIRPITCH_REQUIRE_PDFIUM=1`.

use std::path::PathBuf;

use kirpitch_core::BoundingBox;
use kirpitch_extract::testing::{Item, make_pdf};
use kirpitch_extract::{PdfError, PdfReader};

fn reader() -> Option<PdfReader> {
    match std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR") {
        Some(dir) => Some(PdfReader::new(Some(&PathBuf::from(dir))).expect("load pdfium")),
        None => {
            assert!(
                !std::env::var("KIRPITCH_REQUIRE_PDFIUM").is_ok_and(|v| v == "1"),
                "KIRPITCH_REQUIRE_PDFIUM=1 but KIRPITCH_PDFIUM_LIB_DIR is not set"
            );
            eprintln!("SKIPPED: set KIRPITCH_PDFIUM_LIB_DIR (run `cargo xtask fetch-pdfium`)");
            None
        }
    }
}

fn pin_table_page() -> Vec<Item> {
    vec![
        (72.0, 720.0, 14.0, "Pin Functions"),
        (72.0, 690.0, 12.0, "NAME"),
        (200.0, 690.0, 12.0, "NO."),
        (72.0, 670.0, 12.0, "GND"),
        (200.0, 670.0, 12.0, "1"),
        (72.0, 650.0, 12.0, "TRIG"),
        (200.0, 650.0, 12.0, "2"),
        (72.0, 630.0, 12.0, "OUT"),
        (200.0, 630.0, 12.0, "3"),
    ]
}

#[test]
fn counts_pages_and_measures_them() {
    let Some(r) = reader() else { return };
    let pdf = make_pdf(&[pin_table_page(), vec![(72.0, 700.0, 12.0, "Page two")]]);
    assert_eq!(r.page_count(&pdf).unwrap(), 2);
    assert_eq!(r.page_size(&pdf, 1).unwrap(), (612.0, 792.0));
}

#[test]
fn renders_a_page_with_ink_on_it() {
    let Some(r) = reader() else { return };
    let pdf = make_pdf(&[pin_table_page()]);
    let img = r.render_page(&pdf, 1, 306).unwrap();
    assert_eq!(img.width, 306);
    assert_eq!(img.height, 396);
    assert_eq!(img.rgba.len() as u32, 306 * 396 * 4);
    let dark = img
        .rgba
        .chunks(4)
        .filter(|p| p[0] < 128 && p[1] < 128 && p[2] < 128)
        .count();
    assert!(dark > 100, "only {dark} dark pixels");
}

#[test]
fn characters_have_positions_from_the_top_left() {
    let Some(r) = reader() else { return };
    let pdf = make_pdf(&[pin_table_page()]);
    let chars = r.text_chars(&pdf, 1).unwrap();
    let g = chars.iter().find(|c| c.ch == 'G').expect("a G");
    let [l, t, rr, b] = g.bbox.0;
    // "GND" starts at x = 72 pt of 612 and its baseline is 670 pt up of 792.
    assert!((l - 72.0 / 612.0).abs() < 0.02, "left {l}");
    assert!(t < b && l < rr);
    assert!((b - (1.0 - 670.0 / 792.0)).abs() < 0.02, "bottom {b}");
}

#[test]
fn a_region_gives_the_table_text_line_by_line() {
    let Some(r) = reader() else { return };
    let pdf = make_pdf(&[pin_table_page()]);
    // Below the heading: from y = 600 to 700 pt, the whole width.
    let region = BoundingBox([0.0, 1.0 - 700.0 / 792.0, 1.0, 1.0 - 600.0 / 792.0]);
    let text = r.text_in_region(&pdf, 1, region).unwrap();
    assert_eq!(text, "NAME NO.\nGND 1\nTRIG 2\nOUT 3");

    // The words keep their columns.
    let lines = r.lines_in_region(&pdf, 1, region).unwrap();
    assert_eq!(lines.len(), 4);
    let gnd = &lines[1].words;
    assert_eq!(gnd.len(), 2);
    assert!(gnd[1].left > gnd[0].right + 0.05);

    // Only the right column.
    let right = BoundingBox([0.3, 1.0 - 700.0 / 792.0, 1.0, 1.0 - 600.0 / 792.0]);
    assert_eq!(r.text_in_region(&pdf, 1, right).unwrap(), "NO.\n1\n2\n3");
    // A region without text.
    let empty = BoundingBox([0.0, 0.0, 1.0, 0.05]);
    assert_eq!(r.text_in_region(&pdf, 1, empty).unwrap(), "");
}

#[test]
fn bad_input_gives_clear_errors() {
    let Some(r) = reader() else { return };
    let pdf = make_pdf(&[pin_table_page()]);
    assert!(matches!(
        r.page_size(&pdf, 2),
        Err(PdfError::NoSuchPage { page: 2, count: 1 })
    ));
    assert!(matches!(
        r.page_size(&pdf, 0),
        Err(PdfError::NoSuchPage { page: 0, .. })
    ));
    assert!(matches!(r.page_count(b"not a pdf"), Err(PdfError::Open(_))));
}

#[test]
fn a_missing_library_is_reported() {
    let dir = std::env::temp_dir().join("kirpitch-no-pdfium-here");
    assert!(matches!(
        PdfReader::new(Some(&dir)),
        Err(PdfError::Library(_))
    ));
}

#[test]
fn a_pin_table_region_becomes_a_table_that_the_pin_import_reads() {
    use kirpitch_core::pintable::{ImportOptions, parse_pin_table};
    use kirpitch_extract::{DEFAULT_COLUMN_GAP, table_from_lines};
    let Some(r) = reader() else { return };
    let pdf = make_pdf(&[pin_table_page()]);
    let region = BoundingBox([0.0, 1.0 - 700.0 / 792.0, 1.0, 1.0 - 620.0 / 792.0]);
    let lines = r.lines_in_region(&pdf, 1, region).unwrap();
    let table = table_from_lines(&lines, DEFAULT_COLUMN_GAP);
    assert_eq!(table.to_tsv(), "NAME\tNO.\nGND\t1\nTRIG\t2\nOUT\t3\n");
    let pins = parse_pin_table(&table.to_tsv(), &ImportOptions::default()).unwrap();
    let got: Vec<(&str, &str)> = pins
        .pins
        .iter()
        .map(|p| (p.number.as_str(), p.name.as_str()))
        .collect();
    assert_eq!(got, [("1", "GND"), ("2", "TRIG"), ("3", "OUT")]);
}
