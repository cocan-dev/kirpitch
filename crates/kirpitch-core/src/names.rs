// SPDX-License-Identifier: GPL-3.0-or-later

//! Guesses about what a pin name means.
//!
//! Datasheets name supply pins in many ways. These functions recognise the
//! common ones so that the checks and the symbol generator agree on what a
//! ground pin is.

/// True for names of ground and negative supply pins, such as `GND`, `AGND`,
/// `VSS`, `VEE`, `V-` and `0V`.
pub fn looks_like_ground(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.contains("GND")
        || n.starts_with("VSS")
        || n.starts_with("VEE")
        || n.starts_with("V-")
        || n == "-V"
        || n == "0V"
}

/// True for names of positive supply pins, such as `VCC`, `VDD`, `V+`, `VIN`
/// and `VBAT`.
pub fn looks_like_supply(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    if looks_like_ground(&n) {
        return false;
    }
    n.starts_with("VCC")
        || n.starts_with("VDD")
        || n.starts_with("VBAT")
        || n.starts_with("VIN")
        || n.starts_with("V+")
        || n == "+V"
        || n.starts_with("AVCC")
        || n.starts_with("AVDD")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_names() {
        for n in ["GND", "AGND", "VSS", "V-", "VEE", "-V", "PGND2", "0V"] {
            assert!(looks_like_ground(n), "{n}");
        }
        for n in ["VCC", "VDD", "V+", "VIN", "OUT"] {
            assert!(!looks_like_ground(n), "{n}");
        }
    }

    #[test]
    fn supply_names() {
        for n in ["VCC", "vdd", "V+", "+V", "VIN", "VBAT", "AVDD"] {
            assert!(looks_like_supply(n), "{n}");
        }
        for n in ["GND", "VSS", "OUT", "SDA", "VREF"] {
            assert!(!looks_like_supply(n), "{n}");
        }
    }
}
