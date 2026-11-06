// SPDX-License-Identifier: GPL-3.0-or-later

//! Test-only crate for the golden test set in `testdata/golden`.
//!
//! Each folder there holds `datasheet.json` (manufacturer, URL and SHA-256 hash
//! of the datasheet, never the PDF), `partspec.json` (the hand-checked expected
//! PartSpec) and `expected/` (the KiCad files that must be generated from it).
//! The tests are in `tests/golden.rs`.
