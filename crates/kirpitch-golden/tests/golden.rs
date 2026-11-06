// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs every part of the golden set (`testdata/golden/*`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kirpitch_core::{FabProfile, PartSpec, has_errors, validate};
use kirpitch_kicad::KicadTarget;
use kirpitch_kicad::testing::{
    assert_footprint_loads, assert_snapshot, assert_symbol_loads, kicad_cli_or_skip,
};

fn golden_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden")
}

/// All golden part folders, sorted by name.
fn parts() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(golden_root())
        .expect("testdata/golden exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("partspec.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

fn load(dir: &Path) -> PartSpec {
    let text = fs::read_to_string(dir.join("partspec.json")).expect("read partspec.json");
    PartSpec::from_json_str(&text).unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
}

fn manifest(dir: &Path) -> serde_json::Value {
    let text = fs::read_to_string(dir.join("datasheet.json")).expect("read datasheet.json");
    serde_json::from_str(&text).expect("datasheet.json is JSON")
}

#[test]
fn the_golden_set_has_the_three_soic8_parts() {
    let names: Vec<String> = parts()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["24lc256-soic8", "lm358-soic8", "ne555-soic8"]);
}

#[test]
fn manifests_record_url_and_hash_but_no_pdf() {
    for dir in parts() {
        let m = manifest(&dir);
        let url = m["url"].as_str().expect("url");
        assert!(url.starts_with("https://"), "{}: url", dir.display());
        let hash = m["sha256"].as_str().expect("sha256");
        assert_eq!(hash.len(), 64, "{}: hash length", dir.display());
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{}: hash must be lowercase hex",
            dir.display()
        );
        for key in [
            "manufacturer",
            "part",
            "revision",
            "checked_on",
            "package",
            "pages",
        ] {
            assert!(!m[key].is_null(), "{}: `{key}` missing", dir.display());
        }
        // No datasheet PDF may be stored with the test data.
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            assert!(!name.ends_with(".pdf"), "{}: PDF committed", dir.display());
        }
    }
}

#[test]
fn partspecs_match_their_manifest_and_are_traced_to_pages() {
    for dir in parts() {
        let spec = load(&dir);
        let m = manifest(&dir);
        assert_eq!(spec.metadata.datasheet_url.as_deref(), m["url"].as_str());
        assert_eq!(
            spec.metadata.datasheet_sha256.as_deref(),
            m["sha256"].as_str()
        );
        assert_eq!(Some(spec.metadata.mpn.as_str()), m["part"].as_str());
        // Golden parts carry a source page for every pin and pad.
        for (i, pin) in spec.pins.iter().enumerate() {
            assert!(
                pin.source.is_some(),
                "{}: pins[{i}] has no source",
                dir.display()
            );
        }
        for (i, pad) in spec.land_pattern.pads.iter().enumerate() {
            assert!(
                pad.source.is_some(),
                "{}: pads[{i}] has no source",
                dir.display()
            );
        }
        // The pages named in the manifest are the ones used in the PartSpec.
        let pages = &m["pages"];
        assert_eq!(
            spec.pins[0].source.unwrap().page as u64,
            pages["pinout"].as_u64().unwrap(),
            "{}: pinout page",
            dir.display()
        );
        assert_eq!(
            spec.land_pattern.source.unwrap().page as u64,
            pages["land_pattern"].as_u64().unwrap(),
            "{}: land pattern page",
            dir.display()
        );
        assert_eq!(
            spec.package.source.unwrap().page as u64,
            pages["package_outline"].as_u64().unwrap(),
            "{}: package page",
            dir.display()
        );
    }
}

#[test]
fn partspec_files_are_in_canonical_form() {
    // Rewrite with KIRPITCH_UPDATE_SNAPSHOTS=1 after editing a PartSpec.
    for dir in parts() {
        assert_snapshot(&dir.join("partspec.json"), &load(&dir).to_json_string());
    }
}

#[test]
fn partspecs_have_no_findings() {
    for dir in parts() {
        let findings = validate(&load(&dir), &FabProfile::generic());
        assert!(findings.is_empty(), "{}: {findings:?}", dir.display());
    }
}

fn footprint_for(spec: &PartSpec, target: KicadTarget) -> String {
    let options = kirpitch_gen_footprint::Options {
        target,
        model: Some(format!(
            "${{KIPRJMOD}}/kirpitch.3dshapes/{}",
            kirpitch_gen_3d::file_names(spec).0
        )),
        ..Default::default()
    };
    kirpitch_gen_footprint::generate(spec, &options).unwrap()
}

fn symbol_for(spec: &PartSpec, target: KicadTarget) -> String {
    let options = kirpitch_gen_symbol::Options {
        target,
        ..Default::default()
    };
    kirpitch_gen_symbol::generate(spec, &options).unwrap()
}

/// `expected/kicad<major>` of a golden part.
fn expected_dir(dir: &Path, target: KicadTarget) -> PathBuf {
    dir.join("expected")
        .join(format!("kicad{}", target.major()))
}

#[test]
fn generated_files_match_the_expected_files() {
    for dir in parts() {
        let spec = load(&dir);
        for &target in KicadTarget::ALL {
            let start = Instant::now();
            let footprint = footprint_for(&spec, target);
            let symbol = symbol_for(&spec, target);
            assert!(
                start.elapsed().as_secs_f64() < 1.0,
                "{}: generation took too long",
                dir.display()
            );
            let expected = expected_dir(&dir, target);
            assert_snapshot(
                &expected.join(kirpitch_gen_footprint::file_name(&spec)),
                &footprint,
            );
            assert_snapshot(
                &expected.join(kirpitch_gen_symbol::file_name(&spec)),
                &symbol,
            );
        }
    }
}

#[test]
fn the_3d_files_match_the_expected_files() {
    for dir in parts() {
        let spec = load(&dir);
        let model = kirpitch_gen_3d::build(&spec).unwrap();
        assert_eq!(
            kirpitch_gen_3d::check_fit(&spec, &model),
            vec![],
            "{}",
            dir.display()
        );
        let (step, wrl) = kirpitch_gen_3d::file_names(&spec);
        let expected = dir.join("expected").join("3d");
        assert_snapshot(&expected.join(step), &kirpitch_gen_3d::write_step(&model));
        assert_snapshot(&expected.join(wrl), &kirpitch_gen_3d::write_vrml(&model));
    }
}

#[test]
fn there_are_no_expected_files_for_unsupported_versions() {
    for dir in parts() {
        let mut found: Vec<String> = fs::read_dir(dir.join("expected"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        let mut want: Vec<String> = KicadTarget::ALL
            .iter()
            .map(|t| format!("kicad{}", t.major()))
            .collect();
        want.push("3d".to_string());
        let mut want_sorted = want.clone();
        want_sorted.sort();
        assert_eq!(found, want_sorted, "{}", dir.display());
    }
}

#[test]
fn generation_is_deterministic() {
    for dir in parts() {
        let spec = load(&dir);
        let again = load(&dir);
        for &target in KicadTarget::ALL {
            assert_eq!(footprint_for(&spec, target), footprint_for(&again, target));
            assert_eq!(symbol_for(&spec, target), symbol_for(&again, target));
        }
    }
}

#[test]
fn kicad_loads_every_golden_part() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    for dir in parts() {
        let spec = load(&dir);
        assert!(!has_errors(&validate(&spec, &FabProfile::generic())));
        let label = dir.file_name().unwrap().to_string_lossy().into_owned();
        // Each KiCad version can only check the files written for itself.
        for &target in KicadTarget::ALL {
            assert_footprint_loads(
                &cli,
                target,
                &format!("{label}-fp"),
                &footprint_for(&spec, target),
                &kirpitch_gen_footprint::file_name(&spec),
            );
            assert_symbol_loads(
                &cli,
                target,
                &format!("{label}-sym"),
                &symbol_for(&spec, target),
                &kirpitch_gen_symbol::file_name(&spec),
            );
        }
    }
}
