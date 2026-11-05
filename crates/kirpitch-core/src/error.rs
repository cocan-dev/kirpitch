// SPDX-License-Identifier: GPL-3.0-or-later

//! Error types for the primitive value types.

use thiserror::Error;

/// A PartSpec file that cannot be loaded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LoadError {
    /// The JSON is malformed or a field is wrong. `path` names the field.
    #[error("invalid PartSpec at `{path}`: {message}")]
    Invalid {
        /// JSON path of the field, such as `pins[2].electrical_type`; `.` for
        /// the document root.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// The file uses a schema version this Kirpitch does not read.
    #[error("unsupported schema_version {found}; this version of Kirpitch reads {supported}")]
    UnsupportedVersion {
        /// Version found in the file.
        found: u32,
        /// Version this Kirpitch supports.
        supported: u32,
    },
}

/// A number that cannot be used as a value in a PartSpec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NumberError {
    /// The number is NaN (not a number) or infinite.
    #[error("number must be finite")]
    NotFinite,
    /// A confidence outside the range 0 to 1.
    #[error("confidence must be between 0 and 1")]
    ConfidenceOutOfRange,
}
