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
