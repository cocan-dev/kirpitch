// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what `kicad-cli` accepts and rejects. They need the
//! `KIRPITCH_KICAD_CLI` environment variable and are skipped without it.

use std::fs;
use std::path::Path;

use kirpitch_kicad::KicadTarget;
use kirpitch_kicad::testing::{TempDir, installed_is, kicad_cli_or_skip};
use kirpitch_kicad::{UpgradeOutcome, cli::CliError};

const TINY_MOD: &str = include_str!("fixtures/tiny.kicad_mod");
const TINY_SYM: &str = include_str!("fixtures/tiny.kicad_sym");

/// Builds `<dir>/lib.pretty/Tiny.kicad_mod` from `text`.
fn pretty(dir: &Path, text: &str) -> std::path::PathBuf {
    let lib = dir.join("lib.pretty");
    fs::create_dir_all(&lib).unwrap();
    fs::write(lib.join("Tiny.kicad_mod"), text).unwrap();
    lib
}

#[test]
fn reports_a_version() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let v = cli.version().expect("version");
    assert!(v.major >= 8, "unexpected KiCad version {v}");
}

#[test]
fn good_footprint_exports_to_svg() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("fp-good");
    let lib = pretty(tmp.path(), TINY_MOD);
    let files = cli
        .export_footprint_svg(&lib, &tmp.path().join("out"))
        .expect("export");
    assert_eq!(files.len(), 1);
}

#[test]
fn truncated_footprint_is_rejected() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("fp-trunc");
    let lib = pretty(tmp.path(), &TINY_MOD[..TINY_MOD.len() / 2]);
    let err = cli
        .export_footprint_svg(&lib, &tmp.path().join("out"))
        .unwrap_err();
    assert!(matches!(err, CliError::Failed { .. }), "{err}");
}

#[test]
fn unknown_footprint_token_is_rejected() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("fp-unknown");
    let lib = pretty(
        tmp.path(),
        &TINY_MOD.replace("(attr smd)", "(attr smd)\n\t(bogus_token 1)"),
    );
    let err = cli
        .export_footprint_svg(&lib, &tmp.path().join("out"))
        .unwrap_err();
    assert!(matches!(err, CliError::Failed { .. }), "{err}");
}

#[test]
fn footprint_in_current_format_is_not_upgraded() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("fp-upgrade");
    let lib = pretty(tmp.path(), TINY_MOD);
    let outcome = cli.upgrade_footprints(&lib).expect("upgrade");
    // The fixture uses the KiCad 10 format; older tools may rewrite it.
    if cli.version().unwrap().major >= 10 {
        assert_eq!(outcome, UpgradeOutcome::Unchanged);
        assert_eq!(
            fs::read_to_string(lib.join("Tiny.kicad_mod")).unwrap(),
            TINY_MOD
        );
    }
}

#[test]
fn good_symbol_exports_to_svg() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    // The fixture is written in the KiCad 10 format.
    if !installed_is(&cli, KicadTarget::V10) {
        return;
    }
    let tmp = TempDir::new("sym-good");
    let file = tmp.path().join("tiny.kicad_sym");
    fs::write(&file, TINY_SYM).unwrap();
    let files = cli
        .export_symbol_svg(&file, &tmp.path().join("out"))
        .expect("export");
    assert_eq!(files.len(), 1);
}

#[test]
fn truncated_symbol_is_rejected() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("sym-trunc");
    let file = tmp.path().join("tiny.kicad_sym");
    fs::write(&file, &TINY_SYM[..TINY_SYM.len() / 2]).unwrap();
    let err = cli
        .export_symbol_svg(&file, &tmp.path().join("out"))
        .unwrap_err();
    assert!(matches!(err, CliError::Failed { .. }), "{err}");
}

#[test]
fn symbol_in_current_format_is_not_upgraded() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    if !installed_is(&cli, KicadTarget::V10) {
        return;
    }
    let tmp = TempDir::new("sym-upgrade");
    let file = tmp.path().join("tiny.kicad_sym");
    fs::write(&file, TINY_SYM).unwrap();
    let outcome = cli.upgrade_symbols(&file).expect("upgrade");
    if cli.version().unwrap().major >= 10 {
        assert_eq!(outcome, UpgradeOutcome::Unchanged);
    }
}

#[test]
fn old_symbol_version_is_rewritten_in_kicads_own_layout() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    if cli.version().unwrap().major < 10 {
        return;
    }
    let tmp = TempDir::new("sym-old");
    let file = tmp.path().join("tiny.kicad_sym");
    fs::write(
        &file,
        TINY_SYM.replace("(version 20251024)", "(version 20240101)"),
    )
    .unwrap();
    assert_eq!(cli.upgrade_symbols(&file).unwrap(), UpgradeOutcome::Updated);
    // KiCad restores its own layout and adds the defaults it writes itself, so
    // a file written exactly like KiCad would come back identical, except for
    // the generator name.
    let rewritten = fs::read_to_string(&file)
        .unwrap()
        .replace("\r\n", "\n")
        .replace("kicad_symbol_editor", "kirpitch-test");
    assert_eq!(rewritten, TINY_SYM);
}

#[test]
fn missing_input_is_an_error_not_a_pass() {
    let Some(cli) = kicad_cli_or_skip() else {
        return;
    };
    let tmp = TempDir::new("missing");
    let err = cli
        .export_symbol_svg(
            &tmp.path().join("nothing.kicad_sym"),
            &tmp.path().join("out"),
        )
        .unwrap_err();
    assert!(
        matches!(err, CliError::Failed { .. } | CliError::Unexpected(_)),
        "{err}"
    );
}
