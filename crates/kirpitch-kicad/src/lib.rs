// SPDX-License-Identifier: GPL-3.0-or-later

//! KiCad file writing and `kicad-cli` access for Kirpitch.
//!
//! This crate is shared by the symbol and footprint generators. It holds the
//! S-expression writer and the KiCad version targets (added in later changes)
//! and the wrapper around KiCad's command-line tool.

pub mod cli;
pub mod testing;

pub use cli::{CliError, KicadCli, KicadVersion, UpgradeOutcome};
