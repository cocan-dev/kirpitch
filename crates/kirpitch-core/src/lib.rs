// SPDX-License-Identifier: GPL-3.0-or-later

//! PartSpec schema and validation for Kirpitch.
//!
//! The PartSpec is the single JSON document from which every KiCad output is
//! generated. All lengths are in millimetres.

pub mod electrical;
pub mod error;
pub mod fab;
pub mod partspec;
pub mod source;
pub mod units;
pub mod validate;

pub use electrical::ElectricalType;
pub use error::{LoadError, NumberError};
pub use fab::FabProfile;
pub use partspec::{PartSpec, SCHEMA_VERSION};
pub use source::{BoundingBox, Confidence, Source, Sourced};
pub use units::{LengthUnit, Mm, Tolerance, format_number};
pub use validate::{Check, Finding, Level, has_errors, validate};

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
