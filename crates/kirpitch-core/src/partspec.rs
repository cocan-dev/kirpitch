// SPDX-License-Identifier: GPL-3.0-or-later

//! The PartSpec: the single JSON document every KiCad output is built from.
//!
//! All lengths are millimetres. Source tracing is optional in hand-written
//! files (ADR 0004). The 3D body and electrical parameters are reserved for
//! later milestones (ADR 0001) and not part of the schema yet; unknown fields
//! are rejected so a typo never passes silently.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::electrical::ElectricalType;
use crate::error::LoadError;
use crate::source::{Source, Sourced};
use crate::units::{Mm, Tolerance};

/// The schema version written by this version of Kirpitch.
pub const SCHEMA_VERSION: u32 = 1;

/// A complete description of one part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartSpec {
    /// Version of the PartSpec schema; currently 1.
    pub schema_version: u32,
    /// How the values were obtained. Values of a hand-written file may omit
    /// their source; values read from a datasheet may not (ADR 0004).
    #[serde(default, skip_serializing_if = "Origin::is_hand")]
    pub origin: Origin,
    /// Identity of the part.
    pub metadata: Metadata,
    /// The package (body and pitch).
    pub package: Package,
    /// The pins of the part, in the order they should appear in a group.
    pub pins: Vec<Pin>,
    /// The pads of the footprint.
    pub land_pattern: LandPattern,
    /// Which stages the user has approved.
    #[serde(default)]
    pub approvals: Approvals,
    /// Notes that override warnings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<Override>,
}

/// How the values of a PartSpec were obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Typed in by a person. Sources are optional.
    #[default]
    Hand,
    /// Picked by a person from a region of a datasheet page. Page and region
    /// are required.
    Region,
    /// Read by a language model. Page, region and confidence are required.
    Llm,
}

impl Origin {
    /// True for [`Origin::Hand`]; used to leave the default out of files.
    pub fn is_hand(&self) -> bool {
        *self == Origin::Hand
    }

    /// The name used in PartSpec files.
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Hand => "hand",
            Origin::Region => "region",
            Origin::Llm => "llm",
        }
    }
}

/// Identity and descriptive fields of the part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    /// Manufacturer part number, for example `LM358DR`.
    pub mpn: String,
    /// Manufacturer name.
    pub manufacturer: String,
    /// One-line description.
    pub description: String,
    /// Search keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// URL of the datasheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet_url: Option<String>,
    /// SHA-256 hash (lowercase hex) of the datasheet PDF that was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet_sha256: Option<String>,
}

/// Package family, name and main dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// Package family, for example `SOIC`.
    pub family: String,
    /// Footprint name following KiCad naming, for example
    /// `SOIC-8_3.9x4.9mm_P1.27mm`.
    pub name: String,
    /// Number of pins, exposed pad included.
    pub pin_count: u32,
    /// Distance between neighbouring pin centres.
    pub pitch: Mm,
    /// Outline of the plastic body.
    pub body: Body,
    /// Named dimensions from the datasheet drawing, keyed by their letter
    /// (for example `A`, `A1`, `b`, `D`, `E`, `e`, `L`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dimensions: BTreeMap<String, Sourced<Tolerance>>,
    /// Where the package drawing is in the datasheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// Size of the package body.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Body {
    /// Size along the pin rows (D in JEDEC drawings; y direction in the
    /// footprint, where the rows are on the left and right).
    pub length: Mm,
    /// Size across the pin rows without leads (E1 in JEDEC drawings; x
    /// direction in the footprint).
    pub width: Mm,
    /// Maximum height above the board.
    pub height_max: Mm,
    /// Where the centre of the body is, relative to the footprint origin.
    /// Surface-mount footprints have their origin at the centre (leave this
    /// out); through-hole footprints have it at pin 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<Point>,
}

/// A position in footprint coordinates (x right, y down), in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    /// Horizontal position.
    pub x: Mm,
    /// Vertical position.
    pub y: Mm,
}

/// One pin of the part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    /// Pin number as printed on the datasheet, for example `1`, `A1` or `EP`.
    pub number: String,
    /// Pin name, for example `VCC`. KiCad overbar notation `~{NAME}` is allowed.
    pub name: String,
    /// Electrical type.
    pub electrical_type: ElectricalType,
    /// Functional group, used to place the pin on the symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Alternate functions of the pin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alt_functions: Vec<String>,
    /// Where the pin is listed in the datasheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// The pads of the footprint and its courtyard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LandPattern {
    /// The pads.
    pub pads: Vec<Pad>,
    /// Courtyard rectangle. When absent the generator derives one from the pads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courtyard: Option<Rect>,
    /// Where the land pattern is in the datasheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// An axis-aligned rectangle in footprint coordinates (x right, y down).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    /// Smallest x.
    pub x_min: Mm,
    /// Smallest y.
    pub y_min: Mm,
    /// Largest x.
    pub x_max: Mm,
    /// Largest y.
    pub y_max: Mm,
}

/// One surface-mount pad.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pad {
    /// Pad number; matches a pin number. Several pads may share a number.
    pub number: String,
    /// Shape of the pad.
    pub shape: PadShape,
    /// Centre x, in footprint coordinates (x right).
    pub x: Mm,
    /// Centre y, in footprint coordinates (y down).
    pub y: Mm,
    /// Size along x before rotation.
    pub size_x: Mm,
    /// Size along y before rotation.
    pub size_y: Mm,
    /// Rotation in degrees, counter-clockwise.
    #[serde(default)]
    pub rotation_deg: f64,
    /// Corner radius as a fraction of the shorter side, for rounded rectangles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_ratio: Option<f64>,
    /// Whether the pad gets solder paste.
    #[serde(default = "default_true")]
    pub paste: bool,
    /// What the pad is for.
    #[serde(default)]
    pub role: PadRole,
    /// Surface-mount (default) or through-hole.
    #[serde(default, skip_serializing_if = "PadKind::is_smd")]
    pub kind: PadKind,
    /// Diameter of the hole of a through-hole pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drill: Option<Mm>,
    /// Where the pad is shown in the datasheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

fn default_true() -> bool {
    true
}

/// Shape of a pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    /// Rectangle.
    Rect,
    /// Rectangle with rounded corners.
    RoundRect,
    /// Oval (stadium).
    Oval,
    /// Circle.
    Circle,
}

/// How a pad is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadKind {
    /// Surface-mount pad on the top layer.
    #[default]
    Smd,
    /// Through-hole pad with a drill, on all copper layers.
    ThruHole,
}

impl PadKind {
    /// True for [`PadKind::Smd`]; used to leave the default out of files.
    pub fn is_smd(&self) -> bool {
        *self == PadKind::Smd
    }
}

/// What a pad is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadRole {
    /// A normal pad connected to a pin.
    #[default]
    Signal,
    /// A mechanical pad (for example a mounting tab) without a pin.
    Mechanical,
    /// A pad that is intentionally not connected.
    NotConnected,
}

/// Approval state of the review stages (gates).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Approvals {
    /// Gate 1: pinout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinout: Option<Approval>,
    /// Gate 2: footprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Approval>,
    /// Gate 3: 3D model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_3d: Option<Approval>,
    /// Gate 4: SPICE model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spice: Option<Approval>,
}

/// One recorded approval. Stored in milestone 1, enforced from milestone 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    /// Whether the stage is approved.
    pub approved: bool,
    /// When, as text such as `2026-11-06T12:00:00Z`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// Free-form note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Fingerprint of the stage content that was approved. When the content
    /// changes, the approval is stale and the stage is open again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// A user note that overrides a warning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Override {
    /// Identifier of the check, for example `units`.
    pub check: String,
    /// Path of the field the override applies to; empty means every finding of
    /// that check.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// Why the warning is acceptable.
    pub note: String,
}

/// The JSON schema of the PartSpec as pretty JSON text with a final newline.
///
/// It is generated from the Rust types, so it can never disagree with the
/// code. The committed copy is `schema/partspec.schema.json`.
pub fn json_schema_string() -> String {
    let schema = schemars::schema_for!(PartSpec);
    let mut text = serde_json::to_string_pretty(&schema).expect("schema serialisation cannot fail");
    text.push('\n');
    text
}

impl PartSpec {
    /// Reads a PartSpec from JSON text. Errors name the JSON path of the
    /// offending field.
    pub fn from_json_str(text: &str) -> Result<PartSpec, LoadError> {
        let mut de = serde_json::Deserializer::from_str(text);
        let spec: PartSpec = serde_path_to_error::deserialize(&mut de).map_err(|e| {
            let path = e.path().to_string();
            LoadError::Invalid {
                path,
                message: e.into_inner().to_string(),
            }
        })?;
        de.end().map_err(|e| LoadError::Invalid {
            path: String::new(),
            message: e.to_string(),
        })?;
        if spec.schema_version != SCHEMA_VERSION {
            return Err(LoadError::UnsupportedVersion {
                found: spec.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(spec)
    }

    /// Writes the PartSpec as pretty JSON with two-space indentation and a
    /// final newline. Field order is fixed, so the output is deterministic.
    pub fn to_json_string(&self) -> String {
        let mut text =
            serde_json::to_string_pretty(self).expect("PartSpec serialisation cannot fail");
        text.push('\n');
        text
    }
}

impl Pad {
    /// The smallest axis-aligned box around the pad, as
    /// `(x_min, y_min, x_max, y_max)` in millimetres. Rotated pads use the box
    /// around the rotated rectangle; round and oval shapes use their full
    /// rectangle, so the box is never smaller than the pad.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let r = self.rotation_deg.to_radians();
        let (s, c) = (r.sin().abs(), r.cos().abs());
        let (w, h) = (self.size_x.get(), self.size_y.get());
        let hx = (w * c + h * s) / 2.0;
        let hy = (w * s + h * c) / 2.0;
        (
            self.x.get() - hx,
            self.y.get() - hy,
            self.x.get() + hx,
            self.y.get() + hy,
        )
    }
}
