// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs the `kirpitch` binary.

use std::fs;
use std::path::{Path, PathBuf};
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

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden")
        .join(name)
}

fn spec(name: &str) -> String {
    golden(name)
        .join("partspec.json")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn schema_prints_the_committed_schema() {
    let out = kirpitch(&["schema"]);
    assert!(out.status.success());
    let committed = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/partspec.schema.json"),
    )
    .unwrap();
    assert_eq!(text(&out.stdout), committed.replace("\r\n", "\n"));
}

#[test]
fn check_accepts_a_valid_partspec() {
    let out = kirpitch(&["check", &spec("lm358-soic8")]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "0 error(s), 0 warning(s)\n");
}

#[test]
fn check_reports_errors_by_field_with_exit_code_3() {
    let tmp = TempDir::new("cli-check");
    let broken = fs::read_to_string(spec("lm358-soic8"))
        .unwrap()
        .replace("\"pin_count\": 8", "\"pin_count\": 9");
    let file = tmp.path().join("broken.json");
    fs::write(&file, broken).unwrap();
    let out = kirpitch(&["check", &file.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(3));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("error[pin_count] pins:"), "{stdout}");
    assert!(stdout.contains("1 error(s), 0 warning(s)"), "{stdout}");
}

#[test]
fn gen_writes_exactly_the_expected_golden_files() {
    for (target, folder) in [("10", "kicad10"), ("9", "kicad9"), ("8", "kicad8")] {
        for part in ["lm358-soic8", "ne555-soic8", "24lc256-soic8"] {
            let tmp = TempDir::new("cli-gen");
            let out_dir = tmp.path().join("out");
            let out = kirpitch(&[
                "gen",
                "all",
                &spec(part),
                "-o",
                &out_dir.to_string_lossy(),
                "--target",
                target,
            ]);
            assert!(out.status.success(), "{}", text(&out.stderr));
            let stdout = text(&out.stdout);
            assert_eq!(stdout.lines().count(), 2, "{stdout}");

            let expected = golden(part).join("expected").join(folder);
            let mut checked = 0;
            for entry in fs::read_dir(&expected).unwrap().flatten() {
                let name = entry.file_name();
                let generated = if name.to_string_lossy().ends_with(".kicad_mod") {
                    out_dir.join("kirpitch.pretty").join(&name)
                } else {
                    out_dir.join(&name)
                };
                let got =
                    fs::read(&generated).unwrap_or_else(|e| panic!("{}: {e}", generated.display()));
                let want = fs::read_to_string(entry.path())
                    .unwrap()
                    .replace("\r\n", "\n");
                assert_eq!(String::from_utf8(got).unwrap(), want, "{part}/{name:?}");
                checked += 1;
            }
            assert_eq!(checked, 2);
        }
    }
}

#[test]
fn gen_can_write_only_the_symbol() {
    let tmp = TempDir::new("cli-sym");
    let out = kirpitch(&[
        "gen",
        "symbol",
        &spec("ne555-soic8"),
        "-o",
        &tmp.path().to_string_lossy(),
        "--target",
        "10",
    ]);
    assert!(out.status.success());
    assert!(tmp.path().join("NE555DR.kicad_sym").is_file());
    assert!(!tmp.path().join("kirpitch.pretty").exists());
}

#[test]
fn gen_refuses_an_invalid_partspec_and_writes_nothing() {
    let tmp = TempDir::new("cli-invalid");
    let broken = fs::read_to_string(spec("lm358-soic8"))
        .unwrap()
        .replace("\"pin_count\": 8", "\"pin_count\": 9");
    let file = tmp.path().join("broken.json");
    fs::write(&file, broken).unwrap();
    let out_dir = tmp.path().join("out");
    let out = kirpitch(&[
        "gen",
        "all",
        &file.to_string_lossy(),
        "-o",
        &out_dir.to_string_lossy(),
        "--target",
        "10",
    ]);
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out.stderr).contains("error[pin_count]"));
    assert!(!out_dir.exists());
}

#[test]
fn unsupported_target_and_missing_file_are_errors() {
    let tmp = TempDir::new("cli-errors");
    let out_dir = tmp.path().join("out");
    let out = kirpitch(&[
        "gen",
        "all",
        &spec("lm358-soic8"),
        "-o",
        &out_dir.to_string_lossy(),
        "--target",
        "7",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("KiCad 7 is not a supported target"));

    let out = kirpitch(&["check", &tmp.path().join("nothing.json").to_string_lossy()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).starts_with("error:"));
}

#[test]
fn a_bad_partspec_file_names_the_field() {
    let tmp = TempDir::new("cli-badfield");
    let bad =
        fs::read_to_string(spec("lm358-soic8"))
            .unwrap()
            .replacen("\"output\"", "\"outpt\"", 1);
    let file = tmp.path().join("bad.json");
    fs::write(&file, bad).unwrap();
    let out = kirpitch(&["check", &file.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("pins[0].electrical_type"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn help_and_usage_errors() {
    let out = kirpitch(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("Usage: kirpitch"));
    let out = kirpitch(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!text(&out.stderr).is_empty());
}

#[test]
fn output_files_use_unix_line_endings() {
    let tmp = TempDir::new("cli-lf");
    let out = kirpitch(&[
        "gen",
        "all",
        &spec("lm358-soic8"),
        "-o",
        &tmp.path().to_string_lossy(),
        "--target",
        "10",
    ]);
    assert!(out.status.success());
    let bytes = fs::read(tmp.path().join("LM358DR.kicad_sym")).unwrap();
    assert!(!bytes.contains(&b'\r'));
}
