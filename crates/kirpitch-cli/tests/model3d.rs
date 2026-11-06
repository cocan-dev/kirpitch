// SPDX-License-Identifier: GPL-3.0-or-later

//! The 3D model stage on the command line.

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

fn golden_copy(dir: &Path) -> String {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden/lm358-soic8/partspec.json");
    let to = dir.join("lm358.json");
    fs::copy(from, &to).unwrap();
    to.to_string_lossy().into_owned()
}

#[test]
fn gen_3d_writes_step_and_vrml() {
    let tmp = TempDir::new("cli-3d");
    let spec = golden_copy(tmp.path());
    let out_dir = tmp.path().join("out");
    let out = kirpitch(&["gen", "3d", &spec, "-o", &out_dir.to_string_lossy()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let folder = out_dir.join("kirpitch.3dshapes");
    let step = fs::read_to_string(folder.join("SOIC-8_3.9x4.9mm_P1.27mm.step")).unwrap();
    assert!(step.starts_with("ISO-10303-21;"));
    let wrl = fs::read_to_string(folder.join("SOIC-8_3.9x4.9mm_P1.27mm.wrl")).unwrap();
    assert!(wrl.starts_with("#VRML V2.0 utf8"));
    assert!(!out_dir.join("kirpitch.pretty").exists());
}

#[test]
fn the_3d_stage_is_reviewed_and_approved_after_the_footprint() {
    let tmp = TempDir::new("cli-3d-gate");
    let spec = golden_copy(tmp.path());

    let out = kirpitch(&["review", &spec, "--stage", "3d"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(shown.contains("Stage: 3d (open)"), "{shown}");
    assert!(shown.contains("1.75 mm high"), "{shown}");

    // Approving in the wrong order is refused.
    let out = kirpitch(&["approve", &spec, "--stage", "3d"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out.stderr).contains("approve the footprint stage first"));

    for stage in ["pinout", "footprint", "3d"] {
        let out = kirpitch(&["approve", &spec, "--stage", stage]);
        assert!(out.status.success(), "{stage}: {}", text(&out.stderr));
    }
    let out = kirpitch(&["status", &spec]);
    let shown = text(&out.stdout);
    assert_eq!(shown.matches("approved").count(), 3, "{shown}");

    // Editing the footprint opens the 3D stage again.
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("\"height_max\": 1.75", "\"height_max\": 1.7");
    fs::write(&spec, edited).unwrap();
    let shown = text(&kirpitch(&["status", &spec]).stdout);
    assert!(
        shown.contains("3d") && shown.contains("open again"),
        "{shown}"
    );

    // The model has nothing to edit by itself.
    let out = kirpitch(&["review", &spec, "--stage", "3d", "--edit"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("edit the footprint stage"));
}
