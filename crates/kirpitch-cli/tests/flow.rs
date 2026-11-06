// SPDX-License-Identifier: GPL-3.0-or-later

//! The review flow without a language model, driven through the binary.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use kirpitch_kicad::testing::TempDir;

fn kirpitch(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kirpitch"))
        .args(args)
        .output()
        .expect("run kirpitch")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

const PIN_TABLE: &str = "Pin,Name,Type,Group
1,OUT1,Output,Amplifier 1
2,IN1-,Input,Amplifier 1
3,IN1+,Input,Amplifier 1
4,V-,Power,Power
5,IN2+,Input,Amplifier 2
6,IN2-,Input,Amplifier 2
7,OUT2,Output,Amplifier 2
8,V+,Power,Power
";

/// Creates a SOIC-8 PartSpec from the template and the pin table.
fn make_spec(dir: &Path) -> std::path::PathBuf {
    let spec = dir.join("spec.json");
    let table = dir.join("pins.csv");
    fs::write(&table, PIN_TABLE).unwrap();
    let out = kirpitch(&[
        "template",
        &path(&spec),
        "--mpn",
        "TEST-OPAMP",
        "--manufacturer",
        "Test",
        "--description",
        "Dual op-amp",
        "--keywords",
        "opamp,dual",
        "dual-row",
        "--family",
        "SOIC",
        "--pins",
        "8",
        "--pitch",
        "1.27",
        "--body-length",
        "4.9",
        "--body-width",
        "3.9",
        "--height",
        "1.75",
        "--span",
        "5.8..6.2",
        "--lead-length",
        "0.4..1.27",
        "--lead-width",
        "0.31..0.51",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("SOIC-8_3.9x4.9mm_P1.27mm"));
    let out = kirpitch(&[
        "pins",
        &path(&spec),
        "--import",
        &path(&table),
        "--page",
        "3",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("read 8 pins"));
    spec
}

#[test]
fn from_template_and_pin_table_to_approved_kicad_files() {
    let tmp = TempDir::new("flow");
    let spec = make_spec(tmp.path());
    let s = path(&spec);

    // Nothing is approved yet.
    let out = kirpitch(&["status", &s]);
    let status = text(&out.stdout);
    assert!(status.contains("pinout     open"), "{status}");
    assert!(status.contains("footprint  open"), "{status}");

    // The footprint cannot come first.
    let out = kirpitch(&["approve", &s, "--stage", "footprint"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out.stderr).contains("approve the pinout stage first"));

    // Review shows the pins with their source page.
    let out = kirpitch(&["review", &s, "--stage", "pinout"]);
    let review = text(&out.stdout);
    assert!(review.contains("8 pins (package says 8)"), "{review}");
    assert!(review.contains("IN1-"), "{review}");
    assert!(review.contains("p.3"), "{review}");

    let out = kirpitch(&["approve", &s, "--stage", "pinout", "--note", "checked"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = kirpitch(&["review", &s, "--stage", "footprint"]);
    let review = text(&out.stdout);
    assert!(review.contains("SOIC-8_3.9x4.9mm_P1.27mm"), "{review}");
    assert!(review.contains("-2.475"), "{review}");
    let out = kirpitch(&["approve", &s, "--stage", "footprint"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let status = text(&kirpitch(&["status", &s]).stdout);
    assert!(status.contains("pinout     approved"), "{status}");
    assert!(status.contains("footprint  approved"), "{status}");

    // The approved PartSpec generates KiCad files.
    let out_dir = tmp.path().join("out");
    let out = kirpitch(&["gen", "all", &s, "-o", &path(&out_dir), "--target", "10"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(out_dir.join("TEST-OPAMP.kicad_sym").is_file());
    assert!(
        out_dir
            .join("kirpitch.pretty/SOIC-8_3.9x4.9mm_P1.27mm.kicad_mod")
            .is_file()
    );
}

#[test]
fn editing_the_pinout_reopens_both_stages() {
    let tmp = TempDir::new("flow-stale");
    let spec = make_spec(tmp.path());
    let s = path(&spec);
    assert!(
        kirpitch(&["approve", &s, "--stage", "pinout"])
            .status
            .success()
    );
    assert!(
        kirpitch(&["approve", &s, "--stage", "footprint"])
            .status
            .success()
    );

    // Import the pin table again with one name changed.
    let table = tmp.path().join("pins2.csv");
    fs::write(&table, PIN_TABLE.replace("OUT1", "OUTPUT1")).unwrap();
    let out = kirpitch(&["pins", &s, "--import", &path(&table)]);
    assert!(out.status.success());
    let status = text(&kirpitch(&["status", &s]).stdout);
    assert!(status.contains("pinout     open again"), "{status}");
    assert!(status.contains("footprint  open again"), "{status}");
}

#[test]
fn errors_block_the_approval_and_are_listed_by_field() {
    let tmp = TempDir::new("flow-errors");
    let spec = make_spec(tmp.path());
    let s = path(&spec);
    let table = tmp.path().join("bad.csv");
    fs::write(&table, "1 A\n1 B\n").unwrap();
    assert!(
        kirpitch(&["pins", &s, "--import", &path(&table)])
            .status
            .success()
    );
    let out = kirpitch(&["approve", &s, "--stage", "pinout"]);
    assert_eq!(out.status.code(), Some(3));
    let err = text(&out.stderr);
    assert!(err.contains("error[pin_count]"), "{err}");
    assert!(err.contains("error[pin_number] pins[1].number"), "{err}");
    let status = text(&kirpitch(&["status", &s]).stdout);
    assert!(status.contains("pinout     open"), "{status}");
}

#[test]
fn guessed_pin_types_are_reported_on_import() {
    let tmp = TempDir::new("flow-guess");
    let spec = make_spec(tmp.path());
    let table = tmp.path().join("plain.txt");
    fs::write(&table, "1 GND\n2 TRIG\n").unwrap();
    let out = kirpitch(&["pins", &path(&spec), "--import", &path(&table)]);
    assert!(out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("warning: line 2: type of pin `TRIG`"), "{err}");
}

#[test]
fn a_new_file_needs_its_metadata() {
    let tmp = TempDir::new("flow-meta");
    let spec = tmp.path().join("new.json");
    let out = kirpitch(&[
        "template",
        &path(&spec),
        "dip",
        "--pins",
        "8",
        "--body-length",
        "9.27",
        "--body-width",
        "6.35",
        "--height",
        "5.08",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("give --mpn to create it"));
    assert!(!spec.exists());
}

#[test]
fn a_dip_template_replaces_package_and_pads_but_keeps_the_pins() {
    let tmp = TempDir::new("flow-dip");
    let spec = make_spec(tmp.path());
    let s = path(&spec);
    let out = kirpitch(&[
        "template",
        &s,
        "dip",
        "--pins",
        "8",
        "--body-length",
        "9.27",
        "--body-width",
        "6.35",
        "--height",
        "5.08",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let review = text(&kirpitch(&["review", &s, "--stage", "footprint"]).stdout);
    assert!(review.contains("DIP-8_W7.62mm"), "{review}");
    let pins = text(&kirpitch(&["review", &s, "--stage", "pinout"]).stdout);
    assert!(pins.contains("OUT2"), "{pins}");
}

#[test]
fn bad_ranges_are_explained() {
    let tmp = TempDir::new("flow-range");
    let out = kirpitch(&[
        "template",
        &path(&tmp.path().join("x.json")),
        "--mpn",
        "A",
        "--manufacturer",
        "B",
        "--description",
        "C",
        "dual-row",
        "--pins",
        "8",
        "--pitch",
        "1.27",
        "--body-length",
        "4.9",
        "--body-width",
        "3.9",
        "--height",
        "1.75",
        "--span",
        "6.2..5.8",
        "--lead-length",
        "0.4..1.27",
        "--lead-width",
        "0.31..0.51",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("must not exceed"),
        "{}",
        text(&out.stderr)
    );
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
fn pins_can_be_read_from_a_region_of_a_pdf() {
    let Some(dir) = std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR") else {
        eprintln!("SKIPPED: set KIRPITCH_PDFIUM_LIB_DIR to run this test");
        return;
    };
    let _ = dir;
    let tmp = TempDir::new("flow-pdf");
    let spec = make_spec(tmp.path());
    let s = path(&spec);
    let pdf = tmp.path().join("synthetic.pdf");
    fs::write(&pdf, kirpitch_extract::testing::make_pdf(&[pin_page()])).unwrap();

    // The region starts under the heading and covers the table (y from 700 to
    // 520 pt of 792).
    let top = 1.0 - 725.0 / 792.0;
    let bottom = 1.0 - 515.0 / 792.0;
    let region = format!("0.0,{top},1.0,{bottom}");
    let out = kirpitch(&[
        "pins",
        &s,
        "--pdf",
        &path(&pdf),
        "--page",
        "1",
        "--region",
        &region,
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("read 8 pins"));

    let review = text(&kirpitch(&["review", &s, "--stage", "pinout"]).stdout);
    assert!(review.contains("IN2+"), "{review}");
    assert!(review.contains("p.1"), "{review}");
    // The region is saved as the source of the pins.
    let json = fs::read_to_string(&spec).unwrap();
    assert!(json.contains("\"bbox\""), "{json}");
    // The pins from the PDF pass the pinout gate.
    let out = kirpitch(&["approve", &s, "--stage", "pinout"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
}

#[test]
fn bad_region_arguments_are_explained() {
    let tmp = TempDir::new("flow-region");
    let spec = make_spec(tmp.path());
    let s = path(&spec);
    for (region, want) in [
        ("0.5,0.5,0.1,0.9", "left <= right"),
        ("0.1,0.2,0.3", "four numbers"),
        ("a,b,c,d", "four numbers"),
    ] {
        let out = kirpitch(&[
            "pins", &s, "--pdf", "x.pdf", "--page", "1", "--region", region,
        ]);
        assert_eq!(out.status.code(), Some(2), "{region}");
        assert!(
            text(&out.stderr).contains(want),
            "{region}: {}",
            text(&out.stderr)
        );
    }
    // Neither --import nor --pdf.
    let out = kirpitch(&["pins", &s]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("give --import FILE"));
}
