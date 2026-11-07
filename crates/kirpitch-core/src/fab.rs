// SPDX-License-Identifier: GPL-3.0-or-later

//! Fab (fabrication) profiles: the limits of a printed circuit board maker.
//!
//! There is a built-in generic profile with conservative values (ADR 0003), a
//! preset for JLCPCB (ADR 0032), and profiles that the user imports from a
//! KiCad `.kicad_dru` or an Eagle `.dru` file (see [`crate::fab_import`]) or
//! writes as JSON.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::units::Mm;

/// Manufacturing limits used by the cross-checks and by the footprint generator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct FabProfile {
    /// Name of the profile.
    pub name: String,
    /// Smallest allowed gap between two copper features of different nets.
    pub min_copper_gap: Mm,
    /// Smallest allowed solder mask web between two mask openings.
    pub min_mask_web: Mm,
    /// Smallest allowed silkscreen line width.
    pub min_silk_width: Mm,
    /// Smallest allowed silkscreen text height.
    pub min_silk_text_height: Mm,
    /// Smallest allowed size (the shorter side) of a surface-mount pad.
    pub min_pad_size: Mm,
    /// Smallest allowed drill diameter.
    pub min_drill: Mm,
    /// Smallest allowed annular ring: the copper left around a drilled hole.
    pub min_annular_ring: Mm,
    /// Smallest allowed distance between the edges of two drilled holes.
    pub min_hole_to_hole: Mm,
    /// Smallest allowed distance between silkscreen and copper.
    pub min_silk_pad_gap: Mm,
}

impl FabProfile {
    /// The built-in generic profile.
    ///
    /// The values are chosen to be safe for typical low-cost two-layer
    /// services, not to match any single maker. They are listed in
    /// `docs/requirements.md`.
    pub fn generic() -> FabProfile {
        FabProfile {
            name: "generic".to_string(),
            min_copper_gap: mm(0.20),
            min_mask_web: mm(0.10),
            min_silk_width: mm(0.12),
            min_silk_text_height: mm(0.80),
            min_pad_size: mm(0.20),
            min_drill: mm(0.30),
            min_annular_ring: mm(0.15),
            min_hole_to_hole: mm(0.25),
            min_silk_pad_gap: mm(0.20),
        }
    }

    /// The JLCPCB preset: standard two-layer boards with 1 oz copper, the
    /// numbers of the "PCB Manufacturing & Assembly Capabilities" page of
    /// jlcpcb.com as read on 7 November 2026. The page lists more than one
    /// value for some limits; where it does, the one for ordinary two-layer
    /// boards is used. Check the page for your own order (multilayer boards,
    /// 2 oz copper and black or white mask have other limits).
    pub fn jlcpcb() -> FabProfile {
        FabProfile {
            name: "JLCPCB (2 layers, 1 oz)".to_string(),
            // SMD pad to SMD pad of different nets.
            min_copper_gap: mm(0.15),
            // Green, red, yellow, blue and purple mask; black and white need 0.13.
            min_mask_web: mm(0.10),
            min_silk_width: mm(0.15),
            min_silk_text_height: mm(1.0),
            // Smallest SMD pad.
            min_pad_size: mm(0.25),
            min_drill: mm(0.15),
            // The least for a plated hole on a two-layer 1 oz board.
            min_annular_ring: mm(0.18),
            // Between the holes of two pads.
            min_hole_to_hole: mm(0.45),
            min_silk_pad_gap: mm(0.15),
        }
    }

    /// The built-in profiles as (identifier, profile). The identifier is what
    /// the settings store, written `preset:<identifier>`.
    pub fn presets() -> Vec<(&'static str, FabProfile)> {
        vec![
            ("generic", FabProfile::generic()),
            ("jlcpcb", FabProfile::jlcpcb()),
        ]
    }

    /// The profile a setting names: empty for the generic one,
    /// `preset:<identifier>` for a built-in one, otherwise the path of a JSON
    /// file.
    pub fn resolve(setting: &str) -> Result<FabProfile, String> {
        let setting = setting.trim();
        if setting.is_empty() {
            return Ok(FabProfile::generic());
        }
        match setting.strip_prefix("preset:") {
            Some(id) => FabProfile::presets()
                .into_iter()
                .find(|(i, _)| *i == id)
                .map(|(_, p)| p)
                .ok_or_else(|| format!("there is no built-in fab profile `{id}`")),
            None => FabProfile::from_file(std::path::Path::new(setting)),
        }
    }

    /// The limits by field name, label and value, in the order a form shows
    /// them.
    pub fn limits(&self) -> Vec<(&'static str, &'static str, f64)> {
        vec![
            (
                "min_copper_gap",
                "Copper to copper (pad to pad)",
                self.min_copper_gap.get(),
            ),
            ("min_pad_size", "Smallest SMD pad", self.min_pad_size.get()),
            ("min_drill", "Smallest drill", self.min_drill.get()),
            (
                "min_annular_ring",
                "Annular ring",
                self.min_annular_ring.get(),
            ),
            (
                "min_hole_to_hole",
                "Hole to hole",
                self.min_hole_to_hole.get(),
            ),
            ("min_mask_web", "Solder mask web", self.min_mask_web.get()),
            (
                "min_silk_width",
                "Silkscreen line width",
                self.min_silk_width.get(),
            ),
            (
                "min_silk_text_height",
                "Silkscreen text height",
                self.min_silk_text_height.get(),
            ),
            (
                "min_silk_pad_gap",
                "Silkscreen to copper",
                self.min_silk_pad_gap.get(),
            ),
        ]
    }
}

/// Name of the environment variable that holds the path of a fab profile file.
pub const ENV_FAB_PROFILE: &str = "KIRPITCH_FAB_PROFILE";

impl FabProfile {
    /// Reads a profile from JSON text. Values that are left out keep the values
    /// of the generic profile, so a file can set just what a maker needs, for
    /// example `{"name": "fine", "min_copper_gap": 0.1}`. Unknown fields are
    /// errors, and every length must be positive.
    pub fn from_json_str(text: &str) -> Result<FabProfile, String> {
        let profile: FabProfile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        profile.check()?;
        Ok(profile)
    }

    /// Every length must be greater than zero.
    pub fn check(&self) -> Result<(), String> {
        for (name, _, value) in self.limits() {
            if value <= 0.0 {
                return Err(format!("`{name}` must be greater than 0"));
            }
        }
        Ok(())
    }

    /// The profile as JSON text, as `from_json_str` reads it.
    pub fn to_json_string(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a profile is plain data");
        text.push('\n');
        text
    }

    /// Reads a profile from a JSON file.
    pub fn from_file(path: &std::path::Path) -> Result<FabProfile, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        FabProfile::from_json_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The profile named by `KIRPITCH_FAB_PROFILE`, or the generic one when the
    /// variable is not set.
    pub fn from_env() -> Result<FabProfile, String> {
        match std::env::var_os(ENV_FAB_PROFILE) {
            Some(path) if !path.is_empty() => FabProfile::from_file(std::path::Path::new(&path)),
            _ => Ok(FabProfile::generic()),
        }
    }
}

impl Default for FabProfile {
    fn default() -> FabProfile {
        FabProfile::generic()
    }
}

fn mm(value: f64) -> Mm {
    Mm::new(value).expect("constant is finite")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_file_overrides_only_what_it_names() {
        let p = FabProfile::from_json_str(r#"{"name": "fine", "min_copper_gap": 0.1}"#).unwrap();
        assert_eq!(p.name, "fine");
        assert_eq!(p.min_copper_gap.format(), "0.1");
        assert_eq!(p.min_mask_web.format(), "0.1");
        assert_eq!(p.min_silk_width.format(), "0.12");
        assert_eq!(p.min_drill.format(), "0.3");
    }

    #[test]
    fn bad_profiles_are_refused() {
        assert!(
            FabProfile::from_json_str(r#"{"min_copper_gap": 0}"#)
                .unwrap_err()
                .contains("min_copper_gap")
        );
        assert!(FabProfile::from_json_str(r#"{"min_gap": 0.1}"#).is_err());
        assert!(FabProfile::from_json_str("not json").is_err());
        assert!(FabProfile::from_file(std::path::Path::new("no/such/file.json")).is_err());
    }

    #[test]
    fn generic_values_match_the_documentation() {
        let p = FabProfile::generic();
        assert_eq!(p.min_copper_gap.format(), "0.2");
        assert_eq!(p.min_mask_web.format(), "0.1");
        assert_eq!(p.min_silk_width.format(), "0.12");
        assert_eq!(p.min_silk_text_height.format(), "0.8");
        assert_eq!(p.min_pad_size.format(), "0.2");
        assert_eq!(p.min_drill.format(), "0.3");
        assert_eq!(p.min_annular_ring.format(), "0.15");
        assert_eq!(p.min_hole_to_hole.format(), "0.25");
        assert_eq!(p.min_silk_pad_gap.format(), "0.2");
    }

    #[test]
    fn presets_are_valid_and_found_by_their_setting() {
        for (id, p) in FabProfile::presets() {
            p.check().unwrap();
            assert_eq!(FabProfile::resolve(&format!("preset:{id}")).unwrap(), p);
            // What is written can be read back.
            assert_eq!(FabProfile::from_json_str(&p.to_json_string()).unwrap(), p);
        }
        assert_eq!(FabProfile::resolve("").unwrap(), FabProfile::generic());
        assert!(FabProfile::resolve("preset:nobody").is_err());
        assert_eq!(FabProfile::jlcpcb().min_copper_gap.format(), "0.15");
        assert_eq!(FabProfile::jlcpcb().min_silk_text_height.format(), "1");
    }
}
