// SPDX-License-Identifier: GPL-3.0-or-later

//! The parts that were made: the PartSpec files found in some folders, with what
//! a person looks for (name, package, pins, approval) and a drawing of the
//! footprint, so they can be browsed and opened again.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use kirpitch_core::gates::{Gate, GateStatus};
use kirpitch_core::partspec::{Origin, PartSpec};
use serde::Serialize;

use crate::footprint_svg;

/// How deep below a folder parts are looked for.
const MAX_DEPTH: usize = 3;
/// How many directory entries are looked at in all, so a huge folder cannot make
/// the list slow.
const MAX_ENTRIES: usize = 6000;
/// The most parts listed.
const MAX_PARTS: usize = 500;
/// Larger files are not PartSpecs.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// One part that was made.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartSummary {
    /// The file.
    pub path: String,
    /// The file name.
    pub file: String,
    /// The folder it is in.
    pub folder: String,
    /// Part number.
    pub mpn: String,
    /// Manufacturer.
    pub manufacturer: String,
    /// Description.
    pub description: String,
    /// Package family.
    pub family: String,
    /// Footprint name.
    pub package: String,
    /// Number of pins.
    pub pin_count: usize,
    /// Number of pads.
    pub pad_count: usize,
    /// Distance between pins, mm.
    pub pitch: f64,
    /// Body size across the rows, mm.
    pub body_width: f64,
    /// Body size along the rows, mm.
    pub body_length: f64,
    /// Largest height, mm.
    pub body_height: f64,
    /// Review gates approved.
    pub approved: usize,
    /// Review gates in all.
    pub gates: usize,
    /// Gates that were approved and then changed.
    pub stale: usize,
    /// The first gate that is not approved (`pinout`, `footprint` or `3d`);
    /// empty when all are.
    pub next_gate: String,
    /// When the file was last changed, seconds since 1970.
    pub modified: Option<u64>,
    /// The datasheet address, if the file has one.
    pub datasheet_url: Option<String>,
    /// How the values were obtained.
    pub origin: String,
    /// A drawing of the footprint.
    pub footprint_svg: String,
}

fn summary(path: &Path, spec: &PartSpec) -> PartSummary {
    let gate_status: Vec<GateStatus> = Gate::ALL.iter().map(|g| g.status(spec)).collect();
    let body = &spec.package.body;
    PartSummary {
        path: path.display().to_string(),
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        folder: path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        mpn: spec.metadata.mpn.clone(),
        manufacturer: spec.metadata.manufacturer.clone(),
        description: spec.metadata.description.clone(),
        family: spec.package.family.clone(),
        package: spec.package.name.clone(),
        pin_count: spec.pins.len(),
        pad_count: spec.land_pattern.pads.len(),
        pitch: spec.package.pitch.get(),
        body_width: body.width.get(),
        body_length: body.length.get(),
        body_height: body.height_max.get(),
        approved: gate_status
            .iter()
            .filter(|s| **s == GateStatus::Approved)
            .count(),
        gates: gate_status.len(),
        stale: gate_status
            .iter()
            .filter(|s| **s == GateStatus::Stale)
            .count(),
        next_gate: Gate::ALL
            .iter()
            .zip(&gate_status)
            .find(|(_, s)| **s != GateStatus::Approved)
            .map(|(g, _)| g.name().to_string())
            .unwrap_or_default(),
        modified: fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        datasheet_url: spec.metadata.datasheet_url.clone(),
        origin: match spec.origin {
            Origin::Hand => "typed in",
            Origin::Region => "picked from the datasheet",
            Origin::Llm => "read by a model",
        }
        .to_string(),
        footprint_svg: footprint_svg(spec),
    }
}

struct Walk {
    seen: HashSet<std::path::PathBuf>,
    entries: usize,
    parts: Vec<PartSummary>,
}

fn walk(dir: &Path, depth: usize, w: &mut Walk) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        w.entries += 1;
        if w.entries > MAX_ENTRIES || w.parts.len() >= MAX_PARTS {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let skip = name.starts_with('.')
                || matches!(name.as_str(), "target" | "node_modules" | "AppData")
                || name.starts_with('$');
            if depth < MAX_DEPTH && !skip {
                walk(&path, depth + 1, w);
            }
        } else if kind.is_file() && name.to_ascii_lowercase().ends_with(".json") {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.len() > MAX_FILE {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            // Most JSON files are not parts: a cheap look first.
            if !text.contains("\"schema_version\"") || !text.contains("\"land_pattern\"") {
                continue;
            }
            let Ok(spec) = PartSpec::from_json_str(&text) else {
                continue;
            };
            let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if w.seen.insert(key) {
                w.parts.push(summary(&path, &spec));
            }
        }
    }
}

/// The parts found in `dirs` (and the folders below them), the one changed last
/// first. Folders that do not exist are skipped.
#[must_use]
pub fn list_parts(dirs: &[String]) -> Vec<PartSummary> {
    let mut w = Walk {
        seen: HashSet::new(),
        entries: 0,
        parts: Vec::new(),
    };
    let mut done = HashSet::new();
    for d in dirs {
        let d = d.trim();
        if d.is_empty() {
            continue;
        }
        let dir = Path::new(d);
        let key = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if dir.is_dir() && done.insert(key) {
            walk(dir, 0, &mut w);
        }
    }
    let mut parts = w.parts;
    parts.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.mpn.cmp(&b.mpn)));
    parts
}

/// What is known about each of `paths` (the files that were opened last), in
/// the same order: `None` for a file that is gone or is not a part.
#[must_use]
pub fn summarize_parts(paths: &[String]) -> Vec<Option<PartSummary>> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p.trim());
            check_is_part(path).ok()?;
            let text = fs::read_to_string(path).ok()?;
            let spec = PartSpec::from_json_str(&text).ok()?;
            Some(summary(path, &spec))
        })
        .collect()
}

/// Reads `path` as a part, so that only a PartSpec file is ever renamed or
/// deleted from the screen that lists them.
fn check_is_part(path: &Path) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(format!("{} is not a part file", path.display()));
    }
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    PartSpec::from_json_str(&text)
        .map(|_| ())
        .map_err(|_| format!("{} is not a part file", path.display()))
}

/// Gives a part file another name in the same folder. `new_name` is the name
/// without the ending (`lm358`); the ending of the file (`.partspec.json`) is
/// kept. Nothing is replaced.
///
/// # Errors
///
/// The file is not a part, the name is empty or has characters a file name
/// cannot have, or a file of that name is already there.
pub fn rename_part(path: &str, new_name: &str) -> Result<String, String> {
    let from = Path::new(path.trim());
    check_is_part(from)?;
    let name = from
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ending = if name.to_ascii_lowercase().ends_with(".partspec.json") {
        ".partspec.json"
    } else {
        ".json"
    };
    let mut stem = new_name.trim().to_string();
    if stem.to_ascii_lowercase().ends_with(ending) {
        stem.truncate(stem.len() - ending.len());
    }
    let stem = stem.trim().trim_end_matches('.').to_string();
    if stem.is_empty() {
        return Err("the name is empty".into());
    }
    if stem.starts_with('.') {
        return Err("the name cannot start with a dot".into());
    }
    if let Some(bad) = stem
        .chars()
        .find(|c| c.is_control() || r#"<>:"/\|?*"#.contains(*c))
    {
        return Err(format!("the name cannot have the character {bad:?} in it"));
    }
    let to = from.with_file_name(format!("{stem}{ending}"));
    if to == from {
        return Ok(to.display().to_string());
    }
    if to.exists() {
        return Err(format!(
            "a file named {stem}{ending} is already in that folder"
        ));
    }
    fs::rename(from, &to).map_err(|e| format!("could not rename: {e}"))?;
    Ok(to.display().to_string())
}

/// Deletes a part file. Only the PartSpec file goes: KiCad files that were made
/// from it are not touched.
///
/// # Errors
///
/// The file is not a part, or it could not be deleted.
pub fn delete_part(path: &str) -> Result<(), String> {
    let p = Path::new(path.trim());
    check_is_part(p)?;
    fs::remove_file(p).map_err(|e| format!("could not delete: {e}"))
}
