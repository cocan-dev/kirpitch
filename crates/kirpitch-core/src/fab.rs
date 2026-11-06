// SPDX-License-Identifier: GPL-3.0-or-later

//! Fab (fabrication) profiles: the limits of a printed circuit board maker.
//!
//! Milestone 1 has one built-in generic profile with conservative values
//! (ADR 0003). Presets for specific makers are planned later.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::units::Mm;

/// Manufacturing limits used by the cross-checks.
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
        }
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
        for (name, value) in [
            ("min_copper_gap", profile.min_copper_gap),
            ("min_mask_web", profile.min_mask_web),
            ("min_silk_width", profile.min_silk_width),
            ("min_silk_text_height", profile.min_silk_text_height),
        ] {
            if value.get() <= 0.0 {
                return Err(format!("`{name}` must be greater than 0"));
            }
        }
        Ok(profile)
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
    }
}
