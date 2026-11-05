// SPDX-License-Identifier: GPL-3.0-or-later

//! Error types for the primitive value types.

use thiserror::Error;

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
