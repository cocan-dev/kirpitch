// SPDX-License-Identifier: GPL-3.0-or-later

//! PartSpec schema and validation for Kirpitch.
//!
//! The PartSpec is the single JSON document from which every KiCad output is
//! generated. All lengths are in millimetres.

/// Returns the version of this crate, as set in the workspace manifest.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
