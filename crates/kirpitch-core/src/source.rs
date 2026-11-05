// SPDX-License-Identifier: GPL-3.0-or-later

//! Source tracing: where in a datasheet a value was read, and how sure we are.
//!
//! Source tracing is optional in hand-written PartSpecs and required for
//! extracted values from milestone 2 on (ADR 0004).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::NumberError;
use crate::units::LengthUnit;

/// How confident the extraction was, from 0 (no confidence) to 1 (certain).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Confidence(f64);

impl Confidence {
    /// Creates a confidence, or fails when it is not within 0 to 1.
    pub fn new(value: f64) -> Result<Confidence, NumberError> {
        if (0.0..=1.0).contains(&value) {
            Ok(Confidence(value))
        } else {
            Err(NumberError::ConfidenceOutOfRange)
        }
    }

    /// The value between 0 and 1.
    pub fn get(self) -> f64 {
        self.0
    }
}

impl JsonSchema for Confidence {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Confidence".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "number",
            "minimum": 0,
            "maximum": 1,
            "description": "Confidence from 0 (none) to 1 (certain)."
        })
    }
}

impl TryFrom<f64> for Confidence {
    type Error = NumberError;

    fn try_from(value: f64) -> Result<Confidence, NumberError> {
        Confidence::new(value)
    }
}

impl From<Confidence> for f64 {
    fn from(value: Confidence) -> f64 {
        value.0
    }
}

/// A rectangle on a datasheet page, as fractions of the page size.
///
/// `[left, top, right, bottom]`, each from 0 to 1, measured from the top-left
/// corner of the page. Fractions make the box independent of the render
/// resolution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct BoundingBox(pub [f64; 4]);

impl BoundingBox {
    /// True when all four numbers are within 0 to 1 and the box is not inverted.
    pub fn is_valid(&self) -> bool {
        let [l, t, r, b] = self.0;
        self.0.iter().all(|v| (0.0..=1.0).contains(v)) && l <= r && t <= b
    }
}

/// Where a value was read from in the datasheet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Page number in the datasheet PDF, counting from 1.
    pub page: u32,
    /// Region on the page the value was read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BoundingBox>,
    /// How confident the extraction was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// A value together with its optional source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sourced<T> {
    /// The value itself.
    pub value: T,
    /// Where the value was read; `None` for values typed in without a source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// The unit the datasheet used, when it was not millimetres and the value
    /// was converted. Such values are flagged with a warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converted_from: Option<LengthUnit>,
}

impl<T> Sourced<T> {
    /// A value with no source information.
    pub fn unsourced(value: T) -> Sourced<T> {
        Sourced {
            value,
            source: None,
            converted_from: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_range() {
        assert!(Confidence::new(0.0).is_ok());
        assert!(Confidence::new(1.0).is_ok());
        assert_eq!(
            Confidence::new(1.01),
            Err(NumberError::ConfidenceOutOfRange)
        );
        assert!(Confidence::new(f64::NAN).is_err());
        assert!(serde_json::from_str::<Confidence>("2").is_err());
    }

    #[test]
    fn bbox_validity() {
        assert!(BoundingBox([0.1, 0.1, 0.5, 0.5]).is_valid());
        assert!(!BoundingBox([0.6, 0.1, 0.5, 0.5]).is_valid());
        assert!(!BoundingBox([0.1, 0.1, 1.5, 0.5]).is_valid());
    }

    #[test]
    fn sourced_without_source_is_short_json() {
        let v = Sourced::unsourced(1.27);
        assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"value":1.27}"#);
        let back: Sourced<f64> = serde_json::from_str(r#"{"value":1.27}"#).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn sourced_with_source_round_trips() {
        let json = r#"{"value":0.4,"source":{"page":3,"bbox":[0.1,0.2,0.3,0.4],"confidence":0.9},"converted_from":"mil"}"#;
        let v: Sourced<f64> = serde_json::from_str(json).unwrap();
        assert_eq!(v.source.unwrap().page, 3);
        assert_eq!(v.converted_from, Some(LengthUnit::Mil));
        assert_eq!(serde_json::to_string(&v).unwrap(), json);
    }

    #[test]
    fn source_rejects_unknown_fields() {
        assert!(serde_json::from_str::<Source>(r#"{"page":1,"pg":2}"#).is_err());
    }
}
