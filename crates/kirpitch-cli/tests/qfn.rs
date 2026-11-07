// SPDX-License-Identifier: GPL-3.0-or-later

//! A QFN from template to KiCad files, with a maker's fab profile.

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

fn make_qfn(dir: &Path) -> String {
    let spec = path(&dir.join("qfn.json"));
    let mut table = String::from("Pin,Name,Type\n");
    for n in 1..=32 {
        table.push_str(&format!("{n},IO{n},Input\n"));
    }
    table.push_str("33,EP,Passive\n");
    let csv = dir.join("pins.csv");
    fs::write(&csv, table).unwrap();
    let out = kirpitch(&[
        "template",
        &spec,
        "--mpn",
        "TEST-QFN32",
        "--manufacturer",
        "Test",
        "--description",
        "QFN from a template",
        "qfn",
        "--pins-x",
        "8",
        "--pins-y",
        "8",
        "--pitch",
        "0.4",
        "--body-x",
        "3.95..4.05",
        "--body-y",
        "3.95..4.05",
        "--height",
        "1.0",
        "--lead-length",
        "0.35..0.45",
        "--lead-width",
        "0.15..0.25",
        "--exposed-pad",
        "2.65,2.65",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("QFN-32-1EP_4x4mm_P0.4mm_EP2.65x2.65mm"));
    let out = kirpitch(&["pins", &spec, "--import", &path(&csv)]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    spec
}

#[test]
fn a_fine_pitch_qfn_needs_a_finer_fab_profile_than_the_generic_one() {
    let tmp = TempDir::new("cli-qfn");
    let spec = make_qfn(tmp.path());

    // The generic profile wants 0.2 mm between pads; the corners of this
    // package are 0.07 mm apart (as in KiCad's own footprint).
    let out = kirpitch(&["check", &spec]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        text(&out.stdout).contains("error[pad_clearance]"),
        "{}",
        text(&out.stdout)
    );
    let out = kirpitch(&[
        "gen",
        "footprint",
        &spec,
        "-o",
        &path(&tmp.path().join("no")),
        "--target",
        "10",
    ]);
    assert_eq!(out.status.code(), Some(3));

    // A maker profile with a finer process passes.
    let fab = tmp.path().join("fab.json");
    fs::write(
        &fab,
        r#"{"name": "fine-pitch maker", "min_copper_gap": 0.05, "min_mask_web": 0.05}"#,
    )
    .unwrap();
    let out = kirpitch(&["check", &spec, "--fab", &path(&fab)]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert!(text(&out.stdout).contains("warning[exposed_pad]"));

    let out_dir = tmp.path().join("out");
    let out = kirpitch(&[
        "gen",
        "all",
        &spec,
        "-o",
        &path(&out_dir),
        "--target",
        "10",
        "--fab",
        &path(&fab),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let footprint = fs::read_to_string(
        out_dir.join("kirpitch.pretty/QFN-32-1EP_4x4mm_P0.4mm_EP2.65x2.65mm.kicad_mod"),
    )
    .unwrap();
    assert!(footprint.contains("(pad \"33\" smd rect"));
    assert!(footprint.contains("(size 2.65 2.65)"));
}

#[test]
fn a_bad_fab_profile_is_an_error() {
    let tmp = TempDir::new("cli-fab-bad");
    let spec = make_qfn(tmp.path());
    let fab = tmp.path().join("bad.json");
    fs::write(&fab, r#"{"min_copper_gap": -1}"#).unwrap();
    let out = kirpitch(&["check", &spec, "--fab", &path(&fab)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("min_copper_gap"));
    let out = kirpitch(&["check", &spec, "--fab", "no/such/profile.json"]);
    assert_eq!(out.status.code(), Some(1));
}
