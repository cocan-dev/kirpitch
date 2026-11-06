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
#[serde(deny_unknown_fields)]
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
    fn generic_values_match_the_documentation() {
        let p = FabProfile::generic();
        assert_eq!(p.min_copper_gap.format(), "0.2");
        assert_eq!(p.min_mask_web.format(), "0.1");
        assert_eq!(p.min_silk_width.format(), "0.12");
        assert_eq!(p.min_silk_text_height.format(), "0.8");
    }
}
