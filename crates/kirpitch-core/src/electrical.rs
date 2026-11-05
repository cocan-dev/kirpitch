// SPDX-License-Identifier: GPL-3.0-or-later

//! Electrical types of pins.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The electrical type of a pin. These map one to one onto KiCad pin types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ElectricalType {
    /// Signal input.
    Input,
    /// Signal output.
    Output,
    /// Input and output.
    Bidirectional,
    /// Output that can be switched to high impedance.
    TriState,
    /// Passive pin such as one side of a resistor.
    Passive,
    /// Pin that is not connected to anything inside the part and may be free.
    Free,
    /// Type not known.
    Unspecified,
    /// Power supply input.
    PowerIn,
    /// Power supply output, such as a regulator output.
    PowerOut,
    /// Open collector or open drain output.
    OpenCollector,
    /// Open emitter or open source output.
    OpenEmitter,
    /// Pin that must not be connected.
    NoConnect,
}

impl ElectricalType {
    /// The snake_case name used in PartSpec files.
    pub fn as_str(self) -> &'static str {
        match self {
            ElectricalType::Input => "input",
            ElectricalType::Output => "output",
            ElectricalType::Bidirectional => "bidirectional",
            ElectricalType::TriState => "tri_state",
            ElectricalType::Passive => "passive",
            ElectricalType::Free => "free",
            ElectricalType::Unspecified => "unspecified",
            ElectricalType::PowerIn => "power_in",
            ElectricalType::PowerOut => "power_out",
            ElectricalType::OpenCollector => "open_collector",
            ElectricalType::OpenEmitter => "open_emitter",
            ElectricalType::NoConnect => "no_connect",
        }
    }

    /// True for pins that supply or consume power.
    pub fn is_power(self) -> bool {
        matches!(self, ElectricalType::PowerIn | ElectricalType::PowerOut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_serde() {
        for t in [
            ElectricalType::Input,
            ElectricalType::Output,
            ElectricalType::Bidirectional,
            ElectricalType::TriState,
            ElectricalType::Passive,
            ElectricalType::Free,
            ElectricalType::Unspecified,
            ElectricalType::PowerIn,
            ElectricalType::PowerOut,
            ElectricalType::OpenCollector,
            ElectricalType::OpenEmitter,
            ElectricalType::NoConnect,
        ] {
            assert_eq!(
                serde_json::to_string(&t).unwrap(),
                format!("\"{}\"", t.as_str())
            );
        }
    }

    #[test]
    fn power_flag() {
        assert!(ElectricalType::PowerIn.is_power());
        assert!(!ElectricalType::Input.is_power());
    }
}
