// SPDX-License-Identifier: GPL-3.0-or-later

//! The KLC checks accept the official KiCad footprints and reject broken ones.

use std::fs;
use std::path::PathBuf;

use kirpitch_kicad::klc::check_footprint;

const TINY: &str = include_str!("fixtures/tiny.kicad_mod");

fn golden_footprints() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden");
    let mut out = Vec::new();
    for part in fs::read_dir(root).unwrap().flatten() {
        let expected = part.path().join("expected");
        for version in fs::read_dir(expected).unwrap().flatten() {
            for f in fs::read_dir(version.path()).unwrap().flatten() {
                if f.path().extension().is_some_and(|e| e == "kicad_mod") {
                    out.push(f.path());
                }
            }
        }
    }
    out
}

#[test]
fn generated_golden_footprints_follow_the_conventions() {
    let files = golden_footprints();
    assert!(files.len() >= 9);
    for f in files {
        let found = check_footprint(&fs::read_to_string(&f).unwrap()).unwrap();
        assert!(found.is_empty(), "{}: {found:?}", f.display());
    }
}

#[test]
fn a_bare_footprint_is_reported_in_full() {
    let ids: Vec<&str> = check_footprint(TINY)
        .unwrap()
        .iter()
        .map(|f| f.check)
        .collect();
    for want in [
        "courtyard_present",
        "fab_present",
        "reference_layer",
        "value_layer",
        "reference_text",
    ] {
        assert!(ids.contains(&want), "{want} missing in {ids:?}");
    }
    assert!(!ids.contains(&"pads_present"));
}

#[test]
fn single_deviations_are_found() {
    let good = fs::read_to_string(&golden_footprints()[0]).unwrap();
    let check = |text: &str| -> Vec<&'static str> {
        check_footprint(text)
            .unwrap()
            .iter()
            .map(|f| f.check)
            .collect()
    };
    assert_eq!(check(&good), Vec::<&str>::new());
    // Courtyard line width and grid.
    assert!(check(&good.replace("(width 0.05)", "(width 0.1)")).contains(&"courtyard_width"));
    let off_grid = good.replacen(
        "(fp_rect
		(start ",
        "(fp_rect
		(start 0.001 ",
        1,
    );
    assert!(check(&off_grid).contains(&"courtyard_grid"));
    // Silkscreen too thin.
    assert!(check(&good.replace("(width 0.12)", "(width 0.1)")).contains(&"silk_width"));
    // Reference on the wrong layer.
    let bad_ref = good.replacen("(layer \"F.SilkS\")", "(layer \"F.Fab\")", 1);
    assert!(check(&bad_ref).contains(&"reference_layer"));
    // Not parsable.
    assert!(check_footprint("(footprint").is_err());
}

/// Optional: every SOIC, TSSOP, SOT-23 and DIP footprint of the installed KiCad
/// libraries must pass too. Set `KIRPITCH_KICAD_FOOTPRINTS` to the folder that
/// holds the `.pretty` libraries (for example
/// `C:\Program Files\KiCad\10.0\share\kicad\footprints`).
#[test]
fn official_footprints_follow_the_conventions() {
    let Some(root) = std::env::var_os("KIRPITCH_KICAD_FOOTPRINTS") else {
        eprintln!("SKIPPED: set KIRPITCH_KICAD_FOOTPRINTS to check the official libraries");
        return;
    };
    let mut checked = 0;
    let mut failed = 0;
    for lib in [
        "Package_SO.pretty",
        "Package_DIP.pretty",
        "Package_TO_SOT_SMD.pretty",
    ] {
        for f in fs::read_dir(PathBuf::from(&root).join(lib))
            .unwrap()
            .flatten()
        {
            let name = f.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".kicad_mod") {
                continue;
            }
            let found = check_footprint(&fs::read_to_string(f.path()).unwrap()).unwrap();
            if !found.is_empty() {
                eprintln!("{lib}/{name}: {found:?}");
                failed += 1;
            }
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} files checked");
    // A few hand-drawn official footprints break the grid or width rules; the
    // generated families must not, so require almost all of them to pass.
    assert!(
        failed * 50 <= checked,
        "{failed} of {checked} official footprints fail the checks"
    );
}
