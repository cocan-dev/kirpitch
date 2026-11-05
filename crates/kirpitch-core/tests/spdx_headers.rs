// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks that every Rust source file in the workspace starts with the SPDX
//! (Software Package Data Exchange) licence identifier required by
//! CONTRIBUTING.md.

use std::fs;
use std::path::{Path, PathBuf};

const HEADER: &str = "// SPDX-License-Identifier: GPL-3.0-or-later";

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_rust_file_has_spdx_header() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in ["crates", "xtask"] {
        collect(&root.join(dir), &mut files);
    }
    assert!(!files.is_empty(), "no Rust files found");
    let missing: Vec<_> = files
        .iter()
        .filter(|f| {
            fs::read_to_string(f)
                .map(|s| s.lines().next() != Some(HEADER))
                .unwrap_or(true)
        })
        .collect();
    assert!(missing.is_empty(), "missing SPDX header: {missing:?}");
}
