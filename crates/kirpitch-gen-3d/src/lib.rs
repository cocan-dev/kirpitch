// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds a simple parametric 3D model of a package from a PartSpec.
//!
//! The model is a union of axis-aligned boxes: the plastic body, a marker for
//! pin 1, and the terminals (gull-wing legs, flat no-lead terminals,
//! through-hole pins and the exposed pad). It is written as VRML (`.wrl`) and
//! as STEP (`.step`, application protocol 214, boundary representation).
//!
//! Model coordinates are in millimetres with x to the right, y up the board and
//! z up from the board surface. Footprint coordinates have y down, so y is
//! flipped. The model has the same origin as the footprint, so no offset is
//! needed when it is linked.

mod step;
mod views;
mod vrml;

use kirpitch_core::partspec::{PadKind, PartSpec};

pub use step::write_step;
pub use views::{View, view_svg};
pub use vrml::write_vrml;

/// Thickness of a terminal.
const LEAD_THICKNESS: f64 = 0.2;
/// Height of the body bottom above the board for gull-wing packages when the
/// drawing has no `A1`.
const DEFAULT_STANDOFF: f64 = 0.1;
/// Height of the body bottom for through-hole packages.
const THT_STANDOFF: f64 = 0.5;
/// How far a through-hole pin reaches below the board.
const THT_PIN_DEPTH: f64 = 3.0;
/// Pin 1 marker size.
const MARKER_SIZE: f64 = 0.5;
/// How much lower the body is than the marker, which is at the maximum height.
const MARKER_DEPTH: f64 = 0.02;

/// What a part of the model is made of, which sets its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    /// Plastic body.
    Body,
    /// Pin 1 marking on the body.
    Marking,
    /// Metal terminals.
    Metal,
}

impl Material {
    /// Colour as red, green, blue between 0 and 1.
    pub fn rgb(self) -> (f64, f64, f64) {
        match self {
            Material::Body => (0.1, 0.1, 0.1),
            Material::Marking => (0.8, 0.8, 0.8),
            Material::Metal => (0.78, 0.78, 0.76),
        }
    }
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cuboid {
    /// Smallest corner (x, y, z).
    pub min: [f64; 3],
    /// Largest corner.
    pub max: [f64; 3],
    /// What it is made of.
    pub material: Material,
}

impl Cuboid {
    fn new(a: [f64; 3], b: [f64; 3], material: Material) -> Cuboid {
        Cuboid {
            min: [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
            max: [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
            material,
        }
    }

    /// Size along each axis.
    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

/// The 3D model: boxes in model coordinates. The first one is the body.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The package name.
    pub name: String,
    /// All boxes.
    pub boxes: Vec<Cuboid>,
}

impl Model {
    /// Smallest and largest corner of everything.
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for b in &self.boxes {
            for i in 0..3 {
                lo[i] = lo[i].min(b.min[i]);
                hi[i] = hi[i].max(b.max[i]);
            }
        }
        (lo, hi)
    }
}

/// Why a model cannot be built.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelError {
    /// The PartSpec has no pads to build terminals from.
    #[error("the land pattern has no pads")]
    NoPads,
}

fn nominal(spec: &PartSpec, key: &str) -> Option<f64> {
    spec.package
        .dimensions
        .get(key)
        .map(|d| d.value.nom.get())
        .filter(|v| *v > 0.0)
}

/// Which side of the body a terminal is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn is_no_lead(spec: &PartSpec) -> bool {
    let family = spec.package.family.to_ascii_uppercase();
    ["QFN", "DFN", "LGA", "SON"]
        .iter()
        .any(|f| family.contains(f))
}

/// Builds the model of a part from its package and land pattern.
///
/// This is an approximation for review and for the board's 3D view: it has the
/// right outline, height, terminal positions and pin 1 corner, but not the
/// exact lead shape of the manufacturer.
pub fn build(spec: &PartSpec) -> Result<Model, ModelError> {
    let pads = &spec.land_pattern.pads;
    if pads.is_empty() {
        return Err(ModelError::NoPads);
    }
    let body = &spec.package.body;
    let (cx, cy) = body.center.map_or((0.0, 0.0), |c| (c.x.get(), c.y.get()));
    let (hx, hy) = (body.width.get() / 2.0, body.length.get() / 2.0);
    let top = body.height_max.get();
    let through_hole = pads.iter().all(|p| p.kind == PadKind::ThruHole);
    let no_lead = is_no_lead(spec);
    let standoff = if through_hole {
        THT_STANDOFF
    } else if no_lead {
        0.0
    } else {
        nominal(spec, "A1").unwrap_or(DEFAULT_STANDOFF)
    }
    .min(top / 2.0);

    // Model y is the footprint y flipped.
    let my = |y: f64| -y;
    let body_top = top - MARKER_DEPTH;
    let mut boxes = vec![Cuboid::new(
        [cx - hx, my(cy) - hy, standoff],
        [cx + hx, my(cy) + hy, body_top],
        Material::Body,
    )];

    // Pin 1 marker in the body corner nearest the first pad.
    if let Some(p1) = pads
        .iter()
        .find(|p| p.number == "1")
        .or_else(|| pads.first())
    {
        let sx = if p1.x.get() <= cx { -1.0 } else { 1.0 };
        let sy = if p1.y.get() <= cy { -1.0 } else { 1.0 };
        let size = MARKER_SIZE.min(hx).min(hy);
        let inset = 0.2_f64.min(hx / 2.0).min(hy / 2.0);
        let edge_x = cx + sx * (hx - inset);
        let edge_y = cy + sy * (hy - inset);
        boxes.push(Cuboid::new(
            [edge_x - sx * size, my(edge_y) + sy * size, body_top],
            [edge_x, my(edge_y), top],
            Material::Marking,
        ));
    }

    for pad in pads {
        let (px, py) = (pad.x.get(), pad.y.get());
        let (sx, sy) = (pad.size_x.get(), pad.size_y.get());
        match pad.kind {
            PadKind::ThruHole => {
                let w = pad.drill.map_or(0.5, |d| (d.get() * 0.5).clamp(0.25, 0.8));
                boxes.push(Cuboid::new(
                    [px - w / 2.0, my(py) - w / 2.0, -THT_PIN_DEPTH],
                    [px + w / 2.0, my(py) + w / 2.0, standoff + 1.0],
                    Material::Metal,
                ));
            }
            PadKind::Smd => {
                let is_exposed = (px - cx).abs() < 0.01
                    && (py - cy).abs() < 0.01
                    && sx >= 1.0
                    && sy >= 1.0
                    && pads.len() > 2;
                if is_exposed {
                    boxes.push(Cuboid::new(
                        [px - sx / 2.0, my(py) - sy / 2.0, 0.0],
                        [px + sx / 2.0, my(py) + sy / 2.0, 0.05],
                        Material::Metal,
                    ));
                    continue;
                }
                let side = side_of(px - cx, py - cy, hx, hy);
                boxes.extend(terminal(&Terminal {
                    spec,
                    side,
                    pos: (px, py),
                    size: (sx, sy),
                    center: (cx, cy),
                    half: (hx, hy),
                    standoff,
                    body_top,
                    no_lead,
                }));
            }
        }
    }
    Ok(Model {
        name: spec.package.name.clone(),
        boxes,
    })
}

fn side_of(dx: f64, dy: f64, hx: f64, hy: f64) -> Side {
    if dx.abs() / hx >= dy.abs() / hy {
        if dx < 0.0 { Side::Left } else { Side::Right }
    } else if dy < 0.0 {
        Side::Top
    } else {
        Side::Bottom
    }
}

/// Inputs for one SMD terminal, in footprint coordinates.
struct Terminal<'a> {
    spec: &'a PartSpec,
    side: Side,
    pos: (f64, f64),
    size: (f64, f64),
    center: (f64, f64),
    half: (f64, f64),
    standoff: f64,
    body_top: f64,
    no_lead: bool,
}

/// The boxes of one SMD terminal.
fn terminal(t: &Terminal) -> Vec<Cuboid> {
    let horizontal = matches!(t.side, Side::Left | Side::Right);
    let sign = if matches!(t.side, Side::Left | Side::Top) {
        -1.0
    } else {
        1.0
    };
    // `out` is the distance from the body centre, away from it.
    let (out_pos, across_pos, pad_len, pad_wid, body_edge) = if horizontal {
        (t.pos.0 - t.center.0, t.pos.1, t.size.0, t.size.1, t.half.0)
    } else {
        (t.pos.1 - t.center.1, t.pos.0, t.size.1, t.size.0, t.half.1)
    };
    let out_pos = out_pos * sign;
    let width = nominal(t.spec, "b")
        .map(|b| b.min(pad_wid))
        .unwrap_or(pad_wid * 0.6);
    let pad_outer = out_pos + pad_len / 2.0;
    // Boxes as (out_min, out_max, z_min, z_max).
    let mut parts: Vec<(f64, f64, f64, f64)> = Vec::new();
    if t.no_lead {
        let length = nominal(t.spec, "L").unwrap_or(pad_len * 0.5);
        parts.push((body_edge - length, body_edge, 0.0, LEAD_THICKNESS.min(0.15)));
    } else {
        let foot = nominal(t.spec, "L").unwrap_or(pad_len * 0.6);
        let tip = (pad_outer - 0.3).max(body_edge + foot.min(pad_len));
        let foot_in = (tip - foot).max(body_edge + LEAD_THICKNESS).min(tip - 0.05);
        let shoulder_z = t.standoff + (t.body_top - t.standoff) * 0.5;
        parts.push((foot_in, tip, 0.0, LEAD_THICKNESS));
        parts.push((foot_in, foot_in + LEAD_THICKNESS, 0.0, shoulder_z));
        parts.push((
            body_edge - 0.05,
            foot_in + LEAD_THICKNESS,
            shoulder_z - LEAD_THICKNESS,
            shoulder_z,
        ));
    }
    parts
        .into_iter()
        .map(|(o0, o1, z0, z1)| {
            let (a0, a1) = (across_pos - width / 2.0, across_pos + width / 2.0);
            let (lo, hi) = if sign < 0.0 { (-o1, -o0) } else { (o0, o1) };
            if horizontal {
                Cuboid::new(
                    [t.center.0 + lo, -a1, z0],
                    [t.center.0 + hi, -a0, z1],
                    Material::Metal,
                )
            } else {
                // Footprint y is centre + lo..hi; model y is flipped.
                Cuboid::new(
                    [a0, -(t.center.1 + hi), z0],
                    [a1, -(t.center.1 + lo), z1],
                    Material::Metal,
                )
            }
        })
        .collect()
}

/// A problem found when the model is compared with the footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct FitIssue {
    /// What is wrong, in words.
    pub message: String,
}

/// Compares the model with the footprint and the datasheet limits (the "3D
/// fit" check of the requirements): every SMD pad has a terminal above it, the
/// body is inside the courtyard, the height stays within the maximum, and pin 1
/// is in the same corner as in the footprint.
pub fn check_fit(spec: &PartSpec, model: &Model) -> Vec<FitIssue> {
    let mut issues = Vec::new();
    let (_, hi) = model.bounds();
    let max_h = spec.package.body.height_max.get();
    if hi[2] > max_h + 1e-6 {
        issues.push(FitIssue {
            message: format!(
                "the model is {:.3} mm high, above the maximum {:.3} mm",
                hi[2], max_h
            ),
        });
    }
    let (cx, cy) = spec
        .package
        .body
        .center
        .map_or((0.0, 0.0), |c| (c.x.get(), c.y.get()));
    if let (Some(r), Some(b)) = (spec.land_pattern.courtyard, model.boxes.first()) {
        let (by0, by1) = (-b.max[1], -b.min[1]);
        if b.min[0] < r.x_min.get() - 1e-6
            || b.max[0] > r.x_max.get() + 1e-6
            || by0 < r.y_min.get() - 1e-6
            || by1 > r.y_max.get() + 1e-6
        {
            issues.push(FitIssue {
                message: "the body is not inside the courtyard".into(),
            });
        }
    }
    for pad in &spec.land_pattern.pads {
        if pad.kind != PadKind::Smd {
            continue;
        }
        let (px, py) = (pad.x.get(), -pad.y.get());
        let covered = model.boxes.iter().skip(1).any(|b| {
            b.material == Material::Metal
                && b.min[2] <= 1e-9
                && px >= b.min[0] - pad.size_x.get() / 2.0
                && px <= b.max[0] + pad.size_x.get() / 2.0
                && py >= b.min[1] - pad.size_y.get() / 2.0
                && py <= b.max[1] + pad.size_y.get() / 2.0
        });
        if !covered {
            issues.push(FitIssue {
                message: format!("pad {} has no terminal above it", pad.number),
            });
        }
    }
    if let (Some(p1), Some(marker)) = (
        spec.land_pattern.pads.iter().find(|p| p.number == "1"),
        model.boxes.iter().find(|b| b.material == Material::Marking),
    ) {
        let mx = (marker.min[0] + marker.max[0]) / 2.0 - cx;
        let my = -((marker.min[1] + marker.max[1]) / 2.0) - cy;
        let same_x = (mx < 0.0) == (p1.x.get() - cx < 0.0);
        let same_y = (my < 0.0) == (p1.y.get() - cy < 0.0);
        if !(same_x && same_y) {
            issues.push(FitIssue {
                message: "pin 1 is in a different corner in the model and in the footprint".into(),
            });
        }
    }
    issues
}

/// The file names of the STEP and VRML files of a part.
pub fn file_names(spec: &PartSpec) -> (String, String) {
    (
        format!("{}.step", spec.package.name),
        format!("{}.wrl", spec.package.name),
    )
}

/// The result of [`check_fit`] as findings of the `model_fit` check, ready to
/// be listed with the findings of [`kirpitch_core::validate`]. They are errors.
pub fn fit_findings(spec: &PartSpec) -> Vec<kirpitch_core::Finding> {
    use kirpitch_core::{Check, Finding, Level};
    let issues = match build(spec) {
        Ok(model) => check_fit(spec, &model),
        Err(e) => vec![FitIssue {
            message: e.to_string(),
        }],
    };
    issues
        .into_iter()
        .map(|i| Finding {
            check: Check::ModelFit,
            level: Level::Error,
            path: "land_pattern".to_string(),
            message: i.message,
            overridden_by: None,
        })
        .collect()
}
