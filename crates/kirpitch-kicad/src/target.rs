// SPDX-License-Identifier: GPL-3.0-or-later

//! KiCad versions Kirpitch can write files for.
//!
//! The PartSpec does not depend on any KiCad version; only the writers do. The
//! writers are built in the order KiCad 10, 9, 8 (ADR in requirements, "KiCad
//! versions"). Only KiCad 10 exists so far.

use crate::cli::{KicadCli, KicadVersion};

/// A KiCad major version a file can be written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum KicadTarget {
    /// KiCad 10 (released March 2026).
    V10,
}

/// The reason a version cannot be used as a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("KiCad {major} is not a supported target; supported: {supported}")]
pub struct UnsupportedTarget {
    /// The version that was asked for.
    pub major: u32,
    /// Supported majors, as text.
    pub supported: &'static str,
}

impl KicadTarget {
    /// All supported targets, oldest first.
    pub const ALL: &'static [KicadTarget] = &[KicadTarget::V10];

    /// The newest supported target.
    pub const LATEST: KicadTarget = KicadTarget::V10;

    /// The KiCad major version number.
    pub fn major(self) -> u32 {
        match self {
            KicadTarget::V10 => 10,
        }
    }

    /// The target for a KiCad major version.
    pub fn from_major(major: u32) -> Result<KicadTarget, UnsupportedTarget> {
        KicadTarget::ALL
            .iter()
            .copied()
            .find(|t| t.major() == major)
            .ok_or(UnsupportedTarget {
                major,
                supported: "10",
            })
    }

    /// The target to use by default (ADR 0002): the installed KiCad version
    /// when `cli` is given and reports a supported version, else the latest.
    /// A version newer than the latest supported also gives the latest.
    pub fn detect(cli: Option<&KicadCli>) -> KicadTarget {
        let Some(cli) = cli else {
            return KicadTarget::LATEST;
        };
        match cli.version() {
            Ok(KicadVersion { major, .. }) if major > KicadTarget::LATEST.major() => {
                KicadTarget::LATEST
            }
            Ok(KicadVersion { major, .. }) => {
                KicadTarget::from_major(major).unwrap_or(KicadTarget::LATEST)
            }
            Err(_) => KicadTarget::LATEST,
        }
    }

    /// The format version number written in `.kicad_mod` files.
    pub fn footprint_format_version(self) -> u32 {
        match self {
            KicadTarget::V10 => 20_260_206,
        }
    }

    /// The format version number written in `.kicad_sym` files.
    pub fn symbol_format_version(self) -> u32 {
        match self {
            KicadTarget::V10 => 20_251_024,
        }
    }

    /// The `generator_version` text written in symbol libraries.
    pub fn generator_version(self) -> &'static str {
        match self {
            KicadTarget::V10 => "10.0",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majors() {
        assert_eq!(KicadTarget::from_major(10), Ok(KicadTarget::V10));
        let err = KicadTarget::from_major(7).unwrap_err();
        assert_eq!(err.major, 7);
        assert!(err.to_string().contains("KiCad 7"));
    }

    #[test]
    fn default_without_kicad_is_latest() {
        assert_eq!(KicadTarget::detect(None), KicadTarget::LATEST);
    }

    #[test]
    fn broken_tool_falls_back_to_latest() {
        let cli = KicadCli::new("definitely-not-a-real-kicad-cli");
        assert_eq!(KicadTarget::detect(Some(&cli)), KicadTarget::LATEST);
    }

    #[test]
    fn format_versions_are_those_observed_in_kicad_10() {
        assert_eq!(KicadTarget::V10.footprint_format_version(), 20260206);
        assert_eq!(KicadTarget::V10.symbol_format_version(), 20251024);
    }
}
