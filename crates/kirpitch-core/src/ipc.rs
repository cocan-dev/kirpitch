// SPDX-License-Identifier: GPL-3.0-or-later

//! Land pattern calculation for gull-wing leads after IPC-7351B.
//!
//! IPC-7351 (the IPC standard for land patterns) computes the outer edge `Z`,
//! the inner edge `G` and the width `X` of a pad from the lead dimensions of the
//! package and solder fillet goals for the toe, heel and side of a lead:
//!
//! ```text
//! Zmax = Lmin + 2*Jt + sqrt(CL^2 + F^2 + P^2)
//! Gmin = Smax - 2*Jh - sqrt(CS^2 + F^2 + P^2)
//! Xmax = Wmin + 2*Js + sqrt(CW^2 + F^2 + P^2)
//! ```
//!
//! `L` is the overall width over the leads, `S` the span between the lead tips
//! (inside), `W` the lead width; `C` is the tolerance of each, `F` the board
//! fabrication tolerance and `P` the placement tolerance. The fillet goals
//! depend on the density level and on the pitch. The datasheet gives the lead
//! length `T`, so the span is derived from `L - 2*T`, and its tolerance is the
//! root of the sum of squares of the tolerances of `L` and `2T`.
//!
//! The numbers (fillet goals, rounding to 0.05 mm, F = 0.1 mm, P = 0.05 mm) are
//! the ones that the official KiCad footprint generator documents for
//! IPC-7351B; the tests compare the result with the official KiCad SOIC and
//! TSSOP footprints. This code is an independent implementation of the
//! formulas.

use thiserror::Error;

/// Density level of a land pattern: how much solder margin the pads give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    /// Level C, the smallest pads.
    Least,
    /// Level B, the usual choice.
    #[default]
    Nominal,
    /// Level A, the largest pads, for harsh environments.
    Most,
}

/// A dimension given as a range, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    /// Smallest value.
    pub min: f64,
    /// Largest value.
    pub max: f64,
}

impl Range {
    /// A range with min and max.
    pub fn new(min: f64, max: f64) -> Range {
        Range { min, max }
    }

    /// A dimension without tolerance.
    pub fn exact(value: f64) -> Range {
        Range {
            min: value,
            max: value,
        }
    }

    fn tolerance(self) -> f64 {
        self.max - self.min
    }
}

/// The lead dimensions of a gull-wing package.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GullWingInput {
    /// Overall width over the lead tips (`L`, for SOIC the dimension E).
    pub lead_span: Range,
    /// Length of the flat foot (`T`).
    pub lead_length: Range,
    /// Width of a lead (`W`).
    pub lead_width: Range,
    /// Distance between lead centres.
    pub pitch: f64,
}

/// Manufacturing tolerances used by the formulas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerances {
    /// Board fabrication tolerance `F`.
    pub fabrication: f64,
    /// Component placement tolerance `P`.
    pub placement: f64,
}

impl Default for Tolerances {
    fn default() -> Tolerances {
        Tolerances {
            fabrication: 0.1,
            placement: 0.05,
        }
    }
}

/// The calculated pads of a two-row gull-wing package.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GullWingPads {
    /// Outer distance between the two pad rows (`Zmax`).
    pub z_max: f64,
    /// Inner distance between the two pad rows (`Gmin`).
    pub g_min: f64,
    /// Pad width (`Xmax`).
    pub x_max: f64,
    /// Distance from the centre of the package to the centre of a pad.
    pub pad_center: f64,
    /// Pad length, across the row (from `G` to `Z`).
    pub pad_length: f64,
    /// Distance from the pads and body to the courtyard at this density.
    pub courtyard_margin: f64,
}

/// Reasons a land pattern cannot be calculated.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IpcError {
    /// A dimension is zero, negative or has min greater than max.
    #[error("dimension `{0}` must be positive and min must not exceed max")]
    BadDimension(&'static str),
}

#[derive(Clone, Copy)]
struct FilletGoals {
    toe: f64,
    heel: f64,
    side: f64,
    courtyard: f64,
}

const fn goal(toe: f64, heel: f64, side: f64, courtyard: f64) -> FilletGoals {
    FilletGoals {
        toe,
        heel,
        side,
        courtyard,
    }
}

/// Fillet goals for gull-wing leads. Pitches of 0.625 mm and above use the
/// "large pitch" table, as the KiCad generator does.
fn goals(density: Density, pitch: f64) -> FilletGoals {
    let large = pitch >= 0.625;
    match (density, large) {
        (Density::Least, true) => goal(0.15, 0.25, 0.01, 0.1),
        (Density::Nominal, true) => goal(0.35, 0.35, 0.03, 0.25),
        (Density::Most, true) => goal(0.55, 0.45, 0.05, 0.5),
        (Density::Least, false) => goal(0.15, 0.25, -0.04, 0.1),
        (Density::Nominal, false) => goal(0.35, 0.35, -0.02, 0.25),
        (Density::Most, false) => goal(0.55, 0.45, 0.01, 0.5),
    }
}

/// Rounds to a multiple of `base`; halves go to the even multiple.
fn round_to(value: f64, base: f64) -> f64 {
    (value / base).round_ties_even() * base
}

fn check(name: &'static str, r: Range) -> Result<(), IpcError> {
    if r.min > 0.0 && r.max >= r.min && r.max.is_finite() {
        Ok(())
    } else {
        Err(IpcError::BadDimension(name))
    }
}

/// Calculates the pads of a two-row gull-wing package (SOIC, SOP, TSSOP, SSOP,
/// SOT-23 style leads).
pub fn gull_wing(
    input: &GullWingInput,
    density: Density,
    tol: &Tolerances,
) -> Result<GullWingPads, IpcError> {
    check("lead_span", input.lead_span)?;
    check("lead_length", input.lead_length)?;
    check("lead_width", input.lead_width)?;
    if input.pitch <= 0.0 || !input.pitch.is_finite() {
        return Err(IpcError::BadDimension("pitch"));
    }
    let g = goals(density, input.pitch);
    let (f, p) = (tol.fabrication, tol.placement);

    // Inside span S = L - 2T with the statistical (root sum of squares)
    // tolerance of the two terms.
    let l = input.lead_span;
    let t = input.lead_length;
    let two_t_tol_rms = t.tolerance() * 2f64.sqrt();
    let s_min = l.min - 2.0 * t.max;
    let s_max = l.max - 2.0 * t.min;
    let s_tol = s_max - s_min;
    let s_tol_rms = (l.tolerance().powi(2) + two_t_tol_rms.powi(2))
        .sqrt()
        .min(s_tol);
    let s_max_rms = s_max - (s_tol - s_tol_rms) / 2.0;

    let z = l.min + 2.0 * g.toe + (l.tolerance().powi(2) + f * f + p * p).sqrt();
    let gap = s_max_rms - 2.0 * g.heel - (s_tol_rms.powi(2) + f * f + p * p).sqrt();
    let w = input.lead_width;
    let x = w.min + 2.0 * g.side + (w.tolerance().powi(2) + f * f + p * p).sqrt();

    let base = 0.05;
    let z_max = round_to(z, base);
    let g_min = round_to(gap, base);
    let x_max = round_to(x, base);
    Ok(GullWingPads {
        z_max,
        g_min,
        x_max,
        pad_center: (z_max + g_min) / 4.0,
        pad_length: (z_max - g_min) / 2.0,
        courtyard_margin: g.courtyard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// JEDEC MS-012 (SOIC 3.9 mm): L 5.8 to 6.2, T 0.4 to 1.27, W 0.31 to 0.51.
    /// KiCad has pads of 1.95 x 0.6 at x = +-2.475 for SOIC-8_3.9x4.9mm_P1.27mm.
    #[test]
    fn soic_narrow_matches_the_official_kicad_footprint() {
        let pads = gull_wing(
            &GullWingInput {
                lead_span: Range::new(5.8, 6.2),
                lead_length: Range::new(0.4, 1.27),
                lead_width: Range::new(0.31, 0.51),
                pitch: 1.27,
            },
            Density::Nominal,
            &Tolerances::default(),
        )
        .unwrap();
        assert!(close(pads.z_max, 6.9), "{pads:?}");
        assert!(close(pads.g_min, 3.0), "{pads:?}");
        assert!(close(pads.pad_length, 1.95), "{pads:?}");
        assert!(close(pads.pad_center, 2.475), "{pads:?}");
        assert!(close(pads.x_max, 0.6), "{pads:?}");
        assert!(close(pads.courtyard_margin, 0.25));
    }

    /// JEDEC MO-153 (TSSOP 4.4 mm): L 6.4, T 0.45 to 0.75, W 0.17 to 0.30,
    /// pitch 0.65. KiCad has pads of 1.475 x 0.4 at x = +-2.8625 for TSSOP-14.
    #[test]
    fn tssop_matches_the_official_kicad_footprint() {
        let pads = gull_wing(
            &GullWingInput {
                lead_span: Range::exact(6.4),
                lead_length: Range::new(0.45, 0.75),
                lead_width: Range::new(0.17, 0.30),
                pitch: 0.65,
            },
            Density::Nominal,
            &Tolerances::default(),
        )
        .unwrap();
        assert!(close(pads.pad_length, 1.475), "{pads:?}");
        assert!(close(pads.pad_center, 2.8625), "{pads:?}");
        assert!(close(pads.x_max, 0.4), "{pads:?}");
    }

    #[test]
    fn denser_levels_give_bigger_pads() {
        let input = GullWingInput {
            lead_span: Range::new(5.8, 6.2),
            lead_length: Range::new(0.4, 1.27),
            lead_width: Range::new(0.31, 0.51),
            pitch: 1.27,
        };
        let tol = Tolerances::default();
        let least = gull_wing(&input, Density::Least, &tol).unwrap();
        let nominal = gull_wing(&input, Density::Nominal, &tol).unwrap();
        let most = gull_wing(&input, Density::Most, &tol).unwrap();
        assert!(least.pad_length < nominal.pad_length);
        assert!(nominal.pad_length < most.pad_length);
        assert!(least.x_max <= nominal.x_max && nominal.x_max <= most.x_max);
        assert!(least.courtyard_margin < most.courtyard_margin);
    }

    #[test]
    fn fine_pitch_uses_its_own_side_goal() {
        assert_eq!(goals(Density::Nominal, 0.65).side, 0.03);
        assert_eq!(goals(Density::Nominal, 0.5).side, -0.02);
        assert_eq!(goals(Density::Most, 0.4).side, 0.01);
    }

    #[test]
    fn bad_dimensions_are_rejected() {
        let ok = GullWingInput {
            lead_span: Range::new(5.8, 6.2),
            lead_length: Range::new(0.4, 1.27),
            lead_width: Range::new(0.31, 0.51),
            pitch: 1.27,
        };
        let tol = Tolerances::default();
        let mut bad = ok;
        bad.lead_span = Range::new(6.2, 5.8);
        assert_eq!(
            gull_wing(&bad, Density::Nominal, &tol),
            Err(IpcError::BadDimension("lead_span"))
        );
        bad = ok;
        bad.pitch = 0.0;
        assert_eq!(
            gull_wing(&bad, Density::Nominal, &tol),
            Err(IpcError::BadDimension("pitch"))
        );
        bad = ok;
        bad.lead_width = Range::new(-1.0, 0.5);
        assert!(gull_wing(&bad, Density::Nominal, &tol).is_err());
    }

    #[test]
    fn rounding_goes_to_the_even_multiple_on_halves() {
        assert!(close(round_to(0.25, 0.5), 0.0)); // 0.5 -> 0
        assert!(close(round_to(0.75, 0.5), 1.0)); // 1.5 -> 2
    }
}
