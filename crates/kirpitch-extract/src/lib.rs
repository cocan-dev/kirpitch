// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading datasheets for Kirpitch.
//!
//! Milestone 2a puts the PDF side here: rendering pages as images and reading
//! the text of a page with the position of every character, so that a region
//! picked by the user can be turned into text and tables. Language model
//! extraction (milestone 2b) is added to this crate later.

pub mod pdf;

pub use pdf::{Line, PdfError, PdfReader, RenderedPage, TextChar, Word};
