// SPDX-License-Identifier: GPL-3.0-or-later

//! The fab rules of the settings screen: the built-in profiles, the ones the
//! user imported or wrote, and reading a rules file that a maker or a designer
//! exported (ADR 0032).
//!
//! Saved profiles are JSON files in a `fab` folder next to the settings. The
//! choice itself is the `fab_profile` setting: empty for the generic limits,
//! `preset:<name>` for a built-in profile, or the path of a file.

use std::fs;
use std::path::{Path, PathBuf};

use kirpitch_core::FabProfile;
use kirpitch_core::fab_import;
use kirpitch_models::active_config_path;
use serde::{Deserialize, Serialize};

/// One limit, for the form.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FabLimitView {
    /// Field name in the profile file.
    pub key: String,
    /// What the screen calls it.
    pub label: String,
    /// Millimetres.
    pub value: f64,
}

/// One profile that can be chosen.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FabEntry {
    /// What is stored in the setting: `preset:<name>` or a path; empty for the
    /// generic limits.
    pub setting: String,
    /// Name of the profile.
    pub name: String,
    /// Built in (cannot be changed or deleted).
    pub builtin: bool,
    /// The limits.
    pub limits: Vec<FabLimitView>,
}

/// What the fab rules section shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FabRulesView {
    /// Every profile that can be chosen: built in first, then saved ones.
    pub entries: Vec<FabEntry>,
    /// The folder where saved profiles are kept.
    pub folder: String,
    /// Problems with files in that folder.
    pub problems: Vec<String>,
}

/// A rules file that was read, before it is saved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FabImportView {
    /// `kicad`, `eagle` or `json`.
    pub format: String,
    /// The profile read (a name taken from the file name).
    pub name: String,
    /// The limits, with the generic values where the file has none.
    pub limits: Vec<FabLimitView>,
    /// Which limits came from the file.
    pub used: Vec<String>,
    /// What was not used, and why.
    pub skipped: Vec<String>,
}

/// A profile as the form sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct FabSaveInput {
    /// Name of the profile; it also names the file.
    pub name: String,
    /// The limits by field name, in millimetres.
    pub limits: std::collections::BTreeMap<String, f64>,
}

fn limit_views(p: &FabProfile) -> Vec<FabLimitView> {
    p.limits()
        .into_iter()
        .map(|(key, label, value)| FabLimitView {
            key: key.to_string(),
            label: label.to_string(),
            value: (value * 1e6).round() / 1e6,
        })
        .collect()
}

/// Where saved profiles are kept.
pub fn fab_folder() -> PathBuf {
    active_config_path()
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("fab"), |d| d.join("fab"))
}

fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "profile".to_string()
    } else {
        s
    }
}

/// Lists the built-in profiles and the saved ones in `folder`. A profile that
/// the setting names but that is somewhere else is listed too, so the screen
/// can show what is in use.
pub fn view_in(folder: &Path, current: &str) -> FabRulesView {
    let mut entries: Vec<FabEntry> = FabProfile::presets()
        .into_iter()
        .map(|(id, p)| FabEntry {
            setting: if id == "generic" {
                String::new()
            } else {
                format!("preset:{id}")
            },
            name: p.name.clone(),
            builtin: true,
            limits: limit_views(&p),
        })
        .collect();
    let mut problems = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let current = current.trim();
    if !current.is_empty()
        && !current.starts_with("preset:")
        && !files.iter().any(|f| f.as_path() == Path::new(current))
    {
        files.push(PathBuf::from(current));
    }
    for file in files {
        match FabProfile::from_file(&file) {
            Ok(p) => entries.push(FabEntry {
                setting: file.display().to_string(),
                name: p.name.clone(),
                builtin: false,
                limits: limit_views(&p),
            }),
            Err(e) => problems.push(e),
        }
    }
    FabRulesView {
        entries,
        folder: folder.display().to_string(),
        problems,
    }
}

/// The fab rules section for the settings now.
pub fn fab_rules(current: &str) -> FabRulesView {
    view_in(&fab_folder(), current)
}

/// Reads a rules file (KiCad `.kicad_dru`, Eagle `.dru` or a JSON profile)
/// without saving anything.
pub fn import_file(path: &Path) -> Result<FabImportView, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("rules");
    let imported = fab_import::import(name, &text).map_err(|e| format!("{name}: {e}"))?;
    let format = fab_import::detect(name, &text).unwrap_or("json");
    let mut profile = imported.profile;
    if profile.name == "generic" {
        profile.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported")
            .to_string();
    }
    Ok(FabImportView {
        format: format.to_string(),
        name: profile.name.clone(),
        limits: limit_views(&profile),
        used: imported.used,
        skipped: imported.skipped,
    })
}

/// Builds a profile from the form: the generic limits with the given ones set.
fn profile_from_input(input: &FabSaveInput) -> Result<FabProfile, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("give the fab profile a name".to_string());
    }
    let mut json = serde_json::to_value(FabProfile::generic()).map_err(|e| e.to_string())?;
    let object = json.as_object_mut().expect("a profile is an object");
    for (key, value) in &input.limits {
        if !object.contains_key(key) {
            return Err(format!("`{key}` is not a fab limit"));
        }
        object.insert(key.clone(), serde_json::json!(value));
    }
    object.insert("name".to_string(), serde_json::json!(name));
    FabProfile::from_json_str(&json.to_string())
}

/// Saves a profile in `folder` under its name (replacing the one of that name)
/// and returns the path, which is what the setting stores.
pub fn save_in(folder: &Path, input: &FabSaveInput) -> Result<String, String> {
    let profile = profile_from_input(input)?;
    fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("{}.json", slug(&profile.name)));
    fs::write(&path, profile.to_json_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.display().to_string())
}

/// Saves a profile in the fab folder.
pub fn save_fab_profile(input: &FabSaveInput) -> Result<String, String> {
    save_in(&fab_folder(), input)
}

/// Deletes a saved profile; only files in `folder` can be deleted.
pub fn delete_in(folder: &Path, file: &Path) -> Result<(), String> {
    let inside = match (fs::canonicalize(folder), fs::canonicalize(file)) {
        (Ok(f), Ok(p)) => p.parent() == Some(f.as_path()),
        _ => false,
    };
    if !inside || file.extension().is_none_or(|e| e != "json") {
        return Err("only profiles saved by Kirpitch can be deleted here".to_string());
    }
    fs::remove_file(file).map_err(|e| format!("{}: {e}", file.display()))
}

/// Deletes a saved profile from the fab folder.
pub fn delete_fab_profile(file: &str) -> Result<(), String> {
    delete_in(&fab_folder(), Path::new(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kirpitch-fab-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn input(name: &str, gap: f64) -> FabSaveInput {
        FabSaveInput {
            name: name.to_string(),
            limits: [("min_copper_gap".to_string(), gap)].into(),
        }
    }

    #[test]
    fn built_in_profiles_come_first_and_saved_ones_follow() {
        let dir = temp("list");
        let v = view_in(&dir, "");
        assert_eq!(v.entries.len(), 2);
        assert!(v.entries.iter().all(|e| e.builtin));
        assert_eq!(v.entries[0].setting, "");
        assert_eq!(v.entries[1].setting, "preset:jlcpcb");

        let path = save_in(&dir, &input("My maker", 0.12)).unwrap();
        assert!(path.ends_with("My-maker.json"));
        let v = view_in(&dir, &path);
        assert_eq!(v.entries.len(), 3, "{v:?}");
        let mine = &v.entries[2];
        assert_eq!(mine.name, "My maker");
        assert_eq!(mine.setting, path);
        assert!(!mine.builtin);
        assert_eq!(mine.limits[0].key, "min_copper_gap");
        assert_eq!(mine.limits[0].value, 0.12);
        // The setting resolves to the same profile.
        assert_eq!(FabProfile::resolve(&path).unwrap().name, "My maker");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_profile_outside_the_folder_is_listed_and_a_broken_file_is_a_problem() {
        let dir = temp("outside");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("bad.json"), r#"{"min_copper_gap": 0}"#).unwrap();
        let elsewhere = temp("elsewhere");
        let path = save_in(&elsewhere, &input("Far away", 0.1)).unwrap();
        let v = view_in(&dir, &path);
        assert_eq!(v.entries.len(), 3);
        assert_eq!(v.problems.len(), 1);
        assert!(v.problems[0].contains("min_copper_gap"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&elsewhere);
    }

    #[test]
    fn the_form_is_checked() {
        let dir = temp("form");
        assert!(save_in(&dir, &input("  ", 0.1)).is_err());
        assert!(save_in(&dir, &input("zero", 0.0)).is_err());
        let mut bad = input("typo", 0.1);
        bad.limits.insert("min_gap".to_string(), 0.1);
        assert!(save_in(&dir, &bad).unwrap_err().contains("min_gap"));
        assert!(!dir.exists() || fs::read_dir(&dir).unwrap().next().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rules_file_is_read_without_saving_and_can_then_be_saved() {
        let dir = temp("import");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("PCBWay.kicad_dru");
        fs::write(
            &file,
            "(version 1)\n(rule \"holes\" (constraint hole_size (min 0.2mm)))\n(rule \"gap\" (constraint clearance (min 4mil)))\n",
        )
        .unwrap();
        let r = import_file(&file).unwrap();
        assert_eq!(r.format, "kicad");
        assert_eq!(r.name, "PCBWay");
        let value = |key: &str| r.limits.iter().find(|l| l.key == key).unwrap().value;
        assert_eq!(value("min_drill"), 0.2);
        assert_eq!(value("min_copper_gap"), 0.1016);
        assert_eq!(r.used.len(), 2);
        // Nothing was saved by reading.
        assert!(!dir.join("fab").exists());
        // The form sends back what the user left in it.
        let saved = save_in(
            &dir.join("fab"),
            &FabSaveInput {
                name: r.name.clone(),
                limits: r.limits.iter().map(|l| (l.key.clone(), l.value)).collect(),
            },
        )
        .unwrap();
        let p = FabProfile::resolve(&saved).unwrap();
        assert_eq!(p.name, "PCBWay");
        assert_eq!(p.min_drill.format(), "0.2");
        assert!(import_file(&dir.join("missing.kicad_dru")).is_err());
        fs::write(dir.join("junk.txt"), "hello").unwrap();
        assert!(import_file(&dir.join("junk.txt")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_saved_profiles_can_be_deleted() {
        let dir = temp("delete");
        let path = save_in(&dir, &input("Gone", 0.1)).unwrap();
        let outside = temp("delete-outside");
        let other = save_in(&outside, &input("Kept", 0.1)).unwrap();
        assert!(delete_in(&dir, Path::new(&other)).is_err());
        assert!(Path::new(&other).exists());
        delete_in(&dir, Path::new(&path)).unwrap();
        assert!(!Path::new(&path).exists());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }
}
