// SPDX-License-Identifier: GPL-3.0-or-later

//! Structural validation of a PartSpec (ADR 0006).
//!
//! These checks protect the generators from bad input. Each finding names the
//! check, the severity and the JSON path of the field to fix, never a
//! generated file.

use std::collections::{BTreeMap, BTreeSet};

use crate::electrical::ElectricalType;
use crate::fab::FabProfile;
use crate::ipc;
use crate::names;
use crate::partspec::{Origin, Pad, PadKind, PadRole, PartSpec, SCHEMA_VERSION};
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
    /// Supply and ground pins have power types and there is a ground pin.
    PowerPins,
    /// The pins of a row fit inside the package body.
    PitchFit,
    /// A large exposed pad has one big paste area that should be reviewed.
    ExposedPad,
    /// An override refers to a check that does not exist.
    OverrideUnknown,
    /// The datasheet land pattern and the IPC-7351 result agree.
    TwoSources,
    /// Through-hole pads have a hole that is smaller than the pad.
    Drill,
    /// The 3D model sits on the footprint: terminals on pads, body inside the
    /// courtyard, height within the maximum, pin 1 in the same corner. The
    /// check runs in the 3D generator, which reports it.
    ModelFit,
    /// Pads, drills, holes and the solder mask web meet the other limits of
    /// the fab profile (the copper gap is `pad_clearance`).
    FabLimits,
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
            Check::PowerPins => "power_pins",
            Check::PitchFit => "pitch_fit",
            Check::ExposedPad => "exposed_pad",
            Check::OverrideUnknown => "override_unknown",
            Check::TwoSources => "two_sources",
            Check::Drill => "drill",
            Check::ModelFit => "model_fit",
            Check::FabLimits => "fab_limits",
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
    v.power_pins(spec);
    v.pitch_fit(spec);
    v.exposed_pad(spec);
    v.overrides(spec);
    v.two_sources(spec);
    v.drills(spec);
    v.fab_limits(spec, fab);
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

/// Every check id that can appear in an override.
const ALL_CHECKS: [Check; 19] = [
    Check::SchemaVersion,
    Check::PinCount,
    Check::PinNumber,
    Check::PinName,
    Check::PinPadMatch,
    Check::ToleranceOrder,
    Check::Units,
    Check::PadClearance,
    Check::PositiveValue,
    Check::SourceValid,
    Check::SourceRequired,
    Check::PowerPins,
    Check::PitchFit,
    Check::ExposedPad,
    Check::OverrideUnknown,
    Check::TwoSources,
    Check::Drill,
    Check::ModelFit,
    Check::FabLimits,
];

/// Pads with a paste area above this size count as large exposed pads.
const EXPOSED_PAD_AREA_MM2: f64 = 4.0;

impl Validator {
    fn power_pins(&mut self, spec: &PartSpec) {
        let mut has_power_pin = false;
        let mut has_ground = false;
        for (i, pin) in spec.pins.iter().enumerate() {
            let is_power = pin.electrical_type.is_power();
            has_power_pin |= is_power;
            let ground = names::looks_like_ground(&pin.name);
            has_ground |= is_power && ground;
            if !is_power && (ground || names::looks_like_supply(&pin.name)) {
                self.add(
                    Check::PowerPins,
                    Level::Warning,
                    format!("pins[{i}].electrical_type"),
                    format!(
                        "pin `{}` looks like a supply pin but its type is `{}`; use power_in or power_out",
                        pin.name,
                        pin.electrical_type.as_str()
                    ),
                );
            }
        }
        if has_power_pin && !has_ground {
            self.add(
                Check::PowerPins,
                Level::Warning,
                "pins",
                "no ground pin: no power pin has a name like GND or VSS",
            );
        }
    }

    /// `(pins in a row - 1) x pitch` must fit inside the body length (rows on
    /// the left and right) or width (rows at the top and bottom).
    ///
    /// A row is a group of at least three signal pads with the same x (a
    /// vertical row) or the same y (a horizontal row). A lone exposed pad or
    /// the pairs of pads facing each other across a dual-row package are not
    /// rows.
    fn pitch_fit(&mut self, spec: &PartSpec) {
        let pitch = spec.package.pitch.get();
        if pitch <= 0.0 {
            return; // reported by the positive value check
        }
        let eps = 1e-6;
        let signal: Vec<&Pad> = spec
            .land_pattern
            .pads
            .iter()
            .filter(|p| p.role == PadRole::Signal)
            .collect();
        // (coordinate shared by the row, positions along the row)
        let rows = |across: fn(&Pad) -> f64, along: fn(&Pad) -> f64| {
            let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
            for pad in &signal {
                let key = across(pad);
                let pos = along(pad);
                match groups.iter_mut().find(|(k, _)| (k - key).abs() < eps) {
                    Some((_, v)) => {
                        if !v.iter().any(|p| (p - pos).abs() < eps) {
                            v.push(pos);
                        }
                    }
                    None => groups.push((key, vec![pos])),
                }
            }
            groups
                .into_iter()
                .filter(|(_, v)| v.len() >= 3)
                .map(|(_, v)| v.len())
                .collect::<Vec<usize>>()
        };
        let vertical = rows(|p| p.x.get(), |p| p.y.get());
        let horizontal = rows(|p| p.y.get(), |p| p.x.get());
        let found = vertical
            .iter()
            .map(|n| (*n, spec.package.body.length.get()))
            .chain(
                horizontal
                    .iter()
                    .map(|n| (*n, spec.package.body.width.get())),
            );
        for (count, limit) in found {
            let span = (count - 1) as f64 * pitch;
            if span > limit + eps {
                self.add(
                    Check::PitchFit,
                    Level::Error,
                    "package.pitch",
                    format!(
                        "{count} pins in a row at a pitch of {pitch} mm need {span:.3} mm, but the body is {limit} mm"
                    ),
                );
                return; // one finding is enough
            }
        }
    }

    fn exposed_pad(&mut self, spec: &PartSpec) {
        for (i, pad) in spec.land_pattern.pads.iter().enumerate() {
            let area = pad.size_x.get() * pad.size_y.get();
            if pad.paste && area > EXPOSED_PAD_AREA_MM2 {
                self.add(
                    Check::ExposedPad,
                    Level::Warning,
                    format!("land_pattern.pads[{i}].paste"),
                    format!(
                        "pad `{}` is {area:.1} mm2 with one paste area; large exposed pads usually need split paste windows",
                        pad.number
                    ),
                );
            }
        }
    }

    fn overrides(&mut self, spec: &PartSpec) {
        for (i, o) in spec.overrides.iter().enumerate() {
            if !ALL_CHECKS.iter().any(|c| c.id() == o.check) {
                self.add(
                    Check::OverrideUnknown,
                    Level::Warning,
                    format!("overrides[{i}].check"),
                    format!("there is no check called `{}`", o.check),
                );
            }
        }
    }
}

/// How far the pads of the datasheet may be from the calculated ones before a
/// warning is given: outer pad edge, inner pad edge and pad width, in
/// millimetres. The inner edge is loose because manufacturers shorten the pads
/// towards the body; the aim is to catch gross errors such as wrong units.
const TWO_SOURCES_OUTER: f64 = 0.3;
const TWO_SOURCES_INNER: f64 = 0.5;
const TWO_SOURCES_WIDTH: f64 = 0.2;

fn dimension_range(spec: &PartSpec, key: &str) -> Option<ipc::Range> {
    let t = spec.package.dimensions.get(key)?.value;
    Some(ipc::Range::new(
        t.min.map_or(t.nom.get(), |m| m.get()),
        t.max.map_or(t.nom.get(), |m| m.get()),
    ))
}

impl Validator {
    /// When the datasheet gives both package dimensions (`E`, `L`, `b`) and a
    /// land pattern, the IPC-7351 result and the recommended pattern should
    /// agree (two-row gull-wing packages only).
    fn two_sources(&mut self, spec: &PartSpec) {
        let (Some(e), Some(l), Some(b)) = (
            dimension_range(spec, "E"),
            dimension_range(spec, "L"),
            dimension_range(spec, "b"),
        ) else {
            return;
        };
        let pads: Vec<&Pad> = spec
            .land_pattern
            .pads
            .iter()
            .filter(|p| p.role == PadRole::Signal)
            .collect();
        let quarter = spec.package.body.width.get() / 4.0;
        // Only two rows, one on each side (left and right of the body).
        if pads.is_empty() || pads.iter().any(|p| p.x.get().abs() <= quarter) {
            return;
        }
        let input = ipc::GullWingInput {
            lead_span: e,
            lead_length: l,
            lead_width: b,
            pitch: spec.package.pitch.get(),
        };
        let Ok(calc) = ipc::gull_wing(&input, ipc::Density::Nominal, &ipc::Tolerances::default())
        else {
            return;
        };
        let outer = pads
            .iter()
            .map(|p| p.x.get().abs() + p.size_x.get() / 2.0)
            .fold(0.0, f64::max);
        let inner = pads
            .iter()
            .map(|p| p.x.get().abs() - p.size_x.get() / 2.0)
            .fold(f64::MAX, f64::min);
        let width = pads[0].size_y.get();
        let (calc_outer, calc_inner) = (calc.z_max / 2.0, calc.g_min / 2.0);
        let mut differences = Vec::new();
        if (outer - calc_outer).abs() > TWO_SOURCES_OUTER {
            differences.push(format!(
                "outer pad edge {outer:.3} mm, IPC-7351 gives {calc_outer:.3} mm"
            ));
        }
        if (inner - calc_inner).abs() > TWO_SOURCES_INNER {
            differences.push(format!(
                "inner pad edge {inner:.3} mm, IPC-7351 gives {calc_inner:.3} mm"
            ));
        }
        if (width - calc.x_max).abs() > TWO_SOURCES_WIDTH {
            differences.push(format!(
                "pad width {width:.3} mm, IPC-7351 gives {:.3} mm",
                calc.x_max
            ));
        }
        if !differences.is_empty() {
            self.add(
                Check::TwoSources,
                Level::Warning,
                "land_pattern.pads",
                format!(
                    "the land pattern and the package dimensions disagree: {}",
                    differences.join("; ")
                ),
            );
        }
    }
}

impl Validator {
    /// Through-hole pads need a drill smaller than the pad; surface-mount pads
    /// must not have one.
    fn drills(&mut self, spec: &PartSpec) {
        for (i, pad) in spec.land_pattern.pads.iter().enumerate() {
            let path = format!("land_pattern.pads[{i}].drill");
            match (pad.kind, pad.drill) {
                (PadKind::ThruHole, None) => self.add(
                    Check::Drill,
                    Level::Error,
                    path,
                    "a through-hole pad needs a drill",
                ),
                (PadKind::ThruHole, Some(d)) => {
                    let smallest = pad.size_x.get().min(pad.size_y.get());
                    if d.get() <= 0.0 || d.get() >= smallest {
                        self.add(
                            Check::Drill,
                            Level::Error,
                            path,
                            format!(
                                "the drill ({} mm) must be greater than 0 and smaller than the pad ({} mm)",
                                d.get(),
                                smallest
                            ),
                        );
                    }
                }
                (PadKind::Smd, Some(_)) => self.add(
                    Check::Drill,
                    Level::Error,
                    path,
                    "a surface-mount pad cannot have a drill; set kind to thru_hole",
                ),
                (PadKind::Smd, None) => {}
            }
        }
    }
}

impl Validator {
    /// Pads and holes against the other limits of the fab profile: the size of
    /// a surface-mount pad, the drill, the annular ring, the distance between
    /// holes and the solder mask web. The gap between copper features is the
    /// `pad_clearance` check.
    fn fab_limits(&mut self, spec: &PartSpec, fab: &FabProfile) {
        let pads = &spec.land_pattern.pads;
        for (i, pad) in pads.iter().enumerate() {
            let smallest = pad.size_x.get().min(pad.size_y.get());
            match (pad.kind, pad.drill) {
                (PadKind::Smd, _) => {
                    if smallest > 0.0 && smallest < fab.min_pad_size.get() - 1e-9 {
                        self.add(
                            Check::FabLimits,
                            Level::Error,
                            format!("land_pattern.pads[{i}]"),
                            format!(
                                "pad `{}` is {:.3} mm across at its narrowest, below the {:.3} mm smallest pad of fab profile `{}`",
                                pad.number,
                                smallest,
                                fab.min_pad_size.get(),
                                fab.name
                            ),
                        );
                    }
                }
                (PadKind::ThruHole, Some(d)) if d.get() > 0.0 && d.get() < smallest => {
                    let path = format!("land_pattern.pads[{i}].drill");
                    if d.get() < fab.min_drill.get() - 1e-9 {
                        self.add(
                            Check::FabLimits,
                            Level::Error,
                            path.clone(),
                            format!(
                                "the drill of pad `{}` is {:.3} mm, below the {:.3} mm smallest drill of fab profile `{}`",
                                pad.number,
                                d.get(),
                                fab.min_drill.get(),
                                fab.name
                            ),
                        );
                    }
                    let ring = (smallest - d.get()) / 2.0;
                    if ring < fab.min_annular_ring.get() - 1e-9 {
                        self.add(
                            Check::FabLimits,
                            Level::Error,
                            path,
                            format!(
                                "pad `{}` leaves an annular ring of {:.3} mm around its drill, below the {:.3} mm minimum of fab profile `{}`",
                                pad.number,
                                ring,
                                fab.min_annular_ring.get(),
                                fab.name
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
        // Holes: edge to edge.
        for i in 0..pads.len() {
            for j in (i + 1)..pads.len() {
                let (Some(di), Some(dj)) = (pads[i].drill, pads[j].drill) else {
                    continue;
                };
                if pads[i].kind != PadKind::ThruHole || pads[j].kind != PadKind::ThruHole {
                    continue;
                }
                let centres =
                    (pads[i].x.get() - pads[j].x.get()).hypot(pads[i].y.get() - pads[j].y.get());
                let gap = centres - (di.get() + dj.get()) / 2.0;
                if gap < fab.min_hole_to_hole.get() - 1e-9 {
                    self.add(
                        Check::FabLimits,
                        Level::Error,
                        format!("land_pattern.pads[{j}].drill"),
                        format!(
                            "the hole is {:.3} mm from the hole of pad `{}` (pads[{i}]), below the {:.3} mm minimum of fab profile `{}`",
                            gap,
                            pads[i].number,
                            fab.min_hole_to_hole.get(),
                            fab.name
                        ),
                    );
                }
            }
        }
        // Solder mask: a web narrower than the maker's smallest may be lost,
        // which can bridge the pads. Gaps below the copper limit are already
        // reported by `pad_clearance`.
        for i in 0..pads.len() {
            for j in (i + 1)..pads.len() {
                if pads[i].number == pads[j].number {
                    continue;
                }
                let gap = pad_gap(&pads[i], &pads[j]);
                if gap >= fab.min_copper_gap.get() - 1e-9 && gap < fab.min_mask_web.get() - 1e-9 {
                    self.add(
                        Check::FabLimits,
                        Level::Warning,
                        format!("land_pattern.pads[{j}]"),
                        format!(
                            "the solder mask web to pad `{}` (pads[{i}]) is {:.3} mm, below the {:.3} mm smallest web of fab profile `{}`; the maker may remove it",
                            pads[i].number,
                            gap,
                            fab.min_mask_web.get(),
                            fab.name
                        ),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ids_are_unique_and_listed() {
        let mut ids: Vec<&str> = ALL_CHECKS.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len());
    }
}
