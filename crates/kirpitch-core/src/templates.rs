// SPDX-License-Identifier: GPL-3.0-or-later

//! Package templates: a package family plus the dimension letters of the
//! datasheet give a complete [`Package`] and [`LandPattern`].
//!
//! This is the core of the no-LLM mode (requirements, "Package templates"): the
//! user picks a family and fills in the standard JEDEC letters (E, E1, D, A, L,
//! b, e); the land pattern is calculated after IPC-7351B (see [`crate::ipc`]).
//! Pins are not part of a template; they come from the pin table.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::ipc::{self, Density, IpcError, Range, Tolerances};
use crate::partspec::{Body, LandPattern, Package, Pad, PadRole, PadShape};
use crate::source::Sourced;
use crate::units::{Mm, Tolerance, format_number};

/// Largest corner radius of a rounded pad, as a length (KLC).
const MAX_CORNER_RADIUS: f64 = 0.25;
/// Corner radius of a rounded pad as a fraction of its shorter side (KLC).
const CORNER_RATIO: f64 = 0.25;

/// A package with two rows of gull-wing leads (SOIC, SOP, SSOP, TSSOP, MSOP).
#[derive(Debug, Clone, PartialEq)]
pub struct DualRowGullWing {
    /// Family name used in the footprint name, for example `SOIC` or `TSSOP`.
    pub family: String,
    /// Number of leads, even and at least 4.
    pub pin_count: u32,
    /// Distance between lead centres (`e`).
    pub pitch: f64,
    /// Body length along the rows (`D`).
    pub body_length: f64,
    /// Body width across the rows, without leads (`E1`).
    pub body_width: f64,
    /// Largest height above the board (`A`).
    pub height_max: f64,
    /// Overall width over the lead tips (`E`).
    pub lead_span: Range,
    /// Foot length (`L`).
    pub lead_length: Range,
    /// Lead width (`b`).
    pub lead_width: Range,
}

/// The result of a template: package and land pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// The package with its name and the dimensions that were given.
    pub package: Package,
    /// The pads, numbered like the pins of the package.
    pub land_pattern: LandPattern,
}

/// Reasons a template cannot be filled.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum TemplateError {
    /// The pin count is odd or too small for two rows.
    #[error("a dual-row package needs an even pin count of at least 4, found {0}")]
    PinCount(u32),
    /// A length is zero, negative or not a finite number.
    #[error("`{0}` must be a positive number")]
    BadValue(&'static str),
    /// The land pattern calculation rejected the lead dimensions.
    #[error(transparent)]
    Ipc(#[from] IpcError),
}

fn mm(value: f64, name: &'static str) -> Result<Mm, TemplateError> {
    if value > 0.0 {
        Mm::new(value).map_err(|_| TemplateError::BadValue(name))
    } else {
        Err(TemplateError::BadValue(name))
    }
}

fn dimension(range: Range) -> Result<Sourced<Tolerance>, TemplateError> {
    let nom = (range.min + range.max) / 2.0;
    let m = |v: f64| Mm::new(v).map_err(|_| TemplateError::BadValue("dimension"));
    Ok(Sourced::unsourced(Tolerance {
        min: (range.min != range.max).then(|| m(range.min)).transpose()?,
        nom: m(nom)?,
        max: (range.min != range.max).then(|| m(range.max)).transpose()?,
    }))
}

/// Fills the template of a dual-row gull-wing package.
///
/// Pad numbers run down the left side (pin 1 at the top left) and up the right
/// side, as on every JEDEC package. Rows are on the left and right of the
/// body, pins along the y axis, with y pointing down as in KiCad footprints.
pub fn dual_row_gull_wing(
    p: &DualRowGullWing,
    density: Density,
) -> Result<Template, TemplateError> {
    if p.pin_count < 4 || !p.pin_count.is_multiple_of(2) {
        return Err(TemplateError::PinCount(p.pin_count));
    }
    let per_side = (p.pin_count / 2) as usize;
    let first = -(per_side as f64 - 1.0) / 2.0;
    let mut slots = Vec::with_capacity(p.pin_count as usize);
    for i in 0..per_side {
        slots.push((-1.0, first + i as f64));
    }
    for i in 0..per_side {
        slots.push((1.0, -first - i as f64));
    }
    let name = format!(
        "{}-{}_{}x{}mm_P{}mm",
        p.family,
        p.pin_count,
        format_number(p.body_width),
        format_number(p.body_length),
        format_number(p.pitch)
    );
    build(p, density, &slots, name)
}

/// A SOT-23 package with 3, 5 or 6 leads (JEDEC TO-236 and MO-178).
///
/// The leads sit on a grid of three positions along each side, as in KiCad's
/// footprints: pins 1 to 3 down the left side, the others up the right side.
/// `pin_count` must be 3, 5 or 6. The footprint is named `SOT-23` for three
/// leads (the family name) and `SOT-23-5` or `SOT-23-6` otherwise.
pub fn sot23(p: &DualRowGullWing, density: Density) -> Result<Template, TemplateError> {
    // (side, row in pitches from the centre along y), in pin order.
    let slots: Vec<(f64, f64)> = match p.pin_count {
        3 => vec![(-1.0, -1.0), (-1.0, 1.0), (1.0, 0.0)],
        5 => vec![
            (-1.0, -1.0),
            (-1.0, 0.0),
            (-1.0, 1.0),
            (1.0, 1.0),
            (1.0, -1.0),
        ],
        6 => vec![
            (-1.0, -1.0),
            (-1.0, 0.0),
            (-1.0, 1.0),
            (1.0, 1.0),
            (1.0, 0.0),
            (1.0, -1.0),
        ],
        n => return Err(TemplateError::PinCount(n)),
    };
    let name = if p.pin_count == 3 {
        p.family.clone()
    } else {
        format!("{}-{}", p.family, p.pin_count)
    };
    build(p, density, &slots, name)
}

/// Builds package and pads for pins at the given `(side, row)` slots, numbered
/// from 1 in the order of the slots. `side` is -1 for the left and 1 for the
/// right; `row` counts pitches from the centre of the body along y.
fn build(
    p: &DualRowGullWing,
    density: Density,
    slots: &[(f64, f64)],
    name: String,
) -> Result<Template, TemplateError> {
    let pitch = mm(p.pitch, "pitch")?;
    let body = Body {
        length: mm(p.body_length, "body_length")?,
        width: mm(p.body_width, "body_width")?,
        height_max: mm(p.height_max, "height_max")?,
    };
    let calc = ipc::gull_wing(
        &ipc::GullWingInput {
            lead_span: p.lead_span,
            lead_length: p.lead_length,
            lead_width: p.lead_width,
            pitch: p.pitch,
        },
        density,
        &Tolerances::default(),
    )?;

    let ratio = CORNER_RATIO.min(MAX_CORNER_RADIUS / calc.x_max.min(calc.pad_length));
    let mut pads = Vec::with_capacity(slots.len());
    for (i, (side, row)) in slots.iter().enumerate() {
        pads.push(Pad {
            number: (i + 1).to_string(),
            shape: PadShape::RoundRect,
            x: Mm::new(side * calc.pad_center).map_err(|_| TemplateError::BadValue("pad"))?,
            y: Mm::new(row * p.pitch).map_err(|_| TemplateError::BadValue("pad"))?,
            size_x: Mm::new(calc.pad_length).map_err(|_| TemplateError::BadValue("pad"))?,
            size_y: Mm::new(calc.x_max).map_err(|_| TemplateError::BadValue("pad"))?,
            rotation_deg: 0.0,
            corner_ratio: Some(ratio),
            paste: true,
            role: PadRole::Signal,
            source: None,
        });
    }

    let mut dimensions: BTreeMap<String, Sourced<Tolerance>> = BTreeMap::new();
    dimensions.insert("A".into(), dimension(Range::exact(p.height_max))?);
    dimensions.insert("D".into(), dimension(Range::exact(p.body_length))?);
    dimensions.insert("E".into(), dimension(p.lead_span)?);
    dimensions.insert("E1".into(), dimension(Range::exact(p.body_width))?);
    dimensions.insert("L".into(), dimension(p.lead_length)?);
    dimensions.insert("b".into(), dimension(p.lead_width)?);
    dimensions.insert("e".into(), dimension(Range::exact(p.pitch))?);

    Ok(Template {
        package: Package {
            family: p.family.clone(),
            name,
            pin_count: p.pin_count,
            pitch,
            body,
            dimensions,
            source: None,
        },
        land_pattern: LandPattern {
            pads,
            courtyard: None,
            source: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JEDEC MS-012 (SOIC, 3.9 mm body).
    fn soic(pin_count: u32, body_length: f64) -> DualRowGullWing {
        DualRowGullWing {
            family: "SOIC".into(),
            pin_count,
            pitch: 1.27,
            body_length,
            body_width: 3.9,
            height_max: 1.75,
            lead_span: Range::new(5.8, 6.2),
            lead_length: Range::new(0.4, 1.27),
            lead_width: Range::new(0.31, 0.51),
        }
    }

    fn at(t: &Template, number: &str) -> (f64, f64) {
        let p = t
            .land_pattern
            .pads
            .iter()
            .find(|p| p.number == number)
            .unwrap();
        (p.x.rounded(), p.y.rounded())
    }

    #[test]
    fn soic8_has_the_official_name_and_pads() {
        let t = dual_row_gull_wing(&soic(8, 4.9), Density::Nominal).unwrap();
        assert_eq!(t.package.name, "SOIC-8_3.9x4.9mm_P1.27mm");
        assert_eq!(t.land_pattern.pads.len(), 8);
        assert_eq!(at(&t, "1"), (-2.475, -1.905));
        assert_eq!(at(&t, "4"), (-2.475, 1.905));
        assert_eq!(at(&t, "5"), (2.475, 1.905));
        assert_eq!(at(&t, "8"), (2.475, -1.905));
        let p = &t.land_pattern.pads[0];
        assert_eq!(
            (p.size_x.format().as_str(), p.size_y.format().as_str()),
            ("1.95", "0.6")
        );
        assert_eq!(p.corner_ratio, Some(0.25));
    }

    #[test]
    fn soic16_pins_are_centred_on_the_body() {
        let t = dual_row_gull_wing(&soic(16, 9.9), Density::Nominal).unwrap();
        assert_eq!(t.package.name, "SOIC-16_3.9x9.9mm_P1.27mm");
        // KiCad: pad 1 at y = -4.445.
        assert!((at(&t, "1").1 - -4.445).abs() < 1e-9);
        assert!((at(&t, "8").1 - 4.445).abs() < 1e-9);
        assert!((at(&t, "16").1 - -4.445).abs() < 1e-9);
    }

    #[test]
    fn tssop14_matches_the_official_footprint() {
        let t = dual_row_gull_wing(
            &DualRowGullWing {
                family: "TSSOP".into(),
                pin_count: 14,
                pitch: 0.65,
                body_length: 5.0,
                body_width: 4.4,
                height_max: 1.2,
                lead_span: Range::exact(6.4),
                lead_length: Range::new(0.45, 0.75),
                lead_width: Range::new(0.17, 0.30),
            },
            Density::Nominal,
        )
        .unwrap();
        assert_eq!(t.package.name, "TSSOP-14_4.4x5mm_P0.65mm");
        assert_eq!(at(&t, "1"), (-2.8625, -1.95));
        let p = &t.land_pattern.pads[0];
        assert_eq!(
            (p.size_x.format().as_str(), p.size_y.format().as_str()),
            ("1.475", "0.4")
        );
    }

    #[test]
    fn dimensions_keep_their_tolerances() {
        let t = dual_row_gull_wing(&soic(8, 4.9), Density::Nominal).unwrap();
        let l = t.package.dimensions["L"].value;
        assert_eq!(l.min.unwrap().format(), "0.4");
        assert_eq!(l.max.unwrap().format(), "1.27");
        assert!(l.is_ordered());
        let e1 = t.package.dimensions["E1"].value;
        assert!(e1.min.is_none() && e1.max.is_none());
    }

    #[test]
    fn bad_input_is_rejected() {
        assert_eq!(
            dual_row_gull_wing(&soic(7, 4.9), Density::Nominal),
            Err(TemplateError::PinCount(7))
        );
        assert_eq!(
            dual_row_gull_wing(&soic(2, 4.9), Density::Nominal),
            Err(TemplateError::PinCount(2))
        );
        assert_eq!(
            dual_row_gull_wing(&soic(8, -1.0), Density::Nominal),
            Err(TemplateError::BadValue("body_length"))
        );
        let mut bad = soic(8, 4.9);
        bad.lead_width = Range::new(0.5, 0.3);
        assert!(matches!(
            dual_row_gull_wing(&bad, Density::Nominal),
            Err(TemplateError::Ipc(_))
        ));
    }
}
