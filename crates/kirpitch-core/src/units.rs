// SPDX-License-Identifier: GPL-3.0-or-later

//! Lengths in millimetres, tolerances and unit conversion.
//!
//! All lengths inside Kirpitch are millimetres (CONTRIBUTING.md). Values that
//! were given in inches or mils (thousandths of an inch) are converted on input
//! and flagged with [`LengthUnit`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::NumberError;

/// Number of decimals kept when a length is written to a file (ADR 0010).
pub const DECIMALS: i32 = 6;

/// A length in millimetres. Always finite.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Mm(f64);

impl Mm {
    /// Zero millimetres.
    pub const ZERO: Mm = Mm(0.0);

    /// Creates a length, or fails when `value` is NaN or infinite.
    pub fn new(value: f64) -> Result<Mm, NumberError> {
        if value.is_finite() {
            Ok(Mm(value))
        } else {
            Err(NumberError::NotFinite)
        }
    }

    /// Converts a length given in `unit` to millimetres.
    pub fn from_unit(value: f64, unit: LengthUnit) -> Result<Mm, NumberError> {
        Mm::new(value * unit.mm_per_unit())
    }

    /// The value in millimetres, as stored.
    pub fn get(self) -> f64 {
        self.0
    }

    /// The value rounded to [`DECIMALS`] decimals, with `-0` turned into `0`.
    pub fn rounded(self) -> f64 {
        let factor = 10f64.powi(DECIMALS);
        let r = (self.0 * factor).round() / factor;
        if r == 0.0 { 0.0 } else { r }
    }

    /// The canonical text form used in generated files: rounded, no trailing
    /// zeros, no exponent. For example `1.27`, `0.5`, `3`, `0`.
    pub fn format(self) -> String {
        let mut s = format!("{:.*}", DECIMALS as usize, self.rounded());
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
}

/// Formats any number the way lengths are formatted in generated files (see
/// [`Mm::format`]). Used for angles and ratios too. NaN and infinity, which
/// cannot come from a valid PartSpec, are written as `0`.
pub fn format_number(value: f64) -> String {
    if value.is_finite() {
        Mm(value).format()
    } else {
        "0".to_string()
    }
}

impl JsonSchema for Mm {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Mm".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "number",
            "description": "A length in millimetres."
        })
    }
}

impl TryFrom<f64> for Mm {
    type Error = NumberError;

    fn try_from(value: f64) -> Result<Mm, NumberError> {
        Mm::new(value)
    }
}

impl From<Mm> for f64 {
    fn from(value: Mm) -> f64 {
        value.0
    }
}

/// A unit a datasheet may give lengths in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    /// Millimetres (no conversion).
    Mm,
    /// Inches (25.4 mm).
    Inch,
    /// Mils, thousandths of an inch (0.0254 mm).
    Mil,
}

impl LengthUnit {
    /// How many millimetres one unit is.
    pub fn mm_per_unit(self) -> f64 {
        match self {
            LengthUnit::Mm => 1.0,
            LengthUnit::Inch => 25.4,
            LengthUnit::Mil => 0.0254,
        }
    }
}

/// A dimension with optional minimum and maximum, as printed in datasheets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    /// Smallest allowed value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Mm>,
    /// Nominal (typical) value.
    pub nom: Mm,
    /// Largest allowed value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Mm>,
}

impl Tolerance {
    /// A dimension with only a nominal value.
    pub fn nominal(nom: Mm) -> Tolerance {
        Tolerance {
            min: None,
            nom,
            max: None,
        }
    }

    /// True when `min <= nom <= max` for the values that are present.
    pub fn is_ordered(&self) -> bool {
        let min_ok = self.min.is_none_or(|m| m <= self.nom);
        let max_ok = self.max.is_none_or(|m| self.nom <= m);
        let both_ok = match (self.min, self.max) {
            (Some(lo), Some(hi)) => lo <= hi,
            _ => true,
        };
        min_ok && max_ok && both_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(v: f64) -> Mm {
        Mm::new(v).unwrap()
    }

    #[test]
    fn rejects_non_finite() {
        assert_eq!(Mm::new(f64::NAN), Err(NumberError::NotFinite));
        assert_eq!(Mm::new(f64::INFINITY), Err(NumberError::NotFinite));
    }

    #[test]
    fn formats_canonically() {
        assert_eq!(mm(1.27).format(), "1.27");
        assert_eq!(mm(0.5).format(), "0.5");
        assert_eq!(mm(3.0).format(), "3");
        assert_eq!(mm(0.0).format(), "0");
        assert_eq!(mm(-0.0).format(), "0");
        assert_eq!(mm(-0.0000004).format(), "0");
        assert_eq!(mm(-1.5).format(), "-1.5");
        assert_eq!(mm(0.1 + 0.2).format(), "0.3");
        assert_eq!(mm(1234567.0).format(), "1234567");
        assert_eq!(mm(0.0000015).format(), "0.000002");
    }

    #[test]
    fn converts_units() {
        assert_eq!(
            Mm::from_unit(50.0, LengthUnit::Mil).unwrap().format(),
            "1.27"
        );
        assert_eq!(
            Mm::from_unit(0.1, LengthUnit::Inch).unwrap().format(),
            "2.54"
        );
        assert_eq!(Mm::from_unit(2.0, LengthUnit::Mm).unwrap().format(), "2");
    }

    #[test]
    fn serde_round_trip_and_rejects_bad_numbers() {
        let json = serde_json::to_string(&mm(1.27)).unwrap();
        assert_eq!(json, "1.27");
        let back: Mm = serde_json::from_str("1.27").unwrap();
        assert_eq!(back, mm(1.27));
        assert!(serde_json::from_str::<Mm>("\"x\"").is_err());
        assert!(serde_json::from_str::<Mm>("null").is_err());
    }

    #[test]
    fn tolerance_order() {
        assert!(Tolerance::nominal(mm(1.0)).is_ordered());
        let ok = Tolerance {
            min: Some(mm(0.9)),
            nom: mm(1.0),
            max: Some(mm(1.1)),
        };
        assert!(ok.is_ordered());
        let bad_min = Tolerance {
            min: Some(mm(1.2)),
            ..ok
        };
        assert!(!bad_min.is_ordered());
        let bad_max = Tolerance {
            max: Some(mm(0.8)),
            ..ok
        };
        assert!(!bad_max.is_ordered());
    }

    #[test]
    fn tolerance_rejects_unknown_fields() {
        let r: Result<Tolerance, _> = serde_json::from_str(r#"{"nom":1.0,"typ":1.0}"#);
        assert!(r.is_err());
    }
}
