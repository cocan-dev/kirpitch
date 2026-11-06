// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs every part of the golden set (`testdata/golden/*`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kirpitch_core::{FabProfile, PartSpec, has_errors, validate};
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

#[test]
fn generated_files_match_the_expected_files() {
    for dir in parts() {
        let spec = load(&dir);
        let start = Instant::now();
        let footprint = kirpitch_gen_footprint::generate(&spec, &Default::default()).unwrap();
        let symbol = kirpitch_gen_symbol::generate(&spec, &Default::default()).unwrap();
        assert!(
            start.elapsed().as_secs_f64() < 1.0,
            "{}: generation took too long",
            dir.display()
        );
        assert_snapshot(
            &dir.join("expected")
                .join(kirpitch_gen_footprint::file_name(&spec)),
            &footprint,
        );
        assert_snapshot(
            &dir.join("expected")
                .join(kirpitch_gen_symbol::file_name(&spec)),
            &symbol,
        );
    }
}

#[test]
fn generation_is_deterministic() {
    for dir in parts() {
        let spec = load(&dir);
        let again = load(&dir);
        assert_eq!(
            kirpitch_gen_footprint::generate(&spec, &Default::default()),
            kirpitch_gen_footprint::generate(&again, &Default::default())
        );
        assert_eq!(
            kirpitch_gen_symbol::generate(&spec, &Default::default()),
            kirpitch_gen_symbol::generate(&again, &Default::default())
        );
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
        let footprint = kirpitch_gen_footprint::generate(&spec, &Default::default()).unwrap();
        assert_footprint_loads(
            &cli,
            &format!("{label}-fp"),
            &footprint,
            &kirpitch_gen_footprint::file_name(&spec),
        );
        let symbol = kirpitch_gen_symbol::generate(&spec, &Default::default()).unwrap();
        assert_symbol_loads(
            &cli,
            &format!("{label}-sym"),
            &symbol,
            &kirpitch_gen_symbol::file_name(&spec),
        );
    }
}
