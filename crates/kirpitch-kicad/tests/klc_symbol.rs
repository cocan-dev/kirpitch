// SPDX-License-Identifier: GPL-3.0-or-later

//! The symbol checks accept the generated symbols and reject broken ones.

use std::fs;
use std::path::PathBuf;

use kirpitch_kicad::klc_symbol::check_symbol;

fn golden_symbols() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden");
    let mut out = Vec::new();
    for part in fs::read_dir(root).unwrap().flatten() {
        let expected = part.path().join("expected");
        for version in fs::read_dir(expected).unwrap().flatten() {
            for f in fs::read_dir(version.path()).unwrap().flatten() {
                if f.path().extension().is_some_and(|e| e == "kicad_sym") {
                    out.push(f.path());
                }
            }
        }
    }
    out
}

#[test]
fn generated_golden_symbols_follow_the_conventions() {
    let files = golden_symbols();
    assert!(files.len() >= 9);
    for f in files {
        let found = check_symbol(&fs::read_to_string(&f).unwrap()).unwrap();
        assert!(found.is_empty(), "{}: {found:?}", f.display());
    }
}

fn lm358() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden");
    fs::read_to_string(root.join("lm358-soic8/expected/kicad10/LM358DR.kicad_sym"))
        .unwrap()
        .replace("\r\n", "\n")
}

fn ids(text: &str) -> Vec<&'static str> {
    check_symbol(text)
        .unwrap()
        .iter()
        .map(|f| f.check)
        .collect()
}

#[test]
fn single_deviations_are_found() {
    let good = lm358();
    assert_eq!(ids(&good), Vec::<&str>::new());
    // Pin off the grid.
    assert!(
        ids(&good.replacen("(at -7.62 5.08 0)", "(at -7.62 5.1 0)", 1))
            .contains(&"symbol_pin_grid")
    );
    // Pin too short.
    assert!(
        ids(&good.replacen("(length 2.54)", "(length 1.27)", 1)).contains(&"symbol_pin_length")
    );
    // Pin that no longer touches the body.
    assert!(ids(&good.replacen("(length 2.54)", "(length 3.81)", 1)).contains(&"symbol_pin_body"));
    // Value that is not the symbol name.
    assert!(
        ids(&good.replacen(
            "(property \"Value\" \"LM358DR\"",
            "(property \"Value\" \"X\"",
            1
        ))
        .contains(&"symbol_value")
    );
    // Missing field.
    assert!(
        ids(&good.replace("(property \"Datasheet\"", "(property \"Other\""))
            .contains(&"symbol_fields")
    );
    // Thin body outline.
    assert!(ids(&good.replace("(width 0.254)", "(width 0.1)")).contains(&"symbol_body_stroke"));
    // Two pins on top of each other.
    assert!(
        ids(&good.replacen("(at -7.62 2.54 0)", "(at -7.62 5.08 0)", 1))
            .contains(&"symbol_pin_overlap")
    );
    // Text of the wrong size.
    assert!(ids(&good.replacen("(size 1.27 1.27)", "(size 1 1)", 1)).contains(&"symbol_text_size"));
    // Not parsable, and no symbol.
    assert!(check_symbol("(kicad_symbol_lib").is_err());
    assert!(ids("(kicad_symbol_lib (version 1))").contains(&"symbol_present"));
}
