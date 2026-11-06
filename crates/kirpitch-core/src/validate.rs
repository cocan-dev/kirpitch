// SPDX-License-Identifier: GPL-3.0-or-later

//! Structural validation of a PartSpec (ADR 0006).
//!
//! These checks protect the generators from bad input. Each finding names the
//! check, the severity and the JSON path of the field to fix, never a
//! generated file.

use std::collections::{BTreeMap, BTreeSet};

use crate::electrical::ElectricalType;
use crate::fab::FabProfile;
use crate::partspec::{Origin, Pad, PadRole, PartSpec, SCHEMA_VERSION};
use crate::source::Source;
use crate::units::LengthUnit;

/// Severity of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Blocks approval and generation.
    Error,
    /// Can be overridden with a note saved in the PartSpec.
    Warning,
}

/// The checks implemented in milestone 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    /// The schema version is the one this Kirpitch writes.
    SchemaVersion,
    /// Symbol pins equal the package pin count.
    PinCount,
    /// Pin numbers are present and unique.
    PinNumber,
    /// Repeated pin names are only used for power, ground and not-connected pins.
    PinName,
    /// Every pin has a pad and every pad has a pin.
    PinPadMatch,
    /// min <= nom <= max for every dimension.
    ToleranceOrder,
    /// Values converted from inches or mils are flagged.
    Units,
    /// Pad-to-pad gap is at least the fab profile minimum.
    PadClearance,
    /// Sizes and pitch are greater than zero.
    PositiveValue,
    /// Page numbers and boxes of sources are valid.
    SourceValid,
    /// Values that were read from a datasheet carry a source.
    SourceRequired,
}

impl Check {
    /// The identifier used in reports and in `overrides`.
    pub fn id(self) -> &'static str {
        match self {
            Check::SchemaVersion => "schema_version",
            Check::PinCount => "pin_count",
            Check::PinNumber => "pin_number",
            Check::PinName => "pin_name",
            Check::PinPadMatch => "pin_pad_match",
            Check::ToleranceOrder => "tolerance_order",
            Check::Units => "units",
            Check::PadClearance => "pad_clearance",
            Check::PositiveValue => "positive_value",
            Check::SourceValid => "source_valid",
            Check::SourceRequired => "source_required",
        }
    }
}

/// One result of a check.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Which check produced it.
    pub check: Check,
    /// Severity.
    pub level: Level,
    /// JSON path of the field to fix, such as `pins[3].number`.
    pub path: String,
    /// What is wrong.
    pub message: String,
    /// The note of an override that accepts this warning, if any.
    pub overridden_by: Option<String>,
}

impl Finding {
    /// True when this finding blocks approval: an error, or a warning that has
    /// not been overridden. Only errors block; warnings never do.
    pub fn blocks(&self) -> bool {
        self.level == Level::Error
    }
}

/// Validates `spec` against `fab` and returns all findings, ordered by check
/// and path.
pub fn validate(spec: &PartSpec, fab: &FabProfile) -> Vec<Finding> {
    let mut v = Validator { out: Vec::new() };
    v.schema_version(spec);
    v.positive_values(spec);
    v.pin_count(spec);
    v.pin_numbers_and_names(spec);
    v.pin_pad_match(spec);
    v.tolerances_and_units(spec);
    v.pad_clearance(spec, fab);
    v.sources(spec);
    let mut findings = v.out;
    for f in &mut findings {
        if f.level == Level::Warning {
            f.overridden_by = spec
                .overrides
                .iter()
                .find(|o| o.check == f.check.id() && (o.path.is_empty() || o.path == f.path))
                .map(|o| o.note.clone());
        }
    }
    findings.sort_by(|a, b| (a.check, &a.path).cmp(&(b.check, &b.path)));
    findings
}

/// True when any finding is an error.
pub fn has_errors(findings: &[Finding]) -> bool {
    findings.iter().any(Finding::blocks)
}

struct Validator {
    out: Vec<Finding>,
}

impl Validator {
    fn add(&mut self, check: Check, level: Level, path: impl Into<String>, msg: impl Into<String>) {
        self.out.push(Finding {
            check,
            level,
            path: path.into(),
            message: msg.into(),
            overridden_by: None,
        });
    }

    fn schema_version(&mut self, spec: &PartSpec) {
        if spec.schema_version != SCHEMA_VERSION {
            self.add(
                Check::SchemaVersion,
                Level::Error,
                "schema_version",
                format!("expected {SCHEMA_VERSION}, found {}", spec.schema_version),
            );
        }
    }

    fn positive_values(&mut self, spec: &PartSpec) {
        let p = &spec.package;
        let mut check = |path: &str, value: f64| {
            if value <= 0.0 {
                self.add(
                    Check::PositiveValue,
                    Level::Error,
                    path,
                    format!("must be greater than 0, found {value}"),
                );
            }
        };
        check("package.pitch", p.pitch.get());
        check("package.body.length", p.body.length.get());
        check("package.body.width", p.body.width.get());
        check("package.body.height_max", p.body.height_max.get());
        for (i, pad) in spec.land_pattern.pads.iter().enumerate() {
            check(&format!("land_pattern.pads[{i}].size_x"), pad.size_x.get());
            check(&format!("land_pattern.pads[{i}].size_y"), pad.size_y.get());
        }
    }

    fn pin_count(&mut self, spec: &PartSpec) {
        let actual = spec.pins.len();
        if actual != spec.package.pin_count as usize {
            self.add(
                Check::PinCount,
                Level::Error,
                "pins",
                format!(
                    "package.pin_count is {} but {actual} pins are listed (exposed pad included)",
                    spec.package.pin_count
                ),
            );
        }
    }

    fn pin_numbers_and_names(&mut self, spec: &PartSpec) {
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, pin) in spec.pins.iter().enumerate() {
            if pin.number.trim().is_empty() {
                self.add(
                    Check::PinNumber,
                    Level::Error,
                    format!("pins[{i}].number"),
                    "pin number is empty",
                );
            } else if let Some(first) = seen.insert(pin.number.as_str(), i) {
                self.add(
                    Check::PinNumber,
                    Level::Error,
                    format!("pins[{i}].number"),
                    format!(
                        "pin number `{}` is already used by pins[{first}]",
                        pin.number
                    ),
                );
            }
        }
        let repeatable = |t: ElectricalType| t.is_power() || t == ElectricalType::NoConnect;
        let mut names: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, pin) in spec.pins.iter().enumerate() {
            if let Some(&first) = names.get(pin.name.as_str()) {
                let other = &spec.pins[first];
                if !(repeatable(pin.electrical_type) && repeatable(other.electrical_type)) {
                    self.add(
                        Check::PinName,
                        Level::Error,
                        format!("pins[{i}].name"),
                        format!(
                            "pin name `{}` is repeated from pins[{first}]; only power, ground and not-connected pins may repeat",
                            pin.name
                        ),
                    );
                }
            } else {
                names.insert(pin.name.as_str(), i);
            }
        }
    }

    fn pin_pad_match(&mut self, spec: &PartSpec) {
        let pin_numbers: BTreeSet<&str> = spec.pins.iter().map(|p| p.number.as_str()).collect();
        let signal_pads: BTreeSet<&str> = spec
            .land_pattern
            .pads
            .iter()
            .filter(|p| p.role == PadRole::Signal)
            .map(|p| p.number.as_str())
            .collect();
        for (i, pin) in spec.pins.iter().enumerate() {
            if !signal_pads.contains(pin.number.as_str()) {
                self.add(
                    Check::PinPadMatch,
                    Level::Error,
                    format!("pins[{i}].number"),
                    format!("pin `{}` has no pad in the land pattern", pin.number),
                );
            }
        }
        for (i, pad) in spec.land_pattern.pads.iter().enumerate() {
            if pad.role == PadRole::Signal && !pin_numbers.contains(pad.number.as_str()) {
                self.add(
                    Check::PinPadMatch,
                    Level::Error,
                    format!("land_pattern.pads[{i}].number"),
                    format!(
                        "pad `{}` has no pin; mark it mechanical or not_connected if that is intended",
                        pad.number
                    ),
                );
            }
        }
    }

    fn tolerances_and_units(&mut self, spec: &PartSpec) {
        for (key, dim) in &spec.package.dimensions {
            let path = format!("package.dimensions.{key}");
            if !dim.value.is_ordered() {
                self.add(
                    Check::ToleranceOrder,
                    Level::Error,
                    format!("{path}.value"),
                    "must satisfy min <= nom <= max",
                );
            }
            if let Some(unit) = dim.converted_from
                && unit != LengthUnit::Mm
            {
                self.add(
                    Check::Units,
                    Level::Warning,
                    format!("{path}.converted_from"),
                    format!("value was converted from {unit:?} to millimetres; check the result"),
                );
            }
        }
    }

    fn pad_clearance(&mut self, spec: &PartSpec, fab: &FabProfile) {
        let pads = &spec.land_pattern.pads;
        let min = fab.min_copper_gap.get();
        for i in 0..pads.len() {
            for j in (i + 1)..pads.len() {
                if pads[i].number == pads[j].number {
                    continue;
                }
                let gap = pad_gap(&pads[i], &pads[j]);
                if gap < min - 1e-9 {
                    self.add(
                        Check::PadClearance,
                        Level::Error,
                        format!("land_pattern.pads[{j}]"),
                        format!(
                            "gap to pad `{}` (pads[{i}]) is {:.3} mm, below the {:.3} mm minimum of fab profile `{}`",
                            pads[i].number, gap, min, fab.name
                        ),
                    );
                }
            }
        }
    }

    fn sources(&mut self, spec: &PartSpec) {
        // Every place that can carry a source, with the source if there is one.
        let mut slots: Vec<(String, Option<&Source>)> = Vec::new();
        slots.push(("package.source".into(), spec.package.source.as_ref()));
        for (k, d) in &spec.package.dimensions {
            slots.push((format!("package.dimensions.{k}.source"), d.source.as_ref()));
        }
        for (i, p) in spec.pins.iter().enumerate() {
            slots.push((format!("pins[{i}].source"), p.source.as_ref()));
        }
        slots.push((
            "land_pattern.source".into(),
            spec.land_pattern.source.as_ref(),
        ));
        for (i, p) in spec.land_pattern.pads.iter().enumerate() {
            slots.push((format!("land_pattern.pads[{i}].source"), p.source.as_ref()));
        }
        // Hand-written values may omit the source (ADR 0004); values read from a
        // datasheet by a region pick or a model may not.
        if spec.origin != Origin::Hand {
            for (path, source) in &slots {
                match source {
                    None => self.add(
                        Check::SourceRequired,
                        Level::Error,
                        path.clone(),
                        format!(
                            "a value with origin `{}` needs a source (page and region)",
                            spec.origin.as_str()
                        ),
                    ),
                    Some(s) => {
                        if s.bbox.is_none() {
                            self.add(
                                Check::SourceRequired,
                                Level::Error,
                                format!("{path}.bbox"),
                                "the region on the page is required",
                            );
                        }
                        if spec.origin == Origin::Llm && s.confidence.is_none() {
                            self.add(
                                Check::SourceRequired,
                                Level::Error,
                                format!("{path}.confidence"),
                                "the confidence of a model reading is required",
                            );
                        }
                    }
                }
            }
        }
        let all: Vec<(String, &Source)> = slots
            .into_iter()
            .filter_map(|(p, s)| s.map(|s| (p, s)))
            .collect();
        for (path, s) in all {
            if s.page == 0 {
                self.add(
                    Check::SourceValid,
                    Level::Error,
                    format!("{path}.page"),
                    "page numbers count from 1",
                );
            }
            if let Some(b) = &s.bbox
                && !b.is_valid()
            {
                self.add(
                    Check::SourceValid,
                    Level::Error,
                    format!("{path}.bbox"),
                    "must be [left, top, right, bottom] within 0 to 1, not inverted",
                );
            }
        }
    }
}

/// Smallest distance between the axis-aligned bounding boxes of two pads.
/// Rotated pads use the box around the rotated rectangle, which can only make
/// the gap look smaller, never larger.
fn pad_gap(a: &Pad, b: &Pad) -> f64 {
    let (ax0, ay0, ax1, ay1) = a.bounds();
    let (bx0, by0, bx1, by1) = b.bounds();
    let dx = (ax0 - bx1).max(bx0 - ax1).max(0.0);
    let dy = (ay0 - by1).max(by0 - ay1).max(0.0);
    dx.hypot(dy)
}
