// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading synthetic PDFs created in the test (ADR 0020: no manufacturer PDFs).
//!
//! These tests need the pdfium library. Run `cargo xtask fetch-pdfium` and set
//! `KIRPITCH_PDFIUM_LIB_DIR` to the folder it prints; without it the tests are
//! skipped with a message, unless `KIRPITCH_REQUIRE_PDFIUM=1`.

use std::path::PathBuf;

use kirpitch_core::BoundingBox;
use kirpitch_extract::{PdfError, PdfReader};

/// One text item: `(x, y, font size, text)` in points from the bottom-left.
type Item = (f32, f32, f32, &'static str);

/// Builds a PDF with one US Letter page (612 x 792 pt) per entry of `pages`.
fn make_pdf(pages: &[Vec<Item>]) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", 4 + 2 * i))
        .collect();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (i, items) in pages.iter().enumerate() {
        let content_id = 5 + 2 * i;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {content_id} 0 R /Resources << /Font << /F1 3 0 R >> >> >>"
        ));
        let mut stream = String::new();
        for (x, y, size, text) in items {
            stream.push_str(&format!("BT /F1 {size} Tf {x} {y} Td ({text}) Tj ET\n"));
        }
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}endstream",
            stream.len(),
            stream
        ));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, body));
    }
    let xref = out.len();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for o in offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

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
