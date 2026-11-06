// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers for tests that use `kicad-cli`. Not meant for production code.
//!
//! Tests that need `kicad-cli` call [`kicad_cli_or_skip`]. Without the
//! `KIRPITCH_KICAD_CLI` environment variable they print a message and are
//! skipped, unless `KIRPITCH_REQUIRE_KICAD=1`, in which case they fail (ADR 0007).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::cli::{ENV_KICAD_CLI, KicadCli};

/// Name of the environment variable that turns a missing `kicad-cli` into a
/// test failure.
pub const ENV_REQUIRE_KICAD: &str = "KIRPITCH_REQUIRE_KICAD";

/// Returns the `kicad-cli` to test with, or `None` after printing why the test
/// is skipped. Panics when `KIRPITCH_REQUIRE_KICAD=1` and no tool is set.
pub fn kicad_cli_or_skip() -> Option<KicadCli> {
    match KicadCli::from_env() {
        Some(cli) => Some(cli),
        None => {
            let required = std::env::var(ENV_REQUIRE_KICAD).is_ok_and(|v| v == "1");
            assert!(
                !required,
                "{ENV_REQUIRE_KICAD}=1 but {ENV_KICAD_CLI} is not set"
            );
            eprintln!(
                "SKIPPED: set {ENV_KICAD_CLI} to the path of kicad-cli to run this test \
                 (for example C:\\Program Files\\KiCad\\10.0\\bin\\kicad-cli.exe)"
            );
            None
        }
    }
}

/// A temporary folder that is removed when dropped.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

impl TempDir {
    /// Creates a new empty folder in the system temporary folder.
    pub fn new(label: &str) -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("kirpitch-{label}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    /// The folder path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Name of the environment variable that rewrites snapshot files.
pub const ENV_UPDATE_SNAPSHOTS: &str = "KIRPITCH_UPDATE_SNAPSHOTS";

/// Compares `actual` with the text in the snapshot file at `path`.
///
/// With `KIRPITCH_UPDATE_SNAPSHOTS=1` the file is written instead. Line endings
/// of the stored file are ignored so a checkout with CRLF does not matter.
pub fn assert_snapshot(path: &Path, actual: &str) {
    if std::env::var(ENV_UPDATE_SNAPSHOTS).is_ok_and(|v| v == "1") {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create snapshot folder");
        }
        std::fs::write(path, actual).expect("write snapshot");
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "cannot read snapshot {}: {e}; run with {ENV_UPDATE_SNAPSHOTS}=1 to create it",
            path.display()
        )
    });
    assert!(
        expected.replace("\r\n", "\n") == actual,
        "snapshot {} differs; review the change and run with {ENV_UPDATE_SNAPSHOTS}=1 to accept it",
        path.display()
    );
}

/// Runs the load check of ADR 0012 on a generated symbol library (KiCad 10
/// format) and panics with a readable message when it fails.
///
/// 1. `kicad-cli sym export svg` succeeds and writes an SVG per unit.
/// 2. The text parses and prints back unchanged, with the target's version.
/// 3. `sym upgrade` leaves the file alone.
/// 4. With an old version number, `sym upgrade` rewrites the file to exactly
///    the same text, except for the generator name `kirpitch`.
pub fn assert_symbol_loads(cli: &KicadCli, label: &str, text: &str, file_name: &str) {
    use crate::{KicadTarget, UpgradeOutcome};
    let tmp = TempDir::new(label);
    let file = tmp.path().join(file_name);
    std::fs::write(&file, text).expect("write symbol");
    let svgs = cli
        .export_symbol_svg(&file, &tmp.path().join("svg"))
        .unwrap_or_else(|e| panic!("kicad-cli cannot load the symbol: {e}"));
    assert!(!svgs.is_empty(), "no SVG written");
    crate::sexpr::check_round_trip(text).expect("own parse and round trip");
    let version = KicadTarget::LATEST.symbol_format_version();
    assert!(text.contains(&format!("(version {version})")));
    if cli.version().expect("kicad-cli version").major >= KicadTarget::LATEST.major() {
        assert_eq!(
            cli.upgrade_symbols(&file).expect("upgrade"),
            UpgradeOutcome::Unchanged
        );
        let old = text.replace(&format!("(version {version})"), "(version 20240101)");
        std::fs::write(&file, old).expect("write old symbol");
        assert_eq!(
            cli.upgrade_symbols(&file).expect("upgrade"),
            UpgradeOutcome::Updated
        );
        let rewritten = std::fs::read_to_string(&file)
            .expect("read symbol")
            .replace("\r\n", "\n")
            .replace("kicad_symbol_editor", "kirpitch");
        if rewritten != text {
            let diff: Vec<String> = rewritten
                .lines()
                .zip(text.lines())
                .enumerate()
                .filter(|(_, (x, y))| x != y)
                .take(5)
                .map(|(i, (x, y))| format!("line {}: KiCad `{x}` vs ours `{y}`", i + 1))
                .collect();
            panic!("KiCad rewrites the file differently:\n{}", diff.join("\n"));
        }
    }
}

/// Runs the load check of ADR 0012 on a generated footprint and panics with a
/// readable message when it fails.
///
/// 1. `kicad-cli fp export svg` succeeds on a `.pretty` folder holding it.
/// 2. The text parses and prints back unchanged, with the target's version.
/// 3. `fp upgrade` leaves the file alone. (An old-version rewrite is not used
///    for footprints because KiCad adds random UUIDs, see ADR 0012.)
pub fn assert_footprint_loads(cli: &KicadCli, label: &str, text: &str, file_name: &str) {
    use crate::{KicadTarget, UpgradeOutcome};
    let tmp = TempDir::new(label);
    let lib = tmp.path().join("lib.pretty");
    std::fs::create_dir_all(&lib).expect("create library folder");
    std::fs::write(lib.join(file_name), text).expect("write footprint");
    let svgs = cli
        .export_footprint_svg(&lib, &tmp.path().join("svg"))
        .unwrap_or_else(|e| panic!("kicad-cli cannot load the footprint: {e}"));
    assert_eq!(svgs.len(), 1, "expected one SVG");
    crate::sexpr::check_round_trip(text).expect("own parse and round trip");
    let version = KicadTarget::LATEST.footprint_format_version();
    assert!(text.contains(&format!("(version {version})")));
    if cli.version().expect("kicad-cli version").major >= KicadTarget::LATEST.major() {
        assert_eq!(
            cli.upgrade_footprints(&lib).expect("upgrade"),
            UpgradeOutcome::Unchanged
        );
        let after = std::fs::read_to_string(lib.join(file_name)).expect("read footprint");
        assert_eq!(after.replace("\r\n", "\n"), text);
    }
}
