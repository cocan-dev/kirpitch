// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the datasheet of a part is on this computer.
//!
//! A part file keeps the SHA-256 hash of the datasheet it was made from, not
//! where that PDF is (the path is only true on one computer, and a part file is
//! meant to be shared). So the app keeps a small list of its own, next to the
//! settings: the hash of each datasheet it has seen and the path it was at. A
//! part is then found its datasheet by hash, whatever the part file is called
//! and wherever it is.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kirpitch_models::active_config_path;
use sha2::{Digest, Sha256};

/// The most PDFs looked at, and the largest one, when a datasheet is searched for.
const MAX_SCAN: usize = 40;
const MAX_PDF: u64 = 200 * 1024 * 1024;

fn hash_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(
        Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn read_index(index: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(index)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Writes `sha` -> `pdf` into the list at `index`.
fn remember_in(index: &Path, sha: &str, pdf: &Path) -> Result<(), String> {
    let mut map = read_index(index);
    let path = fs::canonicalize(pdf).unwrap_or_else(|_| pdf.to_path_buf());
    let path = path.display().to_string();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path).to_string();
    if map.get(sha) == Some(&path) {
        return Ok(());
    }
    map.insert(sha.to_string(), path);
    if let Some(dir) = index.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    fs::write(index, text).map_err(|e| format!("{}: {e}", index.display()))
}

/// The PDF of `sha`: the remembered one if it is still there, else one found
/// by hash in `near` (folders to look in, such as the part's own).
fn find_in(index: &Path, sha: &str, near: &[PathBuf]) -> Option<String> {
    let map = read_index(index);
    if let Some(p) = map.get(sha).filter(|p| Path::new(p).is_file()) {
        return Some(p.clone());
    }
    // Not remembered, or moved: look in the folders where datasheets were, and near the part.
    let mut dirs: Vec<PathBuf> = near.to_vec();
    for p in map.values() {
        if let Some(d) = Path::new(p).parent() {
            dirs.push(d.to_path_buf());
        }
    }
    let mut seen = Vec::new();
    let mut looked = 0;
    for dir in dirs {
        if seen.contains(&dir) {
            continue;
        }
        seen.push(dir.clone());
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let is_pdf = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            if !is_pdf || entry.metadata().map_or(true, |m| m.len() > MAX_PDF) {
                continue;
            }
            looked += 1;
            if looked > MAX_SCAN {
                return None;
            }
            if hash_file(&path).as_deref() == Some(sha) {
                let _ = remember_in(index, sha, &path);
                return Some(path.display().to_string());
            }
        }
    }
    None
}

fn index_path() -> PathBuf {
    active_config_path().with_file_name("datasheets.json")
}

/// Remembers where the datasheet `pdf` is, so that the parts made from it can
/// show it again.
///
/// # Errors
///
/// The PDF cannot be read or the list cannot be written.
pub fn remember_datasheet(pdf: &str) -> Result<(), String> {
    let path = Path::new(pdf.trim());
    let sha = hash_file(path).ok_or_else(|| format!("{}: cannot be read", path.display()))?;
    remember_in(&index_path(), &sha, path)
}

/// The datasheet PDF of the part file `spec_path` on this computer, if it is
/// known or can be found next to the part or where its siblings were.
pub fn datasheet_for(spec_path: &str) -> Option<String> {
    let spec_path = Path::new(spec_path.trim());
    let spec = crate::load_spec(spec_path).ok()?;
    let sha = spec.metadata.datasheet_sha256?;
    let near: Vec<PathBuf> = spec_path
        .parent()
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_default();
    find_in(&index_path(), &sha, &near)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kirpitch-ds-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_datasheet_is_found_by_its_hash() {
        let dir = scratch("found");
        let index = dir.join("datasheets.json");
        let pdf = dir.join("a.pdf");
        fs::write(&pdf, b"%PDF-1.4 one").unwrap();
        let sha = hash_file(&pdf).unwrap();
        // Unknown at first.
        assert_eq!(find_in(&index, &sha, &[]), None);
        remember_in(&index, &sha, &pdf).unwrap();
        let found = find_in(&index, &sha, &[]).unwrap();
        assert!(found.ends_with("a.pdf"));
        // Moved to another name in the same folder: found again by hash.
        let moved = dir.join("b.pdf");
        fs::rename(&pdf, &moved).unwrap();
        let found = find_in(&index, &sha, &[]).unwrap();
        assert!(found.ends_with("b.pdf"));
        // Another PDF is not taken for it.
        fs::write(dir.join("c.pdf"), b"%PDF-1.4 two").unwrap();
        fs::remove_file(&moved).unwrap();
        assert_eq!(find_in(&index, &sha, &[]), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_datasheet_is_found_near_the_part_without_being_remembered() {
        let dir = scratch("near");
        let index = dir.join("none").join("datasheets.json");
        let pdf = dir.join("ds.pdf");
        fs::write(&pdf, b"%PDF-1.4 near").unwrap();
        let sha = hash_file(&pdf).unwrap();
        let found = find_in(&index, &sha, std::slice::from_ref(&dir)).unwrap();
        assert!(found.ends_with("ds.pdf"));
        // And it is remembered from then on.
        assert!(read_index(&index).contains_key(&sha));
        let _ = fs::remove_dir_all(&dir);
    }
}
